//! Multiple ChatGPT accounts. Codex keeps one login per CODEX_HOME, so every
//! extra account gets its own home under the app data dir. Those homes link to
//! the user's real config (config.toml, AGENTS.md, skills…) so the agent behaves
//! the same; only auth.json and runtime state are per account.
//! "default" is the user's own ~/.codex.

use crate::codex::Codex;
use crate::store::{Store, Thread};
use crate::usage::UsageLedger;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crate::events::Events;

pub const DEFAULT: &str = "default";

/// Shared from ~/.codex into every extra account home.
const SHARED: &[&str] = &["config.toml", "AGENTS.md", "RTK.md", "skills", "prompts", "rules", "plugins", "hooks"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountEntry {
    pub id: String,
    /// Cached from the last successful `account/read`, so listing needs no sidecar.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
}

pub struct Accounts {
    events: Events,
    store: Arc<Store>,
    ledger: Arc<UsageLedger>,
    dir: PathBuf,
    entries: Mutex<Vec<AccountEntry>>,
    engines: Mutex<HashMap<String, Arc<Codex>>>,
}

impl Accounts {
    pub fn open(events: Events, store: Arc<Store>, ledger: Arc<UsageLedger>, dir: PathBuf) -> Arc<Self> {
        let mut entries: Vec<AccountEntry> = std::fs::read(dir.join("accounts.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if !entries.iter().any(|e| e.id == DEFAULT) {
            entries.insert(0, AccountEntry { id: DEFAULT.into(), email: None, plan: None });
        }
        Arc::new(Self { events, store, ledger, dir, entries: Mutex::new(entries), engines: Mutex::new(HashMap::new()) })
    }

    pub fn list(&self) -> Vec<AccountEntry> {
        self.entries.lock().unwrap().clone()
    }

    fn exists(&self, id: &str) -> bool {
        self.entries.lock().unwrap().iter().any(|e| e.id == id)
    }

    /// The engine for an account, created on first use (the sidecar itself still starts lazily).
    pub fn engine(&self, id: &str) -> Result<Arc<Codex>, String> {
        if !self.exists(id) {
            return Err("Unknown account".into());
        }
        let mut engines = self.engines.lock().unwrap();
        if let Some(e) = engines.get(id) {
            return Ok(e.clone());
        }
        let home = (id != DEFAULT).then(|| self.dir.join(id));
        if let Some(h) = &home {
            link_shared_config(h);
        }
        let engine = Codex::new(self.events.clone(), self.store.clone(), self.ledger.clone(), id.to_owned(), home);
        engines.insert(id.to_owned(), engine.clone());
        Ok(engine)
    }

    /// Engine a thread runs on (falls back to default if its account was removed).
    pub fn engine_for(&self, thread: &Thread) -> Result<Arc<Codex>, String> {
        match thread.account.as_deref() {
            Some(id) if self.exists(id) => self.engine(id),
            _ => self.engine(DEFAULT),
        }
    }

    pub fn running(&self) -> Vec<Arc<Codex>> {
        self.engines.lock().unwrap().values().cloned().collect()
    }

    pub fn add(&self) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        std::fs::create_dir_all(self.dir.join(&id)).map_err(|e| e.to_string())?;
        self.entries.lock().unwrap().push(AccountEntry { id: id.clone(), email: None, plan: None });
        self.save();
        Ok(id)
    }

    pub async fn remove(&self, id: &str) -> Result<(), String> {
        if id == DEFAULT {
            return Err("The default account can't be removed — sign out instead".into());
        }
        let engine = self.engines.lock().unwrap().remove(id);
        if let Some(e) = engine {
            e.shutdown().await;
        }
        self.entries.lock().unwrap().retain(|e| e.id != id);
        self.save();
        let _ = std::fs::remove_dir_all(self.dir.join(id));
        Ok(())
    }

    /// Cache the signed-in identity for listing.
    pub fn remember(&self, id: &str, email: Option<String>, plan: Option<String>) {
        let mut entries = self.entries.lock().unwrap();
        if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
            if e.email == email && e.plan == plan {
                return;
            }
            e.email = email;
            e.plan = plan;
        }
        drop(entries);
        self.save();
    }

    fn save(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(bytes) = serde_json::to_vec_pretty(&*self.entries.lock().unwrap()) {
            let _ = std::fs::write(self.dir.join("accounts.json"), bytes);
        }
    }
}

/// Symlink the user's shared Codex config into an account home (idempotent).
fn link_shared_config(home: &Path) {
    let Some(src) = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")) else { return };
    let _ = std::fs::create_dir_all(home);
    for name in SHARED {
        let from = src.join(name);
        let to = home.join(name);
        if from.exists() && std::fs::symlink_metadata(&to).is_err() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&from, &to);
        }
    }
}
