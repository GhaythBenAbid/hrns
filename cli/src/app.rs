//! State and behavior of the terminal UI. Drawing lives in ui.rs.

use crate::accounts::{Accounts, DEFAULT};
use crate::fuzzy;
use crate::store::{Message, Store, Thread};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

/// Results of background work, fed back into the event loop.
pub enum Msg {
    Models(String, Result<Vec<Model>, String>),
    Account(String, Result<Value, String>),
    /// The thread id the send was for, and its result.
    Sent(String, Result<Thread, String>),
    Titled(String, String),
    Files(Vec<String>),
    Diff(String),
    Error(String),
}

#[derive(Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub default: bool,
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
}

impl Model {
    fn from(v: &Value) -> Option<Self> {
        Some(Self {
            id: v["id"].as_str()?.to_owned(),
            name: v["displayName"].as_str().unwrap_or(v["id"].as_str()?).to_owned(),
            default: v["isDefault"].as_bool().unwrap_or(false),
            efforts: v["supportedReasoningEfforts"].as_array().map(|a| a.iter().filter_map(|e| e["reasoningEffort"].as_str().map(str::to_owned)).collect()).unwrap_or_default(),
            default_effort: v["defaultReasoningEffort"].as_str().map(str::to_owned),
        })
    }
}

/// The in-flight turn: parts as they stream, and approvals waiting for an answer.
#[derive(Default, Clone)]
pub struct Live {
    pub parts: Vec<Value>,
    pub approvals: Vec<Value>,
    pub notice: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PickKind {
    Resume,
    Model,
    Effort,
    Access,
    Account,
    Command,
}

pub struct Item {
    pub label: String,
    pub detail: String,
    pub value: String,
}

pub struct Picker {
    pub kind: PickKind,
    pub title: String,
    pub query: String,
    pub items: Vec<Item>,
    pub shown: Vec<usize>,
    pub sel: usize,
}

impl Picker {
    fn new(kind: PickKind, title: &str, items: Vec<Item>, current: Option<&str>) -> Self {
        let mut p = Self { kind, title: title.into(), query: String::new(), shown: (0..items.len()).collect(), items, sel: 0 };
        if let Some(cur) = current {
            p.sel = p.items.iter().position(|i| i.value == cur).unwrap_or(0);
        }
        p
    }
    fn filter(&mut self) {
        self.shown = if self.query.is_empty() {
            (0..self.items.len()).collect()
        } else {
            fuzzy::rank(&self.query, &self.items, |i| i.label.as_str())
        };
        self.sel = 0;
    }
    pub fn selected(&self) -> Option<&Item> {
        self.shown.get(self.sel).map(|&i| &self.items[i])
    }
}

pub enum Overlay {
    Picker(Picker),
    /// Scrollable text (diff, usage, help).
    Pager { title: String, lines: Vec<String>, scroll: usize },
}

/// The slash commands, shown as you type "/".
pub const COMMANDS: &[(&str, &str)] = &[
    ("/new", "Start a new chat in this folder"),
    ("/split", "Open a second chat side by side"),
    ("/close", "Close the focused pane"),
    ("/resume", "Pick up an earlier chat"),
    ("/model", "Choose the model"),
    ("/effort", "Choose how hard it thinks"),
    ("/access", "Read only, Auto or Full access"),
    ("/account", "Switch ChatGPT account"),
    ("/diff", "Uncommitted changes in this folder"),
    ("/usage", "Tokens and cost, last 7 days"),
    ("/details", "Show or hide every step"),
    ("/help", "Keys and commands"),
    ("/quit", "Leave hrns"),
];

/// One chat on screen (the window shows one, or two side by side).
pub struct Chat {
    pub cwd: String,
    pub branch: Option<String>,
    pub account: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub models: Vec<Model>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub access: String,
    pub limit: Option<f64>,

    pub thread_id: String,
    pub thread: Option<Thread>,
    pub live: Option<Live>,
    pub started: Option<Instant>,
    pub sending: bool,
    /// Lines scrolled up from the bottom of the transcript (0 = following).
    pub scroll: usize,
    pub files: Option<Vec<String>>,
}

impl Chat {
    pub fn new(cwd: String, account: String, email: Option<String>, model: Option<String>, access: String) -> Self {
        Self {
            branch: git_branch(&cwd),
            cwd,
            account,
            email,
            plan: None,
            models: vec![],
            model,
            effort: None,
            access,
            limit: None,
            thread_id: new_id(),
            thread: None,
            live: None,
            started: None,
            sending: false,
            scroll: 0,
            files: None,
        }
    }

    /// A fresh chat with the same folder, account and settings.
    fn sibling(&self) -> Self {
        let mut c = Chat::new(self.cwd.clone(), self.account.clone(), self.email.clone(), self.model.clone(), self.access.clone());
        c.models = self.models.clone();
        c.effort = self.effort.clone();
        c.plan = self.plan.clone();
        c.limit = self.limit;
        c.files = self.files.clone();
        c
    }

    pub fn busy(&self) -> bool {
        self.live.is_some() || self.sending
    }

    pub fn current_model(&self) -> Option<&Model> {
        self.models.iter().find(|m| Some(&m.id) == self.model.as_ref()).or_else(|| self.models.iter().find(|m| m.default)).or(self.models.first())
    }

    fn set_models(&mut self, models: Vec<Model>) {
        self.models = models;
        if let Some(m) = self.current_model().cloned() {
            if self.model.is_none() {
                self.model = Some(m.id.clone());
            }
            if self.effort.as_ref().is_none_or(|e| !m.efforts.contains(e)) {
                self.effort = m.default_effort.clone();
            }
        }
    }

    /// Apply a streamed chat event for this chat's thread.
    fn apply(&mut self, ev: Value, store: &Store) {
        let kind = ev["kind"].as_str().unwrap_or_default();
        if kind == "done" {
            self.live = None;
            self.started = None;
            if let Ok(m) = serde_json::from_value::<Message>(ev["message"].clone()) {
                if let Some(t) = self.thread.as_mut() {
                    if !t.messages.iter().any(|x| x.id == m.id) {
                        t.messages.push(m);
                    }
                }
            }
            // The saved chat has the usage and final title.
            if let Some(t) = store.get(&self.thread_id) {
                let msgs = self.thread.as_ref().map(|x| x.messages.len()).unwrap_or(0);
                if t.messages.len() >= msgs {
                    self.thread = Some(t);
                }
            }
            return;
        }
        let live = self.live.get_or_insert_with(Live::default);
        match kind {
            "part" => {
                let part = ev["part"].clone();
                match live.parts.iter_mut().find(|p| p["id"] == part["id"]) {
                    Some(p) => *p = part,
                    None => live.parts.push(part),
                }
            }
            "delta" => {
                let (pid, field, text) = (ev["partId"].clone(), ev["field"].as_str().unwrap_or("text").to_owned(), ev["text"].as_str().unwrap_or_default());
                if !live.parts.iter().any(|p| p["id"] == pid) {
                    let t = if field == "output" { "command" } else { "text" };
                    live.parts.push(serde_json::json!({ "type": t, "id": pid, "status": "inProgress", "command": "" }));
                }
                if let Some(p) = live.parts.iter_mut().find(|p| p["id"] == pid) {
                    let cur = p[&field].as_str().unwrap_or_default().to_owned();
                    p[&field] = Value::String(cur + text);
                }
            }
            "notice" => live.notice = ev["message"].as_str().map(str::to_owned),
            "approval" => live.approvals.push(ev["approval"].clone()),
            "approvalResolved" => live.approvals.retain(|a| a["key"] != ev["key"]),
            _ => {}
        }
    }
}

pub struct App {
    pub tx: UnboundedSender<Msg>,
    pub accounts: Arc<Accounts>,
    pub store: Arc<Store>,
    pub ledger: Arc<crate::usage::UsageLedger>,
    pub pricing: Arc<crate::pricing::Pricing>,

    /// The focused chat, and the other one when the screen is split.
    pub c: Chat,
    pub other: Option<Chat>,
    /// The focused chat is the right-hand one.
    pub right: bool,

    pub input: String,
    /// Cursor position in chars.
    pub cursor: usize,
    pub history: Vec<String>,
    pub hist: Option<usize>,

    pub details: bool,
    pub overlay: Option<Overlay>,
    /// Suggestions under the composer (slash commands or @ files) and the selected one.
    pub suggest: Vec<(String, String)>,
    pub suggest_sel: usize,
    pub toast: Option<(String, Instant)>,
    pub quit: bool,
    pub ctrl_c: Option<Instant>,
    pub tick: usize,
    /// The next chat picked in the resume picker opens in a new pane.
    pub split_pick: bool,
}

pub fn short_home(p: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if p.starts_with(&h) => format!("~{}", &p[h.len()..]),
        _ => p.to_owned(),
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tx: UnboundedSender<Msg>,
        accounts: Arc<Accounts>,
        store: Arc<Store>,
        ledger: Arc<crate::usage::UsageLedger>,
        pricing: Arc<crate::pricing::Pricing>,
        cwd: String,
        account: String,
        model: Option<String>,
        access: String,
    ) -> Self {
        let email = accounts.list().into_iter().find(|a| a.id == account).and_then(|a| a.email);
        Self {
            tx,
            accounts,
            store,
            ledger,
            pricing,
            c: Chat::new(cwd, account, email, model, access),
            other: None,
            right: false,
            input: String::new(),
            cursor: 0,
            history: load_history(),
            hist: None,
            details: false,
            overlay: None,
            suggest: vec![],
            suggest_sel: 0,
            toast: None,
            quit: false,
            ctrl_c: None,
            tick: 0,
            split_pick: false,
        }
    }

    pub fn busy(&self) -> bool {
        self.c.busy()
    }

    /// Something on screen is animating (either pane working).
    pub fn any_busy(&self) -> bool {
        self.c.busy() || self.other.as_ref().is_some_and(|o| o.busy())
    }

    pub fn current_model(&self) -> Option<&Model> {
        self.c.current_model()
    }

    fn chats_mut(&mut self) -> impl Iterator<Item = &mut Chat> {
        std::iter::once(&mut self.c).chain(self.other.as_mut())
    }

    fn chat_for(&mut self, thread_id: &str) -> Option<&mut Chat> {
        self.chats_mut().find(|c| c.thread_id == thread_id)
    }

    /// (left, right, focused-is-left) for drawing; right is None when not split.
    pub fn panes_mut(&mut self) -> (&mut Chat, Option<&mut Chat>) {
        match (&mut self.other, self.right) {
            (None, _) => (&mut self.c, None),
            (Some(o), true) => (o, Some(&mut self.c)),
            (Some(o), false) => (&mut self.c, Some(o)),
        }
    }

    /// Open a second pane on the right and focus it (a new chat, or `thread` when given).
    pub fn split(&mut self, thread: Option<&str>) {
        if self.other.as_ref().is_some_and(|o| o.busy()) {
            return self.say("The other pane is still working. Switch to it and /close it when it's done.");
        }
        let fresh = self.c.sibling();
        let left = std::mem::replace(&mut self.c, fresh);
        self.other = Some(left);
        self.right = true;
        if let Some(id) = thread {
            self.open_thread(id);
        }
        self.input.clear();
        self.cursor = 0;
        self.say("Split · alt+←/→ switches panes · /close closes one");
    }

    pub fn focus_other(&mut self) {
        if let Some(o) = self.other.as_mut() {
            std::mem::swap(&mut self.c, o);
            self.right = !self.right;
            self.suggest.clear();
        }
    }

    /// Close the focused pane. A turn still running there keeps going and is saved as usual.
    pub fn close_pane(&mut self) {
        let Some(o) = self.other.take() else { return self.say("Nothing to close. /split opens a second pane.") };
        self.c = o;
        self.right = false;
    }

    pub fn say(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    // ---------- background work ----------

    pub fn load_account(&self) {
        let (accounts, id, tx) = (self.accounts.clone(), self.c.account.clone(), self.tx.clone());
        tokio::spawn(async move {
            let Ok(engine) = accounts.engine(&id) else { return };
            let models = engine.models().await.map(|v| v.as_array().map(|a| a.iter().filter_map(Model::from).collect()).unwrap_or_default());
            let _ = tx.send(Msg::Models(id.clone(), models));
            let _ = tx.send(Msg::Account(id, engine.account().await));
        });
    }

    fn load_files(&mut self) {
        if self.c.files.is_some() {
            return;
        }
        self.c.files = Some(vec![]);
        let (cwd, tx) = (self.c.cwd.clone(), self.tx.clone());
        tokio::spawn(async move {
            let out = tokio::process::Command::new("git").args(["-C", &cwd, "ls-files", "--cached", "--others", "--exclude-standard"]).output().await;
            let mut files: Vec<String> = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).lines().take(20000).map(str::to_owned).collect()).unwrap_or_default();
            if files.is_empty() {
                files = walk(&cwd, 4000);
            }
            let _ = tx.send(Msg::Files(files));
        });
    }

    pub fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Models(id, Ok(models)) => {
                for c in self.chats_mut().filter(|c| c.account == id) {
                    c.set_models(models.clone());
                }
            }
            Msg::Models(_, Err(e)) => self.say(format!("Couldn't load models: {e}")),
            Msg::Account(id, Ok(v)) => {
                let email = v["account"]["email"].as_str().map(str::to_owned);
                let plan = v["account"]["planType"].as_str().map(str::to_owned);
                let limit = v["rateLimits"]["primary"]["usedPercent"].as_f64();
                for c in self.chats_mut().filter(|c| c.account == id) {
                    c.email = email.clone().or(c.email.take());
                    c.plan = plan.clone();
                    c.limit = limit;
                }
                if email.is_some() {
                    self.accounts.remember(&id, email, plan);
                }
                if v["account"].is_null() && id == self.c.account {
                    self.say("Not signed in. Run `codex login` (or `hrns login`), then try again.");
                }
            }
            Msg::Account(_, Err(_)) => {}
            Msg::Sent(id, Ok(t)) => {
                let Some(c) = self.chat_for(&id) else { return };
                c.sending = false;
                let first = c.thread.as_ref().is_none_or(|x| x.messages.len() <= 1);
                // Keep what has streamed in already; take settings and title from the saved chat.
                let mut t = t;
                if let Some(m) = c.thread.as_ref().map(|x| x.messages.clone()) {
                    if m.len() > t.messages.len() {
                        t.messages = m;
                    }
                }
                c.thread = Some(t);
                if first {
                    self.name_chat(&id);
                }
            }
            Msg::Sent(id, Err(e)) => {
                if let Some(c) = self.chat_for(&id) {
                    c.sending = false;
                    c.live = None;
                    c.started = None;
                }
                self.say(e);
            }
            Msg::Titled(id, title) => {
                if let Some(t) = self.chat_for(&id).and_then(|c| c.thread.as_mut()) {
                    t.title = title;
                }
            }
            Msg::Files(f) => {
                self.c.files = Some(f);
                self.update_suggest();
            }
            Msg::Diff(d) => {
                let lines: Vec<String> = if d.trim().is_empty() { vec!["No uncommitted changes.".into()] } else { d.lines().map(str::to_owned).collect() };
                self.overlay = Some(Overlay::Pager { title: format!("Uncommitted changes · {}", short_home(&self.c.cwd)), lines, scroll: 0 });
            }
            Msg::Error(e) => self.say(e),
        }
    }

    /// Core events (same ones the desktop window gets), routed to the pane they belong to.
    pub fn on_core(&mut self, kind: &str, v: Value) {
        match kind {
            "chat" => {
                let id = v["threadId"].as_str().unwrap_or_default().to_owned();
                let store = self.store.clone();
                if let Some(c) = self.chat_for(&id) {
                    c.apply(v, &store);
                }
            }
            "thread-title" => {
                let (id, title) = (v["threadId"].as_str().unwrap_or_default().to_owned(), v["title"].as_str().map(str::to_owned));
                if let (Some(t), Some(title)) = (self.chat_for(&id).and_then(|c| c.thread.as_mut()), title) {
                    t.title = title;
                }
            }
            "rate-limits" => {
                let account = v["account"].as_str().unwrap_or_default().to_owned();
                if let Some(p) = v["rateLimits"]["primary"]["usedPercent"].as_f64() {
                    for c in self.chats_mut().filter(|c| c.account == account) {
                        c.limit = Some(p);
                    }
                }
            }
            _ => {}
        }
    }

    // ---------- actions ----------

    pub fn send(&mut self, text: String) {
        let text = text.trim().to_owned();
        if text.is_empty() || self.busy() {
            return;
        }
        if self.history.first() != Some(&text) {
            self.history.insert(0, text.clone());
            self.history.truncate(200);
            save_history(&self.history);
        }
        let now = crate::store::now_ms();
        let msg = Message { id: new_id(), role: "user".into(), text: text.clone(), parts: None, model: None, error: None, created_at: now };
        match self.c.thread.as_mut() {
            Some(t) => t.messages.push(msg),
            None => {
                self.c.thread = Some(Thread {
                    id: self.c.thread_id.clone(),
                    title: text.lines().next().unwrap_or("New chat").chars().take(60).collect(),
                    model: self.c.model.clone().unwrap_or_default(),
                    effort: self.c.effort.clone(),
                    access: self.c.access.clone(),
                    cwd: Some(self.c.cwd.clone()),
                    account: (self.c.account != DEFAULT).then(|| self.c.account.clone()),
                    usage: Default::default(),
                    created_at: now,
                    updated_at: now,
                    messages: vec![msg],
                })
            }
        }
        self.c.live = Some(Live::default());
        self.c.started = Some(Instant::now());
        self.c.sending = true;
        self.c.scroll = 0;
        let Some(model) = self.current_model().map(|m| m.id.clone()).or(self.c.model.clone()) else {
            self.say("Models are still loading. Try again in a moment.");
            self.c.live = None;
            self.c.sending = false;
            return;
        };
        let (accounts, account, tx) = (self.accounts.clone(), self.c.account.clone(), self.tx.clone());
        let (id, effort, access, cwd) = (self.c.thread_id.clone(), self.c.effort.clone(), self.c.access.clone(), self.c.cwd.clone());
        let thread_id = id.clone();
        let existing = self.store.get(&id);
        tokio::spawn(async move {
            let engine = match &existing {
                Some(t) => accounts.engine_for(t),
                None => accounts.engine(&account),
            };
            let r = match engine {
                Ok(e) => e.send(Some(id), text, model, effort, access, Some(cwd)).await,
                Err(e) => Err(e),
            };
            let _ = tx.send(Msg::Sent(thread_id, r));
        });
    }

    /// Like the desktop: a small, fast model writes the title after the first message.
    fn name_chat(&mut self, id: &str) {
        let Some(c) = self.chat_for(id) else { return };
        let Some(m) = c.models.iter().find(|m| ["mini", "nano", "spark", "flash", "lite"].iter().any(|k| m.id.to_lowercase().contains(k))).or(c.current_model()).cloned() else { return };
        let effort = ["none", "minimal", "low"].iter().find(|e| m.efforts.iter().any(|x| x == *e)).map(|e| e.to_string());
        let Some(thread) = c.thread.clone() else { return };
        let Some(first) = thread.messages.iter().find(|x| x.role == "user").map(|x| x.text.clone()) else { return };
        let (accounts, tx) = (self.accounts.clone(), self.tx.clone());
        tokio::spawn(async move {
            if let Ok(e) = accounts.engine_for(&thread) {
                if let Ok(title) = e.generate_title(thread.id.clone(), first, m.id, effort).await {
                    let _ = tx.send(Msg::Titled(thread.id, title));
                }
            }
        });
    }

    pub fn stop(&self) {
        let (accounts, id, account) = (self.accounts.clone(), self.c.thread_id.clone(), self.c.account.clone());
        let thread = self.c.thread.clone();
        tokio::spawn(async move {
            let e = match &thread {
                Some(t) => accounts.engine_for(t),
                None => accounts.engine(&account),
            };
            if let Ok(e) = e {
                let _ = e.stop(&id).await;
            }
        });
    }

    fn respond(&mut self, decision: &str) {
        let Some(a) = self.c.live.as_ref().and_then(|l| l.approvals.first()).cloned() else { return };
        let key = a["key"].as_str().unwrap_or_default().to_owned();
        if let Some(l) = self.c.live.as_mut() {
            l.approvals.retain(|x| x["key"] != a["key"]);
        }
        let (accounts, decision) = (self.accounts.clone(), decision.to_owned());
        tokio::spawn(async move {
            if let Some(e) = accounts.running().into_iter().find(|e| e.has_approval(&key)) {
                let _ = e.respond(&key, &decision).await;
            }
        });
    }

    pub fn new_chat(&mut self) {
        if self.busy() {
            self.say("Wait for the turn to finish (esc stops it).");
            return;
        }
        self.c.thread = None;
        self.c.live = None;
        self.c.thread_id = new_id();
        self.c.scroll = 0;
    }

    pub fn open_thread(&mut self, id: &str) {
        let Some(t) = self.store.get(id) else { return };
        self.c.thread_id = t.id.clone();
        if let Some(cwd) = t.cwd.clone().filter(|c| std::path::Path::new(c).is_dir()) {
            if cwd != self.c.cwd {
                self.c.cwd = cwd;
                self.c.branch = git_branch(&self.c.cwd);
                self.c.files = None;
            }
        }
        let account = t.account.clone().unwrap_or_else(|| DEFAULT.into());
        if account != self.c.account {
            self.c.account = account;
            self.load_account();
        }
        if !t.model.is_empty() {
            self.c.model = Some(t.model.clone());
        }
        self.c.effort = t.effort.clone().or(self.c.effort.take());
        self.c.access = t.access.clone();
        // A turn still running in this process?
        self.c.live = self.accounts.engine_for(&t).ok().and_then(|e| e.live(id)).map(|v| Live {
            parts: v["parts"].as_array().cloned().unwrap_or_default(),
            approvals: v["approvals"].as_array().cloned().unwrap_or_default(),
            notice: None,
        });
        self.c.thread = Some(t);
        self.c.scroll = 0;
    }

    fn run_command(&mut self, cmd: &str) {
        match cmd {
            "/new" | "/clear" => self.new_chat(),
            "/split" => self.open_split_picker(),
            "/close" => self.close_pane(),
            "/focus" => self.focus_other(),
            "/resume" => self.open_resume(),
            "/model" => {
                let items = self.c.models.iter().map(|m| Item { label: m.name.clone(), detail: m.id.clone(), value: m.id.clone() }).collect();
                let cur = self.c.model.clone();
                self.overlay = Some(Overlay::Picker(Picker::new(PickKind::Model, "Model", items, cur.as_deref())));
            }
            "/effort" => {
                let efforts = self.current_model().map(|m| m.efforts.clone()).unwrap_or_default();
                if efforts.is_empty() {
                    return self.say("This model has no effort setting.");
                }
                let items = efforts.into_iter().map(|e| Item { label: e.clone(), detail: String::new(), value: e }).collect();
                let cur = self.c.effort.clone();
                self.overlay = Some(Overlay::Picker(Picker::new(PickKind::Effort, "Reasoning effort", items, cur.as_deref())));
            }
            "/access" => {
                let items = vec![
                    Item { label: "Read only".into(), detail: "Reads files; asks before editing or running anything".into(), value: "read-only".into() },
                    Item { label: "Auto".into(), detail: "Edits and runs inside the folder; asks for anything else".into(), value: "auto".into() },
                    Item { label: "Full access".into(), detail: "No sandbox and no prompts".into(), value: "full".into() },
                ];
                let cur = self.c.access.clone();
                self.overlay = Some(Overlay::Picker(Picker::new(PickKind::Access, "Permissions", items, Some(&cur))));
            }
            "/account" => {
                let items = self.accounts.list().into_iter().map(|a| Item { label: a.email.clone().unwrap_or_else(|| if a.id == DEFAULT { "Default (~/.codex)".into() } else { a.id.clone() }), detail: a.plan.clone().map(|p| format!("ChatGPT {p}")).unwrap_or_default(), value: a.id }).collect();
                let cur = self.c.account.clone();
                self.overlay = Some(Overlay::Picker(Picker::new(PickKind::Account, "Account for new chats", items, Some(&cur))));
            }
            "/diff" => {
                let (cwd, tx) = (self.c.cwd.clone(), self.tx.clone());
                tokio::spawn(async move {
                    let tracked = tokio::process::Command::new("git").args(["-C", &cwd, "diff", "HEAD", "--stat", "--patch", "--no-color"]).output().await;
                    let untracked = tokio::process::Command::new("git").args(["-C", &cwd, "ls-files", "--others", "--exclude-standard"]).output().await;
                    let mut text = match tracked {
                        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
                        _ => return { let _ = tx.send(Msg::Error("Not a git repository.".into())); },
                    };
                    if let Ok(o) = untracked {
                        let list = String::from_utf8_lossy(&o.stdout);
                        if !list.trim().is_empty() {
                            text.push_str("\nNew files:\n");
                            for f in list.lines() {
                                text.push_str(&format!("+ {f}\n"));
                            }
                        }
                    }
                    let _ = tx.send(Msg::Diff(text));
                });
            }
            "/usage" => self.open_usage(),
            "/details" => {
                self.details = !self.details;
                self.say(if self.details { "Showing every step" } else { "Steps folded" });
            }
            "/help" => self.overlay = Some(Overlay::Pager { title: "hrns · keys and commands".into(), lines: help(), scroll: 0 }),
            "/quit" | "/exit" => self.quit = true,
            other => self.say(format!("Unknown command {other}. Type / to see them.")),
        }
    }

    /// Pick what goes in the new pane: a new chat, or any earlier one.
    pub fn open_split_picker(&mut self) {
        self.open_resume();
        if let Some(Overlay::Picker(p)) = self.overlay.as_mut() {
            p.title = "Open beside this chat".into();
            p.items.insert(0, Item { label: "New chat".into(), detail: short_home(&self.c.cwd), value: String::new() });
            p.items.retain(|i| i.value != self.c.thread_id);
            p.shown = (0..p.items.len()).collect();
            p.sel = 0;
        }
        self.split_pick = true;
    }

    pub fn open_resume(&mut self) {
        self.split_pick = false;
        let mut list = self.store.list();
        // This folder first, then everything else; newest first within each.
        list.sort_by_key(|s| (s.cwd.as_deref() != Some(&self.c.cwd), -s.updated_at));
        let items = list
            .into_iter()
            .map(|s| Item {
                label: s.title,
                detail: format!("{} · {} · {} msgs", s.cwd.as_deref().map(|c| c.rsplit('/').next().unwrap_or(c)).unwrap_or("no folder"), ago(s.updated_at), s.messages),
                value: s.id,
            })
            .collect();
        self.overlay = Some(Overlay::Picker(Picker::new(PickKind::Resume, "Resume a chat", items, None)));
    }

    fn open_usage(&mut self) {
        let since = crate::usage::utc_hour_days_ago(8);
        let rows = self.ledger.report(&since);
        let mut by_model: std::collections::BTreeMap<String, (u64, u64, u64, u64)> = Default::default();
        for r in &rows {
            let e = by_model.entry(r.model.clone()).or_default();
            e.0 += r.input;
            e.1 += r.cached;
            e.2 += r.output;
            e.3 += r.requests;
        }
        let models: Vec<String> = by_model.keys().cloned().collect();
        let prices = self.pricing.get("openai", &models);
        let mut lines = vec![format!("{:<22} {:>10} {:>10} {:>9} {:>9}", "model", "input", "output", "requests", "cost"), String::new()];
        let (mut total_tok, mut total_cost) = (0u64, 0f64);
        for (m, (input, cached, output, req)) in &by_model {
            let p = prices.get(m);
            let cost = p.map(|p| ((input.saturating_sub(*cached)) as f64 * p.input.unwrap_or(0.0) + *cached as f64 * p.cached.or(p.input).unwrap_or(0.0) + *output as f64 * p.output.unwrap_or(0.0)) / 1e6).unwrap_or(0.0);
            total_tok += input + output;
            total_cost += cost;
            lines.push(format!("{:<22} {:>10} {:>10} {:>9} {:>9}", m.chars().take(22).collect::<String>(), tok(*input), tok(*output), req, format!("${cost:.2}")));
        }
        lines.push(String::new());
        lines.push(format!("{} tokens · ${:.2} at API prices · last 7 days, every Codex client", tok(total_tok), total_cost));
        lines.push("Your ChatGPT plan covers this; the cost shows what the same work would cost through the API.".into());
        self.overlay = Some(Overlay::Pager { title: "Usage".into(), lines, scroll: 0 });
    }

    fn pick(&mut self, kind: PickKind, value: String) {
        match kind {
            PickKind::Resume if self.split_pick => {
                self.split_pick = false;
                self.split((!value.is_empty()).then_some(value.as_str()));
            }
            PickKind::Resume => {
                // Already open in the other pane: just go there.
                if self.other.as_ref().is_some_and(|o| o.thread_id == value) {
                    self.focus_other();
                } else {
                    self.open_thread(&value);
                }
            }
            PickKind::Model => {
                self.c.model = Some(value);
                if let Some(m) = self.current_model().cloned() {
                    if self.c.effort.as_ref().is_none_or(|e| !m.efforts.contains(e)) {
                        self.c.effort = m.default_effort;
                    }
                    self.say(format!("Model: {}", m.name));
                }
            }
            PickKind::Effort => self.c.effort = Some(value),
            PickKind::Access => self.c.access = value,
            PickKind::Account => {
                if value != self.c.account {
                    self.c.account = value;
                    self.c.models.clear();
                    self.c.limit = None;
                    self.c.email = self.accounts.list().into_iter().find(|a| a.id == self.c.account).and_then(|a| a.email);
                    self.load_account();
                    if self.c.thread.is_some() {
                        self.new_chat();
                        self.say("New chat on the other account");
                    }
                }
            }
            PickKind::Command => self.run_command(&value),
        }
    }

    // ---------- input ----------

    pub fn on_event(&mut self, ev: Event) {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k),
            Event::Paste(s) => {
                if let Some(Overlay::Picker(p)) = self.overlay.as_mut() {
                    p.query.push_str(&s);
                    p.filter();
                } else {
                    self.insert(&s.replace("\r\n", "\n").replace('\r', "\n"));
                }
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => self.scroll_by(3),
                MouseEventKind::ScrollDown => self.scroll_by(-3),
                _ => {}
            },
            _ => {}
        }
    }

    fn scroll_by(&mut self, d: isize) {
        match self.overlay.as_mut() {
            Some(Overlay::Pager { scroll, .. }) => *scroll = (*scroll as isize - d).max(0) as usize,
            _ => self.c.scroll = (self.c.scroll as isize + d).max(0) as usize,
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);

        if ctrl && k.code == KeyCode::Char('c') {
            if self.overlay.take().is_some() {
                return;
            }
            if !self.input.is_empty() {
                self.input.clear();
                self.cursor = 0;
                self.update_suggest();
                return;
            }
            if self.busy() {
                self.stop();
                return;
            }
            if self.ctrl_c.is_some_and(|t| t.elapsed().as_millis() < 1500) {
                self.quit = true;
            } else {
                self.ctrl_c = Some(Instant::now());
                self.say("Press ctrl+c again to quit");
            }
            return;
        }
        if ctrl && k.code == KeyCode::Char('d') && self.input.is_empty() {
            self.quit = true;
            return;
        }

        // Overlays take the keys while open.
        if let Some(ov) = self.overlay.as_mut() {
            match ov {
                Overlay::Picker(p) => match k.code {
                    KeyCode::Esc => {
                        self.overlay = None;
                        self.split_pick = false;
                    }
                    KeyCode::Up => p.sel = p.sel.saturating_sub(1),
                    KeyCode::Down => p.sel = (p.sel + 1).min(p.shown.len().saturating_sub(1)),
                    KeyCode::Char('p') if ctrl => p.sel = p.sel.saturating_sub(1),
                    KeyCode::Char('n') if ctrl => p.sel = (p.sel + 1).min(p.shown.len().saturating_sub(1)),
                    KeyCode::Enter => {
                        let (kind, value) = (p.kind, p.selected().map(|i| i.value.clone()));
                        self.overlay = None;
                        if let Some(v) = value {
                            self.pick(kind, v);
                        }
                    }
                    KeyCode::Backspace => {
                        p.query.pop();
                        p.filter();
                    }
                    KeyCode::Char(c) if !ctrl => {
                        p.query.push(c);
                        p.filter();
                    }
                    _ => {}
                },
                Overlay::Pager { scroll, lines, .. } => match k.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.overlay = None,
                    KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => *scroll = (*scroll + 1).min(lines.len().saturating_sub(1)),
                    KeyCode::PageUp => *scroll = scroll.saturating_sub(20),
                    KeyCode::PageDown | KeyCode::Char(' ') => *scroll = (*scroll + 20).min(lines.len().saturating_sub(1)),
                    KeyCode::Home | KeyCode::Char('g') => *scroll = 0,
                    _ => {}
                },
            }
            return;
        }

        // An approval waiting: y / a / n answer it.
        if self.c.live.as_ref().is_some_and(|l| !l.approvals.is_empty()) && self.input.is_empty() {
            match k.code {
                KeyCode::Char('y') | KeyCode::Enter => return self.respond("accept"),
                KeyCode::Char('a') => return self.respond("acceptForSession"),
                KeyCode::Char('n') | KeyCode::Esc => return self.respond("decline"),
                _ => {}
            }
        }

        // Suggestions (slash commands, @ files).
        if !self.suggest.is_empty() {
            match k.code {
                KeyCode::Up => return self.suggest_sel = self.suggest_sel.saturating_sub(1),
                KeyCode::Down => return self.suggest_sel = (self.suggest_sel + 1).min(self.suggest.len() - 1),
                KeyCode::Tab | KeyCode::Enter => return self.accept_suggestion(k.code == KeyCode::Enter),
                KeyCode::Esc => {
                    self.suggest.clear();
                    return;
                }
                _ => {}
            }
        }

        match k.code {
            KeyCode::Esc if self.busy() => self.stop(),
            KeyCode::Enter if alt || k.modifiers.contains(KeyModifiers::SHIFT) => self.insert("\n"),
            KeyCode::Char('j') if ctrl => self.insert("\n"),
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.input);
                self.cursor = 0;
                self.hist = None;
                self.suggest.clear();
                if text.trim_start().starts_with('/') && !text.contains('\n') && !text.trim().contains(' ') {
                    self.run_command(text.trim());
                } else {
                    self.send(text);
                }
            }
            KeyCode::Char('o') if ctrl => self.details = !self.details,
            KeyCode::Char('\\') if ctrl => {
                if self.other.is_some() {
                    self.focus_other();
                } else {
                    self.split(None);
                }
            }
            KeyCode::Char('4') if ctrl => self.split(None), // some terminals send ctrl+\\ as ctrl+4
            KeyCode::Left | KeyCode::Right if alt && self.other.is_some() => {
                let want_right = k.code == KeyCode::Right;
                if want_right != self.right {
                    self.focus_other();
                }
            }
            KeyCode::F(2) => self.focus_other(),
            KeyCode::Char('r') if ctrl => self.open_resume(),
            KeyCode::Char('n') if ctrl && self.input.is_empty() => self.new_chat(),
            KeyCode::Char('a') if ctrl => self.cursor = self.line_start(),
            KeyCode::Char('e') if ctrl => self.cursor = self.line_end(),
            KeyCode::Char('u') if ctrl => {
                let start = self.line_start();
                self.remove_range(start, self.cursor);
            }
            KeyCode::Char('w') if ctrl => {
                let chars: Vec<char> = self.input.chars().collect();
                let mut i = self.cursor;
                while i > 0 && chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                while i > 0 && !chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                self.remove_range(i, self.cursor);
            }
            KeyCode::Char('l') if ctrl => self.c.scroll = 0,
            KeyCode::Char(c) if !ctrl => self.insert(&c.to_string()),
            KeyCode::Backspace if self.cursor > 0 => {
                if alt {
                    let start = self.cursor.saturating_sub(1);
                    self.remove_range(start, self.cursor);
                } else {
                    self.remove_range(self.cursor - 1, self.cursor);
                }
            }
            KeyCode::Delete if self.cursor < self.input.chars().count() => self.remove_range(self.cursor, self.cursor + 1),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.chars().count()),
            KeyCode::Home => self.cursor = self.line_start(),
            KeyCode::End => self.cursor = self.line_end(),
            KeyCode::Up => {
                if self.input.contains('\n') && self.hist.is_none() {
                    self.move_line(-1);
                } else if !self.history.is_empty() && (self.input.is_empty() || self.hist.is_some()) {
                    let i = self.hist.map_or(0, |i| (i + 1).min(self.history.len() - 1));
                    self.hist = Some(i);
                    self.input = self.history[i].clone();
                    self.cursor = self.input.chars().count();
                } else {
                    self.scroll_by(1);
                }
            }
            KeyCode::Down => {
                if self.input.contains('\n') && self.hist.is_none() {
                    self.move_line(1);
                } else if let Some(i) = self.hist {
                    if i == 0 {
                        self.hist = None;
                        self.input.clear();
                    } else {
                        self.hist = Some(i - 1);
                        self.input = self.history[i - 1].clone();
                    }
                    self.cursor = self.input.chars().count();
                } else {
                    self.scroll_by(-1);
                }
            }
            KeyCode::PageUp => self.scroll_by(15),
            KeyCode::PageDown => self.scroll_by(-15),
            _ => {}
        }
        self.update_suggest();
    }

    fn byte_at(&self, char_idx: usize) -> usize {
        self.input.char_indices().nth(char_idx).map_or(self.input.len(), |(b, _)| b)
    }

    fn insert(&mut self, s: &str) {
        let at = self.byte_at(self.cursor);
        self.input.insert_str(at, s);
        self.cursor += s.chars().count();
        self.update_suggest();
    }

    fn remove_range(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let (a, b) = (self.byte_at(from), self.byte_at(to));
        self.input.replace_range(a..b, "");
        self.cursor = from;
    }

    fn line_start(&self) -> usize {
        let chars: Vec<char> = self.input.chars().collect();
        (0..self.cursor).rev().find(|&i| chars[i] == '\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self) -> usize {
        let chars: Vec<char> = self.input.chars().collect();
        (self.cursor..chars.len()).find(|&i| chars[i] == '\n').unwrap_or(chars.len())
    }

    fn move_line(&mut self, d: isize) {
        let col = self.cursor - self.line_start();
        if d < 0 {
            let start = self.line_start();
            if start == 0 {
                return;
            }
            self.cursor = start - 1;
            let prev_start = self.line_start();
            self.cursor = (prev_start + col).min(start - 1);
        } else {
            let end = self.line_end();
            if end >= self.input.chars().count() {
                return;
            }
            self.cursor = end + 1;
            let next_end = self.line_end();
            self.cursor = (end + 1 + col).min(next_end);
        }
    }

    /// The word under the cursor, if it starts with `@`: (start char index, query).
    fn mention(&self) -> Option<(usize, String)> {
        let chars: Vec<char> = self.input.chars().collect();
        let mut i = self.cursor;
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        (chars.get(i) == Some(&'@')).then(|| (i, chars[i + 1..self.cursor].iter().collect()))
    }

    fn update_suggest(&mut self) {
        let prev = self.suggest.first().cloned();
        self.suggest.clear();
        let t = self.input.as_str();
        if t.starts_with('/') && !t.contains(' ') && !t.contains('\n') {
            for i in fuzzy::rank(&t[1..], COMMANDS, |c| c.0) {
                self.suggest.push((COMMANDS[i].0.to_owned(), COMMANDS[i].1.to_owned()));
            }
        } else if let Some((_, q)) = self.mention() {
            self.load_files();
            if let Some(files) = &self.c.files {
                for i in fuzzy::rank(&q, files, |f| f.as_str()).into_iter().take(8) {
                    self.suggest.push((files[i].clone(), String::new()));
                }
            }
        }
        if self.suggest.first() != prev.as_ref() {
            self.suggest_sel = 0;
        }
        self.suggest_sel = self.suggest_sel.min(self.suggest.len().saturating_sub(1));
    }

    fn accept_suggestion(&mut self, run: bool) {
        let Some((value, _)) = self.suggest.get(self.suggest_sel).cloned() else { return };
        if self.input.starts_with('/') {
            self.input.clear();
            self.cursor = 0;
            self.suggest.clear();
            if run {
                self.run_command(&value);
            } else {
                self.insert(&value);
            }
        } else if let Some((start, _)) = self.mention() {
            self.remove_range(start, self.cursor);
            self.insert(&format!("@{value} "));
            self.suggest.clear();
        }
    }
}

// ---------- helpers ----------

pub fn ago(ms: i64) -> String {
    let m = (crate::store::now_ms() - ms) / 60_000;
    match m {
        i64::MIN..=0 => "now".into(),
        1..=59 => format!("{m}m"),
        60..=1439 => format!("{}h", m / 60),
        _ => format!("{}d", m / 1440),
    }
}

pub fn tok(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

pub fn git_branch(dir: &str) -> Option<String> {
    let mut d = Some(std::path::Path::new(dir));
    while let Some(p) = d {
        let git = p.join(".git");
        let head = if git.is_file() {
            let ptr = std::fs::read_to_string(&git).ok()?;
            std::fs::read_to_string(p.join(ptr.trim().strip_prefix("gitdir:")?.trim()).join("HEAD")).ok()?
        } else if git.is_dir() {
            std::fs::read_to_string(git.join("HEAD")).ok()?
        } else {
            d = p.parent();
            continue;
        };
        let head = head.trim();
        return Some(head.strip_prefix("ref: refs/heads/").map(str::to_owned).unwrap_or_else(|| head.chars().take(7).collect()));
    }
    None
}

fn walk(root: &str, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || ["node_modules", "target", "dist", "build", "vendor"].contains(&name.as_str()) {
                continue;
            }
            let path = e.path();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}

fn history_path() -> Option<std::path::PathBuf> {
    Some(crate::data_dir()?.join("cli-history.json"))
}

fn load_history() -> Vec<String> {
    history_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_history(h: &[String]) {
    if let (Some(p), Ok(b)) = (history_path(), serde_json::to_vec(h)) {
        let _ = std::fs::write(p, b);
    }
}

fn help() -> Vec<String> {
    let mut l = vec![
        "Keys".to_string(),
        "  enter            send".into(),
        "  alt+enter, ctrl+j  new line".into(),
        "  esc              stop the turn (or deny an approval)".into(),
        "  y / a / n        approve · approve for this chat · deny".into(),
        "  ↑ ↓              history (empty prompt), scroll otherwise".into(),
        "  pgup pgdn, wheel scroll the chat".into(),
        "  ctrl+o           show or hide every step".into(),
        "  ctrl+r           resume a chat".into(),
        "  ctrl+n           new chat".into(),
        "  ctrl+\\           split: a second chat side by side".into(),
        "  alt+← / alt+→    switch pane (also F2)".into(),
        "  ctrl+c           clear the prompt, stop, or quit (twice)".into(),
        "  @                mention a file".into(),
        String::new(),
        "Commands".into(),
    ];
    for (c, d) in COMMANDS {
        l.push(format!("  {c:<16} {d}"));
    }
    l.push(String::new());
    l.push("Chats, accounts and usage are shared with the hrns desktop app and phone.".into());
    l
}
