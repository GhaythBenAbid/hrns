//! Usage ledger for the Usage page.
//!
//! Two sources, never overlapping:
//! - Codex rollout transcripts (`<CODEX_HOME>/sessions/**/rollout-*.jsonl`) written by the
//!   CLI, IDE and Codex app. Parsed like T3 Code does: one record per `token_count`
//!   request, de-duplicated with Codex's cumulative counters, fork copies skipped.
//!   Scans are incremental (byte offset per session) and cached, so only new lines are read.
//! - hrns' own chats, which run as ephemeral threads (no transcript) and are logged
//!   to `usage-hrns.jsonl` as each turn finishes.
//!
//! Records are bucketed by UTC hour so the UI can group by the user's local day.

use crate::store::Usage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const CACHE_VERSION: u32 = 1;
/// Forks copy their parent's history in a quick burst; that usage is billed in the parent.
const FORK_COPY_GAP_MS: i64 = 1000;

/// [input (incl. cached), cached, output (incl. reasoning), reasoning, requests]
type Tokens = [u64; 5];

#[derive(Default, Serialize, Deserialize)]
struct ScanState {
    model: String,
    saw_meta: bool,
    fork_anchor_ms: i64,
    last_cumulative: Option<[u64; 4]>,
    last_signature: Option<[u64; 4]>,
}

#[derive(Default, Serialize, Deserialize)]
struct FileEntry {
    path: String,
    size: u64,
    offset: u64,
    state: ScanState,
    /// "YYYY-MM-DDTHH|model" -> tokens
    buckets: HashMap<String, Tokens>,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    version: u32,
    /// Keyed by session file name, so a rollout moved to archived_sessions keeps its progress.
    files: HashMap<String, FileEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// UTC hour, "YYYY-MM-DDTHH".
    pub hour: String,
    /// "codex" for transcripts, otherwise the hrns account id.
    pub source: String,
    pub model: String,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub reasoning: u64,
    pub requests: u64,
}

pub struct UsageLedger {
    cache_path: PathBuf,
    ledger_path: PathBuf,
    lock: Mutex<()>,
}

impl UsageLedger {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            cache_path: data_dir.join("usage-scan-cache.json"),
            ledger_path: data_dir.join("usage-hrns.jsonl"),
            lock: Mutex::new(()),
        }
    }

    /// Record one finished hrns turn.
    pub fn log(&self, account: &str, model: &str, u: &Usage) {
        if u.input + u.output == 0 {
            return;
        }
        let _guard = self.lock.lock().unwrap();
        let line = serde_json::json!({
            "ts": crate::store::now_ms(), "account": account, "model": model,
            "input": u.input, "cached": u.cached, "output": u.output, "reasoning": u.reasoning,
        });
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.ledger_path) {
            let _ = writeln!(f, "{line}");
        }
    }

    /// All usage since `since_hour` ("YYYY-MM-DDTHH", UTC). Scans Codex transcripts incrementally.
    pub fn report(&self, since_hour: &str) -> Vec<Row> {
        let _guard = self.lock.lock().unwrap();
        let mut cache: Cache = std::fs::read(&self.cache_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .filter(|c: &Cache| c.version == CACHE_VERSION)
            .unwrap_or(Cache { version: CACHE_VERSION, ..Default::default() });

        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            for dir in ["sessions", "archived_sessions"] {
                let mut files = vec![];
                collect_rollouts(&home.join(".codex").join(dir), &mut files);
                for path in files {
                    scan_file(&mut cache, &path);
                }
            }
        }
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let tmp = self.cache_path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(tmp, &self.cache_path);
            }
        }

        let mut out: HashMap<(String, String, String), Tokens> = HashMap::new();
        for entry in cache.files.values() {
            for (key, t) in &entry.buckets {
                let Some((hour, model)) = key.split_once('|') else { continue };
                if hour < since_hour {
                    continue;
                }
                add(out.entry((hour.to_owned(), "codex".into(), model.to_owned())).or_default(), t);
            }
        }
        if let Ok(f) = File::open(&self.ledger_path) {
            for line in BufReader::new(f).lines().map_while(Result::ok) {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                let hour = utc_hour_from_ms(v["ts"].as_i64().unwrap_or(0));
                if hour.as_str() < since_hour {
                    continue;
                }
                let n = |k: &str| v[k].as_u64().unwrap_or(0);
                let t = [n("input"), n("cached"), n("output"), n("reasoning"), 1];
                let source = v["account"].as_str().unwrap_or("default").to_owned();
                let model = v["model"].as_str().unwrap_or("unknown").to_owned();
                add(out.entry((hour, source, model)).or_default(), &t);
            }
        }

        out.into_iter()
            .map(|((hour, source, model), t)| Row {
                hour,
                source,
                model,
                input: t[0],
                cached: t[1],
                output: t[2],
                reasoning: t[3],
                requests: t[4],
            })
            .collect()
    }
}

fn add(acc: &mut Tokens, t: &Tokens) {
    for i in 0..5 {
        acc[i] += t[i];
    }
}

fn collect_rollouts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rollouts(&p, out);
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl")) {
            out.push(p);
        }
    }
}

/// Read only the complete lines appended since the last scan.
fn scan_file(cache: &mut Cache, path: &Path) {
    let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { return };
    let Ok(size) = std::fs::metadata(path).map(|m| m.len()) else { return };
    let entry = cache.files.entry(name).or_default();
    if size < entry.offset {
        // Rewritten or truncated: start over.
        *entry = FileEntry::default();
    }
    entry.path = path.display().to_string();
    if size == entry.offset {
        entry.size = size;
        return;
    }
    let Ok(mut f) = File::open(path) else { return };
    if f.seek(SeekFrom::Start(entry.offset)).is_err() {
        return;
    }
    let mut buf = Vec::with_capacity((size - entry.offset) as usize);
    if f.take(size - entry.offset).read_to_end(&mut buf).is_err() {
        return;
    }
    // Stop at the last newline; a partial line is re-read next time.
    let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return };
    for line in buf[..=end].split(|&b| b == b'\n') {
        parse_line(entry, line);
    }
    entry.offset += end as u64 + 1;
    entry.size = size;
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn parse_line(entry: &mut FileEntry, line: &[u8]) {
    // Cheap prefilter: most lines are messages/tool output we don't need to parse.
    let interesting = contains(line, b"\"token_count\"") || contains(line, b"\"turn_context\"") || contains(line, b"\"session_meta\"");
    if !interesting {
        return;
    }
    let Ok(v) = serde_json::from_slice::<Value>(line) else { return };
    let p = &v["payload"];
    let st = &mut entry.state;
    let ts_str = v["timestamp"].as_str().unwrap_or_default();

    match (v["type"].as_str(), p["type"].as_str()) {
        (Some("session_meta"), _) => {
            if st.saw_meta {
                return;
            }
            st.saw_meta = true;
            let forked = p["forked_from_id"].is_string() || p["source"]["subagent"]["thread_spawn"]["parent_thread_id"].is_string();
            if forked {
                st.fork_anchor_ms = parse_ms(ts_str).unwrap_or(0);
            }
        }
        (Some("turn_context"), _) => {
            if let Some(m) = p["model"].as_str().filter(|m| !m.is_empty()) {
                st.model = m.to_owned();
            }
        }
        (_, Some("token_count")) => {
            let Some(ts) = parse_ms(ts_str) else { return };
            if st.model.is_empty() {
                return;
            }
            let info = &p["info"];
            let last = read_usage(&info["last_token_usage"]);
            let cumulative = read_usage(&info["total_token_usage"]);

            if st.fork_anchor_ms > 0 {
                if ts - st.fork_anchor_ms < FORK_COPY_GAP_MS {
                    st.fork_anchor_ms = ts;
                    if cumulative.is_some() {
                        st.last_cumulative = cumulative;
                    }
                    return;
                }
                st.fork_anchor_ms = 0;
            }

            let previous = st.last_cumulative;
            if cumulative.is_some() {
                st.last_cumulative = cumulative;
            }
            let Some(last) = last else { return };
            // Zero requests report context size (e.g. after compaction), not usage.
            if last[0] + last[2] == 0 {
                return;
            }
            match cumulative {
                // Re-emitted counters: nothing new was billed.
                Some(c) if Some(c) == previous => return,
                Some(_) => {}
                None => {
                    if st.last_signature == Some(last) {
                        return;
                    }
                }
            }
            st.last_signature = Some(last);

            let key = format!("{}|{}", &ts_str[..13.min(ts_str.len())], st.model);
            add(entry.buckets.entry(key).or_default(), &[last[0], last[1], last[2], last[3], 1]);
        }
        _ => {}
    }
}

/// [input, cached, output, reasoning]
fn read_usage(v: &Value) -> Option<[u64; 4]> {
    let input = v["input_tokens"].as_u64()?;
    let output = v["output_tokens"].as_u64()?;
    let cached = v["cached_input_tokens"].as_u64().unwrap_or(0).min(input);
    let reasoning = v["reasoning_output_tokens"].as_u64().unwrap_or(0).min(output);
    Some([input, cached, output, reasoning])
}

/// "2026-10-30T19:50:27.657Z" -> unix ms (UTC). Minimal RFC 3339 for Codex's fixed format.
fn parse_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let ms = if b.get(19) == Some(&b'.') { s.get(20..23).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0) } else { 0 };
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se) * 1000 + ms)
}

fn utc_hour_from_ms(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}T{:02}", secs.rem_euclid(86_400) / 3600)
}

pub fn utc_hour_days_ago(days: i64) -> String {
    utc_hour_from_ms(crate::store::now_ms() - days * 86_400_000)
}

// Howard Hinnant's civil date algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        let ms = parse_ms("2026-10-30T19:50:27.657Z").unwrap();
        assert_eq!(utc_hour_from_ms(ms), "2026-10-30T19");
        assert_eq!(parse_ms("1970-01-01T00:00:00Z"), Some(0));
    }

    fn line(ts: &str, last: [u64; 3], total: [u64; 3]) -> String {
        format!(
            r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":{},"cached_input_tokens":{},"output_tokens":{}}},"total_token_usage":{{"input_tokens":{},"cached_input_tokens":{},"output_tokens":{}}}}}}}}}"#,
            last[0], last[1], last[2], total[0], total[1], total[2]
        )
    }

    #[test]
    fn counts_each_request_once() {
        let mut e = FileEntry::default();
        parse_line(&mut e, br#"{"timestamp":"2026-10-30T10:00:00.000Z","type":"turn_context","payload":{"model":"gpt-6.1-sol"}}"#);
        parse_line(&mut e, line("2026-10-30T10:00:01.000Z", [100, 50, 10], [100, 50, 10]).as_bytes());
        // Re-emission of the same cumulative counters must not double count.
        parse_line(&mut e, line("2026-10-30T10:00:02.000Z", [100, 50, 10], [100, 50, 10]).as_bytes());
        parse_line(&mut e, line("2026-10-30T10:00:03.000Z", [200, 150, 20], [300, 200, 30]).as_bytes());
        let t = e.buckets["2026-10-30T10|gpt-6.1-sol"];
        assert_eq!(t, [300, 200, 30, 0, 2]);
    }

    #[test]
    fn skips_fork_copies() {
        let mut e = FileEntry::default();
        parse_line(&mut e, br#"{"timestamp":"2026-10-30T10:00:00.000Z","type":"session_meta","payload":{"forked_from_id":"x"}}"#);
        parse_line(&mut e, br#"{"timestamp":"2026-10-30T10:00:00.010Z","type":"turn_context","payload":{"model":"m"}}"#);
        parse_line(&mut e, line("2026-10-30T10:00:00.020Z", [100, 0, 10], [100, 0, 10]).as_bytes());
        parse_line(&mut e, line("2026-10-30T10:00:09.000Z", [50, 0, 5], [150, 0, 15]).as_bytes());
        assert_eq!(e.buckets["2026-10-30T10|m"], [50, 0, 5, 0, 1]);
    }
}

#[cfg(test)]
mod real_data {
    #[test]
    #[ignore = "reads the developer's real ~/.codex; run manually"]
    fn scan_real_home() {
        let dir = std::env::temp_dir().join("hrns-usage-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = super::UsageLedger::new(&dir);
        for pass in ["cold", "warm"] {
            let t = std::time::Instant::now();
            let rows = ledger.report("2000-01-01T00");
            let (mut inp, mut out, mut req) = (0u64, 0u64, 0u64);
            let mut models = std::collections::BTreeMap::<String, u64>::new();
            for r in &rows {
                inp += r.input;
                out += r.output;
                req += r.requests;
                *models.entry(r.model.clone()).or_default() += r.input + r.output;
            }
            println!("{pass}: {:?} rows={} requests={req} input={inp} output={out}", t.elapsed(), rows.len());
            if pass == "cold" {
                println!("models: {models:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
