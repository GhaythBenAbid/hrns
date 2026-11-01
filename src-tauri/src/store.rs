//! Local chat history: one JSON file per thread, cached in memory.
//! Writes only happen when a message is added (never per streamed token).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub use crate::worktree::Worktree;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    /// "user" | "assistant"
    pub role: String,
    pub text: String,
    /// Ordered turn activity: text, reasoning, commands, file changes, tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: i64,
}

/// Token usage, as reported by Codex.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub reasoning: u64,
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.input += o.input;
        self.cached += o.cached;
        self.output += o.output;
        self.reasoning += o.reasoning;
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub model: String,
    #[serde(default)]
    pub effort: Option<String>,
    /// "read-only" | "auto" | "full"
    #[serde(default = "default_access")]
    pub access: String,
    /// Project folder the agent works in.
    #[serde(default)]
    pub cwd: Option<String>,
    /// ChatGPT account (hrns account id) this chat runs on; None = default.
    #[serde(default)]
    pub account: Option<String>,
    /// Set when the chat runs in its own git worktree (`cwd` is then the worktree folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<Worktree>,
    /// Token usage per model, so cost can be priced per model.
    #[serde(default)]
    pub usage: HashMap<String, Usage>,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<Message>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub worktree: Option<Worktree>,
    pub account: Option<String>,
    pub model: String,
    /// Start of the latest reply (or user message), for the sidebar.
    pub preview: String,
    pub messages: usize,
    pub files: usize,
    pub additions: usize,
    pub deletions: usize,
    pub commands: usize,
    pub usage: HashMap<String, Usage>,
    pub failed: bool,
    pub updated_at: i64,
}

impl Summary {
    fn of(t: &Thread) -> Self {
        let (mut files, mut additions, mut deletions, mut commands) = (0, 0, 0, 0);
        for part in t.messages.iter().filter_map(|m| m.parts.as_ref()).flatten() {
            match part["type"].as_str() {
                Some("fileChange") => {
                    for c in part["changes"].as_array().into_iter().flatten() {
                        files += 1;
                        let (a, d) = diff_stat(c["kind"].as_str().unwrap_or_default(), c["diff"].as_str().unwrap_or_default());
                        additions += a;
                        deletions += d;
                    }
                }
                Some("command") => commands += 1,
                _ => {}
            }
        }
        let last = t.messages.last();
        let preview_src = t
            .messages
            .iter()
            .rev()
            .find(|m| !m.text.is_empty())
            .map(|m| m.text.as_str())
            .unwrap_or_default();
        Self {
            id: t.id.clone(),
            title: t.title.clone(),
            cwd: t.cwd.clone(),
            worktree: t.worktree.clone(),
            account: t.account.clone(),
            model: t.model.clone(),
            preview: preview_src.chars().take(220).collect(),
            messages: t.messages.len(),
            files,
            additions,
            deletions,
            commands,
            usage: t.usage.clone(),
            failed: last.is_some_and(|m| m.error.as_deref().is_some_and(|e| e != "Stopped")),
            updated_at: t.updated_at,
        }
    }
}

/// Codex sends unified diffs for edits but raw contents for added/deleted files.
fn diff_stat(kind: &str, diff: &str) -> (usize, usize) {
    let lines = diff.strip_suffix('\n').unwrap_or(diff).lines();
    match kind {
        "add" => (lines.count(), 0),
        "delete" => (0, lines.count()),
        _ => lines.fold((0, 0), |(a, d), l| {
            if l.starts_with('+') && !l.starts_with("+++") {
                (a + 1, d)
            } else if l.starts_with('-') && !l.starts_with("---") {
                (a, d + 1)
            } else {
                (a, d)
            }
        }),
    }
}

fn default_access() -> String {
    "auto".into()
}

pub struct Store {
    dir: PathBuf,
    threads: Mutex<HashMap<String, Thread>>,
}

impl Store {
    pub fn open(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let mut threads = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Some(t) = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Thread>(&b).ok())
                {
                    threads.insert(t.id.clone(), t);
                }
            }
        }
        Self { dir, threads: Mutex::new(threads) }
    }

    pub fn list(&self) -> Vec<Summary> {
        let threads = self.threads.lock().unwrap();
        let mut out: Vec<Summary> = threads
            .values()
            .map(Summary::of)
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    pub fn get(&self, id: &str) -> Option<Thread> {
        self.threads.lock().unwrap().get(id).cloned()
    }

    pub fn insert(&self, thread: Thread) {
        self.save(&thread);
        self.threads.lock().unwrap().insert(thread.id.clone(), thread);
    }

    /// Mutate a thread and persist it.
    pub fn update<R>(&self, id: &str, f: impl FnOnce(&mut Thread) -> R) -> Option<R> {
        let mut threads = self.threads.lock().unwrap();
        let thread = threads.get_mut(id)?;
        let r = f(thread);
        self.save(thread);
        Some(r)
    }

    pub fn delete(&self, id: &str) {
        self.threads.lock().unwrap().remove(id);
        let _ = std::fs::remove_file(self.path(id));
    }

    fn path(&self, id: &str) -> PathBuf {
        // ids are uuids we generate; strip anything path-like just in case
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        self.dir.join(format!("{safe}.json"))
    }

    fn save(&self, thread: &Thread) {
        let path = self.path(&thread.id);
        let tmp = path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec(thread) {
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}
