//! Talks to `codex app-server` (JSON-RPC over stdio) so chats run on the
//! user's ChatGPT subscription with the same agent, sandbox and approvals as
//! the official Codex clients.
//!
//! Battery notes:
//! - the sidecar is spawned lazily and killed after `IDLE_SHUTDOWN` with no turns;
//! - nothing polls: everything is driven by stdout lines from the sidecar;
//! - streamed output is persisted once per turn, never per token.

use crate::store::{now_ms, Message, Store, Thread, Usage};
use crate::usage::UsageLedger;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

const IDLE_SHUTDOWN: Duration = Duration::from_secs(10 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Command output kept in memory / on disk per command (tail is kept).
const MAX_OUTPUT: usize = 64 * 1024;

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum ChatEvent {
    /// A part was created or replaced (text, reasoning, command, fileChange, tool).
    Part { thread_id: String, part: Value },
    /// Text appended to `part[field]`.
    Delta { thread_id: String, part_id: String, field: String, text: String },
    Notice { thread_id: String, message: String },
    Approval { thread_id: String, approval: Value },
    ApprovalResolved { thread_id: String, key: String },
    Done { thread_id: String, message: Option<Message> },
}

/// Codex-style access presets: (thread sandbox mode, approval policy, turn sandbox policy).
fn access_policy(access: &str) -> (&'static str, &'static str, Value) {
    match access {
        "read-only" => ("read-only", "on-request", json!({ "type": "readOnly" })),
        "full" => ("danger-full-access", "never", json!({ "type": "dangerFullAccess" })),
        _ => ("workspace-write", "on-request", json!({ "type": "workspaceWrite" })),
    }
}

struct Conn {
    stdin: Mutex<ChildStdin>,
    child: StdMutex<Option<Child>>,
    pending: StdMutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    next_id: AtomicU64,
    alive: AtomicBool,
}

impl Conn {
    async fn write(&self, msg: Value) -> Result<(), String> {
        let mut line = msg.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        if !self.alive.load(Ordering::Acquire) {
            return Err("Codex is not running".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(e) = self.write(json!({ "id": id, "method": method, "params": params })).await {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("Codex exited unexpectedly".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("Codex did not answer `{method}` in time"))
            }
        }
    }

    fn kill(&self) {
        self.alive.store(false, Ordering::Release);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.start_kill();
        }
        for (_, tx) in self.pending.lock().unwrap().drain() {
            let _ = tx.send(Err("Codex exited".into()));
        }
    }
}

/// An in-flight turn, keyed by Codex thread id.
struct Turn {
    local_id: String,
    turn_id: Option<String>,
    model: String,
    parts: Vec<Value>,
    usage: Usage,
    error: Option<String>,
}

impl Turn {
    fn part_mut(&mut self, id: &str) -> Option<&mut Value> {
        self.parts.iter_mut().find(|p| p["id"] == id)
    }

    fn upsert(&mut self, mut part: Value) -> Value {
        let id = part["id"].as_str().unwrap_or_default().to_owned();
        if let Some(existing) = self.part_mut(&id) {
            // Keep text we streamed if the completed item came back without it.
            for field in ["text", "output"] {
                if part[field].as_str().is_none_or(str::is_empty) && existing[field].is_string() {
                    part[field] = existing[field].clone();
                }
            }
            *existing = part.clone();
        } else {
            self.parts.push(part.clone());
        }
        part
    }
}

/// A side turn that names a chat; its output never reaches the chat itself.
struct TitleJob {
    text: String,
    done: Option<oneshot::Sender<Result<String, String>>>,
}

struct PendingApproval {
    local_id: String,
    ctid: String,
    rpc_id: Value,
    method: String,
    params: Value,
    view: Value,
}

#[derive(Default)]
struct State {
    local_to_codex: HashMap<String, String>,
    codex_to_local: HashMap<String, String>,
    turns: HashMap<String, Turn>,
    approvals: HashMap<String, PendingApproval>,
    /// Title jobs, keyed by their own Codex thread id.
    titles: HashMap<String, TitleJob>,
    /// Last cumulative token usage reported per Codex thread (to compute per-turn deltas).
    token_totals: HashMap<String, Usage>,
    last_activity: Option<Instant>,
}

pub struct Codex {
    app: AppHandle,
    store: Arc<Store>,
    /// hrns account id, and its CODEX_HOME (None = the user's default ~/.codex).
    account_id: String,
    home: Option<PathBuf>,
    ledger: Arc<UsageLedger>,
    conn: Mutex<Option<Arc<Conn>>>,
    state: StdMutex<State>,
}

impl Codex {
    pub fn new(
        app: AppHandle,
        store: Arc<Store>,
        ledger: Arc<UsageLedger>,
        account_id: String,
        home: Option<PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self { app, store, account_id, home, ledger, conn: Mutex::new(None), state: StdMutex::new(State::default()) })
    }

    pub fn has_approval(&self, key: &str) -> bool {
        self.state.lock().unwrap().approvals.contains_key(key)
    }

    pub async fn shutdown(&self) {
        if let Some(c) = self.conn.lock().await.take() {
            c.kill();
        }
    }

    // ---------- public API (called from Tauri commands) ----------

    pub async fn models(self: &Arc<Self>) -> Result<Value, String> {
        let conn = self.conn().await?;
        let r = conn.request("model/list", json!({})).await?;
        let models: Vec<Value> = r["data"]
            .as_array()
            .map(|a| a.iter().filter(|m| !m["hidden"].as_bool().unwrap_or(false)).cloned().collect())
            .unwrap_or_default();
        Ok(Value::Array(models))
    }

    pub async fn account(self: &Arc<Self>) -> Result<Value, String> {
        let conn = self.conn().await?;
        let account = conn.request("account/read", json!({})).await?;
        let limits = conn.request("account/rateLimits/read", json!({})).await.unwrap_or(Value::Null);
        Ok(json!({ "account": account["account"], "rateLimits": limits["rateLimits"] }))
    }

    pub async fn login(self: &Arc<Self>) -> Result<String, String> {
        let conn = self.conn().await?;
        let r = conn.request("account/login/start", json!({ "type": "chatgpt" })).await?;
        r["authUrl"].as_str().map(str::to_owned).ok_or_else(|| "No login URL returned".into())
    }

    pub async fn logout(self: &Arc<Self>) -> Result<(), String> {
        let conn = self.conn().await?;
        conn.request("account/logout", json!({})).await.map(|_| ())
    }

    /// Parts and approvals of the in-flight turn, if any.
    pub fn live(&self, local_id: &str) -> Option<Value> {
        let st = self.state.lock().unwrap();
        let turn = st.turns.values().find(|t| t.local_id == local_id)?;
        let approvals: Vec<Value> =
            st.approvals.values().filter(|a| a.local_id == local_id).map(|a| a.view.clone()).collect();
        Some(json!({ "parts": turn.parts, "approvals": approvals }))
    }

    fn is_streaming(&self, local_id: &str) -> bool {
        self.state.lock().unwrap().turns.values().any(|t| t.local_id == local_id)
    }

    /// Append a user message to a (possibly new) local thread and start a turn.
    /// Output arrives through `chat` events.
    pub async fn send(
        self: &Arc<Self>,
        thread_id: Option<String>,
        text: String,
        model: String,
        effort: Option<String>,
        access: String,
        cwd: Option<String>,
        worktree: Option<crate::worktree::Worktree>,
        attachments: Vec<String>,
    ) -> Result<Thread, String> {
        let account = (self.account_id != "default").then(|| self.account_id.clone());
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err("Message is empty".into());
        }
        let now = now_ms();
        let id = thread_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if self.is_streaming(&id) {
            return Err("Still working — stop it first".into());
        }
        if self.store.get(&id).is_none() {
            let cwd = cwd.filter(|c| Path::new(c).is_dir()).ok_or("Choose a project folder first")?;
            self.store.insert(Thread {
                id: id.clone(),
                title: make_title(&text),
                model: model.clone(),
                effort: effort.clone(),
                access: access.clone(),
                cwd: Some(cwd),
                account,
                worktree,
                usage: Default::default(),
                created_at: now,
                updated_at: now,
                messages: vec![],
            });
        }
        let user_msg = Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            text: text.clone(),
            parts: None,
            model: None,
            error: None,
            attachments: (!attachments.is_empty()).then(|| attachments.clone()),
            created_at: now,
        };
        let thread = self
            .store
            .update(&id, |t| {
                t.messages.push(user_msg);
                t.model = model.clone();
                t.effort = effort.clone();
                t.access = access.clone();
                t.updated_at = now;
                t.clone()
            })
            .ok_or("Thread vanished")?;

        let me = self.clone();
        let started = async {
            let conn = me.conn().await?;
            let ctid = me.ensure_codex_thread(&conn, &thread).await?;
            // Register the turn before starting it so no event is missed.
            me.state.lock().unwrap().turns.insert(
                ctid.clone(),
                Turn {
                    local_id: id.clone(),
                    turn_id: None,
                    model: model.clone(),
                    parts: vec![],
                    usage: Usage::default(),
                    error: None,
                },
            );
            let (_, approval, sandbox) = access_policy(&access);
            let mut params = json!({
                "threadId": ctid,
                "input": turn_input(&text, &attachments),
                "model": model,
                "approvalPolicy": approval,
                "sandboxPolicy": sandbox,
                "summary": "auto",
            });
            if let Some(e) = &effort {
                params["effort"] = json!(e);
            }
            match conn.request("turn/start", params).await {
                Ok(r) => {
                    let mut st = me.state.lock().unwrap();
                    st.last_activity = Some(Instant::now());
                    if let (Some(t), Some(tid)) = (st.turns.get_mut(&ctid), r["turn"]["id"].as_str()) {
                        t.turn_id.get_or_insert_with(|| tid.to_owned());
                    }
                    Ok(())
                }
                Err(e) => {
                    me.state.lock().unwrap().turns.remove(&ctid);
                    Err(e)
                }
            }
        }
        .await;

        if let Err(e) = started {
            self.finish(&id, vec![], model, Usage::default(), Some(e));
            self.schedule_idle_shutdown();
        }
        Ok(thread)
    }

    /// Name a chat from its first message, like Codex does: a separate read-only thread on
    /// `model` with no tools, whose reply becomes the title. Saves it and emits `thread-title`.
    pub async fn generate_title(
        self: &Arc<Self>,
        local_id: String,
        first: String,
        model: String,
        effort: Option<String>,
    ) -> Result<String, String> {
        let conn = self.conn().await?;
        let cwd = self
            .store
            .get(&local_id)
            .and_then(|t| t.cwd)
            .unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".into()));
        let r = conn
            .request(
                "thread/start",
                json!({ "ephemeral": true, "approvalPolicy": "never", "sandbox": "read-only", "cwd": cwd, "model": model, "serviceName": "hrns" }),
            )
            .await?;
        let ctid = r["thread"]["id"].as_str().ok_or("Codex returned no thread id")?.to_owned();

        let (tx, rx) = oneshot::channel();
        self.state.lock().unwrap().titles.insert(ctid.clone(), TitleJob { text: String::new(), done: Some(tx) });
        let excerpt: String = first.chars().take(2000).collect();
        let prompt = format!(
            "You name chat sessions. Write a short title (2-6 words, sentence case, no quotes, no trailing punctuation) \
             for a conversation with a coding agent that starts with the message below. Reply with the title only; \
             do not run any tools or commands.\n\n<message>\n{excerpt}\n</message>"
        );
        let mut params = json!({
            "threadId": ctid,
            "input": [{ "type": "text", "text": prompt }],
            "model": model,
            "approvalPolicy": "never",
            "sandboxPolicy": { "type": "readOnly" },
        });
        if let Some(e) = &effort {
            params["effort"] = json!(e);
        }
        let reply = match conn.request("turn/start", params).await {
            Ok(_) => tokio::time::timeout(Duration::from_secs(90), rx)
                .await
                .map_err(|_| "Naming the chat took too long".to_owned())
                .and_then(|r| r.map_err(|_| "Codex exited".to_owned()))
                .and_then(|r| r),
            Err(e) => Err(e),
        };
        {
            let mut st = self.state.lock().unwrap();
            st.titles.remove(&ctid);
            st.token_totals.remove(&ctid);
            st.last_activity = Some(Instant::now());
        }
        self.schedule_idle_shutdown();

        let title = clean_title(&reply?).ok_or("The model returned no title")?;
        self.store.update(&local_id, |t| t.title = title.clone()).ok_or("Thread vanished")?;
        let _ = self.app.emit("thread-title", json!({ "threadId": local_id, "title": title }));
        Ok(title)
    }

    /// Route a notification to its title job, if it belongs to one.
    fn on_title_event(&self, method: &str, ctid: &str, p: &Value) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(job) = st.titles.get_mut(ctid) else { return false };
        match method {
            "item/agentMessage/delta" => job.text.push_str(p["delta"].as_str().unwrap_or_default()),
            "item/completed" if p["item"]["type"] == "agentMessage" => {
                if let Some(t) = p["item"]["text"].as_str().filter(|t| !t.is_empty()) {
                    job.text = t.to_owned();
                }
            }
            "error" if !p["willRetry"].as_bool().unwrap_or(false) => {
                let message = p["error"]["message"].as_str().unwrap_or("Codex error").to_owned();
                if let Some(tx) = job.done.take() {
                    let _ = tx.send(Err(message));
                }
            }
            "turn/completed" => {
                if let Some(tx) = job.done.take() {
                    let ok = p["turn"]["status"].as_str() != Some("failed");
                    let _ = tx.send(if ok { Ok(job.text.clone()) } else { Err("Naming the chat failed".into()) });
                }
            }
            _ => {}
        }
        true
    }

    pub async fn stop(self: &Arc<Self>, local_id: &str) -> Result<(), String> {
        let target = {
            let st = self.state.lock().unwrap();
            st.turns
                .iter()
                .find(|(_, t)| t.local_id == local_id)
                .map(|(ctid, t)| (ctid.clone(), t.turn_id.clone()))
        };
        let Some((ctid, Some(turn_id))) = target else { return Ok(()) };
        let conn = self.conn().await?;
        conn.request("turn/interrupt", json!({ "threadId": ctid, "turnId": turn_id })).await.map(|_| ())
    }

    /// Answer an approval request. `decision`: accept | acceptForSession | decline | cancel.
    pub async fn respond(self: &Arc<Self>, key: &str, decision: &str) -> Result<(), String> {
        let Some(a) = self.state.lock().unwrap().approvals.remove(key) else { return Ok(()) };
        let result = if a.method == "item/permissions/requestApproval" {
            let granted = matches!(decision, "accept" | "acceptForSession");
            json!({
                "permissions": if granted { a.params["permissions"].clone() } else { json!({}) },
                "scope": if decision == "acceptForSession" { "session" } else { "turn" },
            })
        } else {
            json!({ "decision": decision })
        };
        self.emit(ChatEvent::ApprovalResolved { thread_id: a.local_id.clone(), key: key.to_owned() });
        let conn = self.conn().await?;
        conn.write(json!({ "id": a.rpc_id, "result": result })).await?;
        if decision == "cancel" {
            let turn_id = self.state.lock().unwrap().turns.get(&a.ctid).and_then(|t| t.turn_id.clone());
            if let Some(turn_id) = turn_id {
                let _ = conn.request("turn/interrupt", json!({ "threadId": a.ctid, "turnId": turn_id })).await;
            }
        }
        Ok(())
    }

    /// Drop the Codex-side thread for a deleted local thread.
    pub fn forget(&self, local_id: &str) {
        let mut st = self.state.lock().unwrap();
        if let Some(ctid) = st.local_to_codex.remove(local_id) {
            st.codex_to_local.remove(&ctid);
        }
    }

    // ---------- sidecar lifecycle ----------

    async fn conn(self: &Arc<Self>) -> Result<Arc<Conn>, String> {
        let mut guard = self.conn.lock().await;
        if let Some(c) = guard.as_ref() {
            if c.alive.load(Ordering::Acquire) {
                return Ok(c.clone());
            }
        }
        let c = self.spawn().await?;
        *guard = Some(c.clone());
        Ok(c)
    }

    async fn spawn(self: &Arc<Self>) -> Result<Arc<Conn>, String> {
        let path_env = login_path().await;
        let (bin, extra_env) = resolve_codex(&path_env).ok_or(
            "Couldn't find the `codex` CLI. Install it with `npm i -g @openai/codex` (or set HRNS_CODEX_BIN).",
        )?;
        let mut cmd = Command::new(&bin);
        cmd.arg("app-server")
            .env("PATH", &path_env)
            .envs(extra_env)
            .envs(self.home.iter().map(|h| ("CODEX_HOME", h.as_os_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("Failed to start {}: {e}", bin.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;

        let conn = Arc::new(Conn {
            stdin: Mutex::new(stdin),
            child: StdMutex::new(Some(child)),
            pending: StdMutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
        });

        let me = self.clone();
        let reader_conn = conn.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                me.on_line(&reader_conn, &line).await;
            }
            me.on_exit(&reader_conn);
        });

        conn.request(
            "initialize",
            json!({ "clientInfo": { "name": "hrns", "title": "hrns", "version": env!("CARGO_PKG_VERSION") } }),
        )
        .await?;
        conn.write(json!({ "method": "initialized" })).await?;

        self.state.lock().unwrap().last_activity = Some(Instant::now());
        self.schedule_idle_shutdown();
        Ok(conn)
    }

    fn on_exit(&self, conn: &Arc<Conn>) {
        conn.kill();
        let orphaned: Vec<Turn> = {
            let mut st = self.state.lock().unwrap();
            st.local_to_codex.clear();
            st.codex_to_local.clear();
            st.token_totals.clear();
            st.approvals.clear();
            st.turns.drain().map(|(_, t)| t).collect()
        };
        for t in orphaned {
            self.finish(&t.local_id, t.parts, t.model, t.usage, Some("Codex exited unexpectedly".into()));
        }
    }

    /// Kill the sidecar once nothing has happened for `IDLE_SHUTDOWN`.
    /// One sleeping task per activity burst — no periodic wakeups.
    fn schedule_idle_shutdown(self: &Arc<Self>) {
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(IDLE_SHUTDOWN).await;
            let idle = {
                let st = me.state.lock().unwrap();
                st.turns.is_empty() && st.last_activity.is_some_and(|t| t.elapsed() >= IDLE_SHUTDOWN)
            };
            if idle {
                if let Some(c) = me.conn.lock().await.take() {
                    c.kill();
                }
                let mut st = me.state.lock().unwrap();
                st.local_to_codex.clear();
                st.codex_to_local.clear();
                st.token_totals.clear();
            }
        });
    }

    /// Map a local thread onto a live Codex thread, re-creating it (with the
    /// prior conversation injected) when the sidecar was restarted.
    async fn ensure_codex_thread(&self, conn: &Arc<Conn>, thread: &Thread) -> Result<String, String> {
        if let Some(ctid) = self.state.lock().unwrap().local_to_codex.get(&thread.id) {
            return Ok(ctid.clone());
        }
        let (sandbox, approval, _) = access_policy(&thread.access);
        let cwd = thread.cwd.clone().unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".into()));
        let r = conn
            .request(
                "thread/start",
                json!({
                    "ephemeral": true,
                    "approvalPolicy": approval,
                    "sandbox": sandbox,
                    "cwd": cwd,
                    "model": thread.model,
                    "serviceName": "hrns",
                }),
            )
            .await?;
        let ctid = r["thread"]["id"].as_str().ok_or("Codex returned no thread id")?.to_owned();

        // Everything except the message we're about to send.
        let history = &thread.messages[..thread.messages.len().saturating_sub(1)];
        let items: Vec<Value> = history
            .iter()
            .filter(|m| !m.text.is_empty())
            .map(|m| {
                let (role, kind) = if m.role == "user" { ("user", "input_text") } else { ("assistant", "output_text") };
                json!({ "type": "message", "role": role, "content": [{ "type": kind, "text": m.text }] })
            })
            .collect();
        if !items.is_empty() {
            conn.request("thread/inject_items", json!({ "threadId": ctid, "items": items })).await?;
        }

        let mut st = self.state.lock().unwrap();
        st.local_to_codex.insert(thread.id.clone(), ctid.clone());
        st.codex_to_local.insert(ctid.clone(), thread.id.clone());
        Ok(ctid)
    }

    // ---------- incoming messages ----------

    async fn on_line(self: &Arc<Self>, conn: &Arc<Conn>, line: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(line) else { return };
        match (msg["method"].as_str(), msg.get("id")) {
            // response to one of our requests
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else { return };
                if let Some(tx) = conn.pending.lock().unwrap().remove(&id) {
                    let r = match msg.get("error") {
                        Some(err) => Err(err["message"].as_str().unwrap_or("Codex error").to_owned()),
                        None => Ok(msg["result"].clone()),
                    };
                    let _ = tx.send(r);
                }
            }
            (Some(method), Some(id)) => self.on_server_request(conn, method, id.clone(), &msg["params"]).await,
            (Some(method), None) => self.on_notification(method, &msg["params"]),
            _ => {}
        }
    }

    async fn on_server_request(self: &Arc<Self>, conn: &Arc<Conn>, method: &str, rpc_id: Value, p: &Value) {
        let kind = match method {
            "item/commandExecution/requestApproval" => "command",
            "item/fileChange/requestApproval" => "fileChange",
            "item/permissions/requestApproval" => "permissions",
            _ => {
                let _ = conn
                    .write(json!({ "id": rpc_id, "error": { "code": -32601, "message": "not supported by hrns" } }))
                    .await;
                return;
            }
        };
        let ctid = p["threadId"].as_str().unwrap_or_default().to_owned();
        let key = uuid::Uuid::new_v4().to_string();
        let mut st = self.state.lock().unwrap();
        let Some(local_id) = st.codex_to_local.get(&ctid).cloned() else { return };
        // File-change approvals reference an item we already showed; include its diff.
        let changes = st
            .turns
            .get_mut(&ctid)
            .and_then(|t| t.part_mut(p["itemId"].as_str().unwrap_or_default()).map(|part| part["changes"].clone()))
            .unwrap_or(Value::Null);
        let view = json!({
            "key": key,
            "kind": kind,
            "command": p["command"],
            "cwd": p["cwd"],
            "reason": p["reason"],
            "grantRoot": p["grantRoot"],
            "permissions": p["permissions"],
            "changes": changes,
        });
        st.approvals.insert(
            key,
            PendingApproval {
                local_id: local_id.clone(),
                ctid,
                rpc_id,
                method: method.to_owned(),
                params: p.clone(),
                view: view.clone(),
            },
        );
        drop(st);
        self.emit(ChatEvent::Approval { thread_id: local_id, approval: view });
    }

    fn on_notification(self: &Arc<Self>, method: &str, p: &Value) {
        let ctid = p["threadId"].as_str().unwrap_or_default();
        if self.on_title_event(method, ctid, p) {
            return;
        }
        match method {
            "turn/started" => {
                let mut st = self.state.lock().unwrap();
                if let (Some(t), Some(id)) = (st.turns.get_mut(ctid), p["turn"]["id"].as_str()) {
                    t.turn_id.get_or_insert_with(|| id.to_owned());
                }
            }
            "item/started" | "item/completed" => {
                let Some(part) = item_to_part(&p["item"]) else { return };
                let mut st = self.state.lock().unwrap();
                let Some(t) = st.turns.get_mut(ctid) else { return };
                let part = t.upsert(part);
                let local = t.local_id.clone();
                drop(st);
                self.emit(ChatEvent::Part { thread_id: local, part });
            }
            "item/agentMessage/delta" => self.append(ctid, p, "text", "text", ""),
            "item/reasoning/summaryTextDelta" => self.append(ctid, p, "reasoning", "text", ""),
            "item/reasoning/summaryPartAdded" => self.append(ctid, p, "reasoning", "text", "\n\n"),
            "item/commandExecution/outputDelta" => self.append(ctid, p, "command", "output", ""),
            "error" => {
                let message = p["error"]["message"].as_str().unwrap_or("Codex error").to_owned();
                let retrying = p["willRetry"].as_bool().unwrap_or(false);
                let mut st = self.state.lock().unwrap();
                let Some(t) = st.turns.get_mut(ctid) else { return };
                if !retrying {
                    t.error = Some(message.clone());
                }
                let local = t.local_id.clone();
                drop(st);
                if retrying {
                    self.emit(ChatEvent::Notice { thread_id: local, message: format!("{message} — retrying…") });
                }
            }
            "serverRequest/resolved" => {
                // Codex resolved an approval on its own (e.g. turn interrupted).
                let rpc_id = &p["requestId"];
                let mut st = self.state.lock().unwrap();
                let Some(key) = st.approvals.iter().find(|(_, a)| &a.rpc_id == rpc_id).map(|(k, _)| k.clone()) else {
                    return;
                };
                let a = st.approvals.remove(&key).unwrap();
                drop(st);
                self.emit(ChatEvent::ApprovalResolved { thread_id: a.local_id, key });
            }
            "turn/completed" => {
                let (turn, stale): (Option<Turn>, Vec<(String, String)>) = {
                    let mut st = self.state.lock().unwrap();
                    let stale = st
                        .approvals
                        .iter()
                        .filter(|(_, a)| a.ctid == ctid)
                        .map(|(k, a)| (k.clone(), a.local_id.clone()))
                        .collect();
                    st.approvals.retain(|_, a| a.ctid != ctid);
                    (st.turns.remove(ctid), stale)
                };
                for (key, local) in stale {
                    self.emit(ChatEvent::ApprovalResolved { thread_id: local, key });
                }
                let Some(t) = turn else { return };
                let turn = &p["turn"];
                let error = t.error.or_else(|| match turn["status"].as_str() {
                    Some("failed") => Some(turn["error"]["message"].as_str().unwrap_or("Turn failed").to_owned()),
                    Some("interrupted") => Some("Stopped".to_owned()),
                    _ => None,
                });
                self.finish(&t.local_id, t.parts, t.model, t.usage, error);
                self.state.lock().unwrap().last_activity = Some(Instant::now());
                self.schedule_idle_shutdown();
            }
            "thread/tokenUsage/updated" => {
                let u = &p["tokenUsage"]["total"];
                let total = Usage {
                    input: u["inputTokens"].as_u64().unwrap_or(0),
                    cached: u["cachedInputTokens"].as_u64().unwrap_or(0),
                    output: u["outputTokens"].as_u64().unwrap_or(0),
                    reasoning: u["reasoningOutputTokens"].as_u64().unwrap_or(0),
                };
                let mut st = self.state.lock().unwrap();
                let prev = st.token_totals.insert(ctid.to_owned(), total).unwrap_or_default();
                if let Some(t) = st.turns.get_mut(ctid) {
                    t.usage.add(&Usage {
                        input: total.input.saturating_sub(prev.input),
                        cached: total.cached.saturating_sub(prev.cached),
                        output: total.output.saturating_sub(prev.output),
                        reasoning: total.reasoning.saturating_sub(prev.reasoning),
                    });
                }
            }
            "account/rateLimits/updated" => {
                let _ = self.app.emit("rate-limits", json!({ "account": self.account_id, "rateLimits": p["rateLimits"] }));
            }
            "account/updated" | "account/login/completed" => {
                let _ = self.app.emit("account-changed", json!({ "account": self.account_id }));
            }
            _ => {}
        }
    }

    /// Append streamed text to `part[field]`, creating the part if needed.
    fn append(&self, ctid: &str, p: &Value, kind: &str, field: &str, fixed: &str) {
        let part_id = p["itemId"].as_str().unwrap_or_default();
        let text = if fixed.is_empty() { p["delta"].as_str().unwrap_or_default() } else { fixed };
        let mut st = self.state.lock().unwrap();
        let Some(t) = st.turns.get_mut(ctid) else { return };
        if t.part_mut(part_id).is_none() {
            t.parts.push(json!({ "type": kind, "id": part_id, field: "" }));
        }
        let part = t.part_mut(part_id).unwrap();
        let current = part[field].as_str().unwrap_or_default();
        if fixed.is_empty() || !current.is_empty() {
            part[field] = json!(cap_tail(format!("{current}{text}")));
        } else {
            return;
        }
        let local = t.local_id.clone();
        drop(st);
        self.emit(ChatEvent::Delta {
            thread_id: local,
            part_id: part_id.to_owned(),
            field: field.to_owned(),
            text: text.to_owned(),
        });
    }

    /// Persist the assistant turn (or error) and tell the UI it ended.
    fn finish(&self, local_id: &str, parts: Vec<Value>, model: String, usage: Usage, error: Option<String>) {
        let text = parts
            .iter()
            .filter(|p| p["type"] == "text")
            .filter_map(|p| p["text"].as_str())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let message = (!parts.is_empty() || error.is_some()).then(|| Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "assistant".into(),
            text,
            parts: Some(parts),
            model: Some(model.clone()),
            error,
            attachments: None,
            created_at: now_ms(),
        });
        self.ledger.log(&self.account_id, &model, &usage);
        let m = message.clone();
        self.store.update(local_id, |t| {
            if let Some(m) = m {
                t.messages.push(m);
            }
            t.usage.entry(model.clone()).or_default().add(&usage);
            t.updated_at = now_ms();
        });
        self.emit(ChatEvent::Done { thread_id: local_id.to_owned(), message });
    }

    fn emit(&self, event: ChatEvent) {
        let _ = self.app.emit("chat", event);
    }
}

/// Convert a Codex ThreadItem into the compact part shape the UI renders.
fn item_to_part(item: &Value) -> Option<Value> {
    let id = item["id"].clone();
    let status = item["status"].clone();
    Some(match item["type"].as_str()? {
        "agentMessage" => json!({ "type": "text", "id": id, "text": item["text"] }),
        "reasoning" => {
            let summary: Vec<&str> =
                item["summary"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            json!({ "type": "reasoning", "id": id, "text": summary.join("\n\n") })
        }
        "commandExecution" => json!({
            "type": "command",
            "id": id,
            "command": item["command"],
            "actions": item["commandActions"],
            "cwd": item["cwd"],
            "output": item["aggregatedOutput"].as_str().map(|s| cap_tail(s.to_owned())),
            "exitCode": item["exitCode"],
            "durationMs": item["durationMs"],
            "status": status,
        }),
        "fileChange" => {
            let changes: Vec<Value> = item["changes"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|c| json!({ "path": c["path"], "kind": c["kind"]["type"], "diff": c["diff"] }))
                        .collect()
                })
                .unwrap_or_default();
            json!({ "type": "fileChange", "id": id, "changes": changes, "status": status })
        }
        "mcpToolCall" => json!({
            "type": "tool",
            "id": id,
            "title": format!("{}.{}", item["server"].as_str().unwrap_or("mcp"), item["tool"].as_str().unwrap_or("tool")),
            "status": status,
            "error": item["error"]["message"],
        }),
        "webSearch" => json!({
            "type": "tool",
            "id": id,
            "title": format!("Searched the web: {}", item["query"].as_str().unwrap_or_default()),
            "status": "completed",
        }),
        "plan" => json!({ "type": "plan", "id": id, "text": item["text"] }),
        _ => return None,
    })
}

fn cap_tail(mut s: String) -> String {
    if s.len() > MAX_OUTPUT {
        let mut cut = s.len() - MAX_OUTPUT;
        while !s.is_char_boundary(cut) {
            cut += 1;
        }
        s.replace_range(..cut, "…\n");
    }
    s
}

/// First line of a model's reply, without the quotes, labels or markdown models like to add.
fn clean_title(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches(['#', '*', '-', ' ']);
    let line = ["Title:", "title:", "TITLE:"].iter().fold(line, |l, p| l.strip_prefix(p).unwrap_or(l)).trim();
    let line = line.trim_matches(['"', '\'', '`', '*', '“', '”', ' ']).trim_end_matches(['.', '!', ':']);
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    (!title.is_empty()).then_some(title)
}

fn make_title(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("New chat").trim();
    let mut title: String = line.chars().take(48).collect();
    if line.chars().count() > 48 {
        title.push('…');
    }
    title
}

fn is_image(path: &str) -> bool {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp")
}

/// The message as Codex input: images go in as images; other files are listed by path for the
/// agent to read itself (it can, from the sandbox, unless they're outside what it may read).
fn turn_input(text: &str, attachments: &[String]) -> Value {
    let (images, files): (Vec<&String>, Vec<&String>) = attachments.iter().partition(|p| is_image(p));
    let mut text = text.to_owned();
    if !files.is_empty() {
        text.push_str("\n\nAttached files:");
        for f in &files {
            text.push_str(&format!("\n- {f}"));
        }
    }
    let mut input = vec![json!({ "type": "text", "text": text })];
    input.extend(images.iter().map(|p| json!({ "type": "localImage", "path": p })));
    Value::Array(input)
}

/// GUI apps launched from Finder get a minimal PATH; ask the login shell once.
pub async fn login_path() -> String {
    static PATH: OnceLock<String> = OnceLock::new();
    if let Some(p) = PATH.get() {
        return p.clone();
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let probe = Command::new(shell)
        .args(["-ilc", "printf '\\n__HRNS_PATH__%s\\n' \"$PATH\""])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let from_shell = tokio::time::timeout(Duration::from_secs(5), probe)
        .await
        .ok()
        .and_then(|r| r.ok())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find_map(|l| l.strip_prefix("__HRNS_PATH__").map(str::to_owned))
        });
    let mut path = from_shell.unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        if !path.split(':').any(|p| p == extra) {
            path.push(':');
            path.push_str(extra);
        }
    }
    PATH.get_or_init(|| path).clone()
}

/// Find the codex binary. Prefers the native binary inside an npm install so
/// we don't keep an extra Node.js process alive just to proxy stdio.
fn resolve_codex(path_env: &str) -> Option<(PathBuf, Vec<(String, String)>)> {
    if let Ok(p) = std::env::var("HRNS_CODEX_BIN") {
        return Some((PathBuf::from(p), vec![]));
    }
    for dir in path_env.split(':').filter(|d| !d.is_empty()) {
        let candidate = Path::new(dir).join("codex");
        if !candidate.is_file() {
            continue;
        }
        let real = std::fs::canonicalize(&candidate).unwrap_or(candidate.clone());
        if real.extension().is_some_and(|e| e == "js") {
            // .../@openai/codex/bin/codex.js -> .../@openai/codex
            if let Some(pkg_root) = real.parent().and_then(Path::parent) {
                if let Some(native) = find_native_codex(pkg_root) {
                    let env = vec![
                        ("CODEX_MANAGED_PACKAGE_ROOT".to_owned(), pkg_root.display().to_string()),
                        ("CODEX_MANAGED_BY_NPM".to_owned(), "1".to_owned()),
                    ];
                    return Some((native, env));
                }
            }
        }
        return Some((candidate, vec![]));
    }
    let app_bundled = PathBuf::from("/Applications/Codex.app/Contents/Resources/codex");
    app_bundled.is_file().then(|| (app_bundled, vec![]))
}

fn find_native_codex(pkg_root: &Path) -> Option<PathBuf> {
    let scoped = pkg_root.join("node_modules/@openai");
    for platform in std::fs::read_dir(&scoped).ok()?.flatten() {
        if !platform.file_name().to_string_lossy().starts_with("codex-") {
            continue;
        }
        for triple in std::fs::read_dir(platform.path().join("vendor")).ok()?.flatten() {
            let bin = triple.path().join("bin/codex");
            if bin.is_file() {
                return Some(bin);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{clean_title, turn_input};
    use serde_json::json;

    #[test]
    fn cleans_model_titles() {
        assert_eq!(clean_title("Fix login redirect bug").as_deref(), Some("Fix login redirect bug"));
        assert_eq!(clean_title("\n\"Add dark mode toggle.\"\n").as_deref(), Some("Add dark mode toggle"));
        assert_eq!(clean_title("Title: **Refactor store**").as_deref(), Some("Refactor store"));
        assert_eq!(clean_title("# Split view for chats\nextra").as_deref(), Some("Split view for chats"));
        assert_eq!(clean_title("  \n "), None);
    }

    #[test]
    fn attachments_input() {
        let v = turn_input("look", &["/a/shot.PNG".into(), "/a/notes.md".into()]);
        assert_eq!(v[0]["text"], "look\n\nAttached files:\n- /a/notes.md");
        assert_eq!(v[1], json!({ "type": "localImage", "path": "/a/shot.PNG" }));
        assert_eq!(turn_input("hi", &[]).as_array().unwrap().len(), 1);
    }
}
