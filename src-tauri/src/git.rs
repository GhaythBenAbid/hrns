//! Uncommitted changes of a project (working tree + index vs HEAD, untracked files included).
//!
//! Two calls keep the changes panel cheap: `status` returns only per-file counts plus a
//! fingerprint (`rev`) the UI uses to skip unchanged refreshes, and `file_diff` returns one
//! file's patch when the panel actually shows it.

use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;
use std::time::UNIX_EPOCH;

#[derive(Serialize)]
pub struct FileStat {
    pub path: String,
    /// Path before a rename.
    pub old: Option<String>,
    /// "add" | "delete" | "rename" | "update"
    pub kind: &'static str,
    pub add: usize,
    pub del: usize,
    pub binary: bool,
    pub untracked: bool,
}

#[derive(Serialize)]
pub struct Status {
    pub root: String,
    /// Changes whenever any count, path or untracked file changes.
    pub rev: String,
    pub files: Vec<FileStat>,
    pub add: usize,
    pub del: usize,
}

const MAX_UNTRACKED: usize = 300;
/// Untracked files bigger than this are listed without a line count or diff.
const MAX_FILE: u64 = 1024 * 1024;
/// Patch text sent for one file; bigger diffs show a note instead.
const MAX_DIFF: usize = 512 * 1024;
/// `git hash-object -t tree /dev/null`: the base when the repo has no commits yet.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_OPTIONAL_LOCKS", "0").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn base(root: &Path) -> &'static str {
    if git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some() { "HEAD" } else { EMPTY_TREE }
}

/// None when `dir` isn't inside a git repository (or git isn't installed).
pub fn status(dir: &str) -> Option<Status> {
    let root = git(Path::new(dir), &["rev-parse", "--show-toplevel"])?.trim().to_owned();
    let root_path = Path::new(&root);
    let base = base(root_path);
    let numstat = git(root_path, &["diff", base, "--numstat", "-M", "-z"])?;
    let names = git(root_path, &["diff", base, "--name-status", "-M", "-z"])?;
    let untracked = git(root_path, &["ls-files", "--others", "--exclude-standard", "-z"]).unwrap_or_default();

    let mut hasher = DefaultHasher::new();
    (&numstat, &names).hash(&mut hasher);

    let kinds = parse_name_status(&names);
    let mut files = parse_numstat(&numstat, &kinds);
    for path in untracked.split('\0').filter(|p| !p.is_empty()).take(MAX_UNTRACKED) {
        let meta = std::fs::metadata(root_path.join(path)).ok();
        let (size, mtime) = meta
            .as_ref()
            .map(|m| (m.len(), m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())))
            .unwrap_or_default();
        (path, size, mtime).hash(&mut hasher);
        let (add, binary) = if size > MAX_FILE { (0, true) } else { count_lines(&root_path.join(path)) };
        files.push(FileStat { path: path.to_owned(), old: None, kind: "add", add, del: 0, binary, untracked: true });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let (add, del) = files.iter().fold((0, 0), |(a, d), f| (a + f.add, d + f.del));
    Some(Status { root, rev: format!("{:016x}", hasher.finish()), files, add, del })
}

/// `-z` name-status: "M\0path\0", "R087\0old\0new\0".
fn parse_name_status(out: &str) -> HashMap<String, &'static str> {
    let mut kinds = HashMap::new();
    let mut it = out.split('\0').filter(|s| !s.is_empty());
    while let Some(code) = it.next() {
        let kind = match code.as_bytes()[0] {
            b'A' => "add",
            b'D' => "delete",
            b'R' | b'C' => {
                it.next(); // old path
                "rename"
            }
            _ => "update",
        };
        if let Some(path) = it.next() {
            kinds.insert(path.to_owned(), kind);
        }
    }
    kinds
}

/// `-z` numstat: "add\tdel\tpath\0", or "add\tdel\t\0old\0new\0" for renames; "-\t-" for binary.
fn parse_numstat(out: &str, kinds: &HashMap<String, &'static str>) -> Vec<FileStat> {
    let mut files = Vec::new();
    let mut it = out.split('\0');
    while let Some(rec) = it.next() {
        let mut cols = rec.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (cols.next(), cols.next(), cols.next()) else { continue };
        let (path, old) = if path.is_empty() {
            let old = it.next().unwrap_or_default().to_owned();
            (it.next().unwrap_or_default().to_owned(), Some(old))
        } else {
            (path.to_owned(), None)
        };
        let binary = a == "-";
        files.push(FileStat {
            kind: kinds.get(&path).copied().unwrap_or(if old.is_some() { "rename" } else { "update" }),
            path,
            old,
            add: a.parse().unwrap_or(0),
            del: d.parse().unwrap_or(0),
            binary,
            untracked: false,
        });
    }
    files
}

fn count_lines(path: &Path) -> (usize, bool) {
    let Ok(bytes) = std::fs::read(path) else { return (0, false) };
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return (0, true);
    }
    let n = bytes.iter().filter(|&&b| b == b'\n').count();
    (n + usize::from(bytes.last().is_some_and(|&b| b != b'\n')), false)
}

/// One file's unified diff from its first hunk on. None for binary or oversized diffs.
pub fn file_diff(root: &str, path: &str, old: Option<&str>, untracked: bool) -> Option<String> {
    let root = Path::new(root);
    if untracked {
        let file = root.join(path);
        if std::fs::metadata(&file).ok()?.len() > MAX_FILE {
            return None;
        }
        let bytes = std::fs::read(file).ok()?;
        if bytes.iter().take(8000).any(|&b| b == 0) {
            return None;
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let mut d = format!("@@ -0,0 +1,{} @@", lines.len());
        for l in lines {
            d.push_str("\n+");
            d.push_str(l);
        }
        return Some(d);
    }
    let mut args = vec!["diff", base(root), "-M", "--no-color", "--no-ext-diff", "--"];
    args.extend(old);
    args.push(path);
    let patch = git(root, &args)?;
    let body = &patch[patch.find("\n@@").map(|i| i + 1)?..];
    (body.len() <= MAX_DIFF).then(|| body.trim_end_matches('\n').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numstat_and_kinds() {
        let names = "M\0src/a.ts\0R090\0old.rs\0new.rs\0D\0gone.txt\0A\0img.png\0";
        let numstat = "3\t1\tsrc/a.ts\0002\t2\t\0old.rs\0new.rs\0000\t5\tgone.txt\0-\t-\timg.png\0";
        let files = parse_numstat(numstat, &parse_name_status(names));
        let got: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.kind, f.add, f.del, f.binary)).collect();
        assert_eq!(
            got,
            [
                ("src/a.ts", "update", 3, 1, false),
                ("new.rs", "rename", 2, 2, false),
                ("gone.txt", "delete", 0, 5, false),
                ("img.png", "add", 0, 0, true),
            ]
        );
        assert_eq!(files[1].old.as_deref(), Some("old.rs"));
    }

    #[test]
    #[ignore = "reads this repo's working tree; run with --ignored"]
    fn this_repo() {
        let t = std::time::Instant::now();
        let s = status(env!("CARGO_MANIFEST_DIR")).expect("a git repo");
        eprintln!("status in {:?}: {} files +{} -{} rev={}", t.elapsed(), s.files.len(), s.add, s.del, s.rev);
        let f = s.files.iter().find(|f| !f.untracked && !f.binary).unwrap();
        let t = std::time::Instant::now();
        let d = file_diff(&s.root, &f.path, f.old.as_deref(), false).unwrap();
        eprintln!("diff of {} in {:?}: {} bytes", f.path, t.elapsed(), d.len());
    }
}
