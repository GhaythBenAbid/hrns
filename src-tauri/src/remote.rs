//! Remote control over Tailscale: a small HTTP + WebSocket API so the mobile
//! app can list chats, stream them live, send messages, stop and approve.
//!
//! Safety: the server binds only to this machine's Tailscale IPv4 (never
//! 0.0.0.0, never Funnel), and every request needs the pairing token.
//! It is off by default and costs nothing while idle (async accept only).

use crate::accounts::{Accounts, DEFAULT};
use crate::store::Store;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tokio::sync::{broadcast, oneshot};

const DEFAULT_PORT: u16 = 7788;

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    enabled: bool,
    token: String,
    port: u16,
}

pub struct Remote {
    app: AppHandle,
    path: PathBuf,
    config: Mutex<Config>,
    running: Mutex<Option<(SocketAddr, oneshot::Sender<()>)>>,
    /// Chat events fanned out to connected phones.
    events: broadcast::Sender<String>,
}

#[derive(Clone)]
struct Ctx {
    remote: Arc<Remote>,
}

fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

impl Remote {
    pub fn new(app: AppHandle, data_dir: &std::path::Path) -> Arc<Self> {
        let path = data_dir.join("remote.json");
        let config = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Config { enabled: false, token: new_token(), port: DEFAULT_PORT });
        let (events, _) = broadcast::channel(512);
        Arc::new(Self { app, path, config: Mutex::new(config), running: Mutex::new(None), events })
    }

    pub fn publish(&self, kind: &str, payload: &str) {
        if self.events.receiver_count() > 0 {
            let _ = self.events.send(format!(r#"{{"type":"{kind}","payload":{payload}}}"#));
        }
    }

    /// Show a notification on paired phones.
    pub fn notify(&self, title: &str, body: &str) -> usize {
        let payload = json!({ "title": title, "body": body }).to_string();
        let phones = self.events.receiver_count();
        self.publish("notify", &payload);
        phones
    }

    pub fn should_autostart(&self) -> bool {
        self.config.lock().unwrap().enabled
    }

    pub fn info(&self) -> Value {
        let cfg = self.config.lock().unwrap().clone();
        let addr = self.running.lock().unwrap().as_ref().map(|(a, _)| *a);
        let url = addr.map(|a| format!("http://{a}"));
        json!({
            "enabled": cfg.enabled,
            "running": addr.is_some(),
            "url": url,
            "token": cfg.token,
            "pairUrl": url.as_ref().map(|u| format!("hrns://pair?url={}&token={}", u, cfg.token)),
        })
    }

    pub async fn start(self: &Arc<Self>) -> Result<Value, String> {
        self.stop();
        let ip = tailscale_ip().await.ok_or("Tailscale isn't running (no 100.x address). Open Tailscale and sign in.")?;
        let port = self.config.lock().unwrap().port;
        let listener = tokio::net::TcpListener::bind((ip.as_str(), port))
            .await
            .map_err(|e| format!("Couldn't listen on {ip}:{port}: {e}"))?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;

        let ctx = Ctx { remote: self.clone() };
        let api = Router::new()
            .route("/api/info", get(info))
            .route("/api/threads", get(threads))
            .route("/api/threads/{id}", get(thread))
            .route("/api/models", get(models))
            .route("/api/accounts/{id}/status", get(account_status))
            .route("/api/usage", get(usage))
            .route("/api/pricing", post(pricing))
            .route("/api/notify", post(notify))
            .route("/api/send", post(send))
            .route("/api/stop", post(stop))
            .route("/api/approval", post(approval))
            .route("/api/events", get(events))
            .route("/api/projects", get(projects))
            .route("/api/projects/icon", get(project_icon))
            .route("/api/git/status", get(git_status))
            .route("/api/git/diff", get(git_diff))
            .route("/api/title", post(title))
            .route("/api/terminals", get(terminals).post(terminal_open))
            .route("/api/terminals/{id}", axum::routing::delete(terminal_close))
            .route("/api/terminals/{id}/ws", get(terminal_ws))
            .route("/api/terminal.html", get(terminal_page))
            .layer(middleware::from_fn_with_state(ctx.clone(), auth))
            .with_state(ctx);

        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, api).with_graceful_shutdown(async { let _ = rx.await; }).await;
        });
        *self.running.lock().unwrap() = Some((addr, tx));
        self.config.lock().unwrap().enabled = true;
        self.save();
        Ok(self.info())
    }

    pub fn stop(&self) {
        if let Some((_, tx)) = self.running.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }

    pub fn disable(&self) -> Value {
        self.stop();
        self.config.lock().unwrap().enabled = false;
        self.save();
        self.info()
    }

    /// New token: every paired phone must scan the QR again.
    pub async fn rotate(self: &Arc<Self>) -> Result<Value, String> {
        self.config.lock().unwrap().token = new_token();
        self.save();
        if self.running.lock().unwrap().is_some() {
            return self.start().await;
        }
        Ok(self.info())
    }

    fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(&*self.config.lock().unwrap()) {
            let _ = std::fs::write(&self.path, b);
        }
    }

    fn token(&self) -> String {
        self.config.lock().unwrap().token.clone()
    }
}

async fn tailscale_ip() -> Option<String> {
    for bin in ["/usr/local/bin/tailscale", "/opt/homebrew/bin/tailscale", "/Applications/Tailscale.app/Contents/MacOS/Tailscale"] {
        let Ok(out) = tokio::process::Command::new(bin).args(["ip", "-4"]).output().await else { continue };
        let ip = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().trim().to_owned();
        if ip.starts_with("100.") {
            return Some(ip);
        }
    }
    None
}

// ---------- handlers ----------

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn auth(State(ctx): State<Ctx>, Query(q): Query<TokenQuery>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let expected = ctx.remote.token();
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);
    let given = bearer.or(q.token).unwrap_or_default();
    // Constant-time-ish compare; tokens are random 64-hex.
    let ok = given.len() == expected.len() && given.bytes().zip(expected.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0;
    if !ok {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    next.run(req).await
}

fn err(e: impl ToString) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))).into_response()
}

fn store(ctx: &Ctx) -> Arc<Store> {
    ctx.remote.app.state::<Arc<Store>>().inner().clone()
}

fn accounts(ctx: &Ctx) -> Arc<Accounts> {
    ctx.remote.app.state::<Arc<Accounts>>().inner().clone()
}

async fn info(State(ctx): State<Ctx>) -> Json<Value> {
    Json(json!({
        "name": "hrns",
        "version": env!("CARGO_PKG_VERSION"),
        "host": std::process::Command::new("scutil").args(["--get", "ComputerName"]).output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()),
        "accounts": accounts(&ctx).list(),
    }))
}

async fn threads(State(ctx): State<Ctx>) -> Json<Value> {
    Json(json!(store(&ctx).list()))
}

async fn thread(State(ctx): State<Ctx>, Path(id): Path<String>) -> Response {
    let Some(t) = store(&ctx).get(&id) else { return (StatusCode::NOT_FOUND, "no such chat").into_response() };
    let live = accounts(&ctx).engine_for(&t).ok().and_then(|e| e.live(&id));
    Json(json!({ "thread": t, "live": live })).into_response()
}

#[derive(Deserialize)]
struct ModelsQuery {
    account: Option<String>,
}

async fn models(State(ctx): State<Ctx>, Query(q): Query<ModelsQuery>) -> Response {
    let account = q.account.unwrap_or_else(|| DEFAULT.into());
    match accounts(&ctx).engine(&account) {
        Ok(e) => match e.models().await {
            Ok(m) => Json(m).into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}

async fn account_status(State(ctx): State<Ctx>, Path(id): Path<String>) -> Response {
    let accounts = accounts(&ctx);
    match accounts.engine(&id) {
        Ok(e) => match e.account().await {
            Ok(status) => {
                let a = &status["account"];
                accounts.remember(&id, a["email"].as_str().map(Into::into), a["planType"].as_str().map(Into::into));
                Json(status).into_response()
            }
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
struct UsageQuery {
    days: Option<i64>,
}

async fn usage(State(ctx): State<Ctx>, Query(q): Query<UsageQuery>) -> Response {
    let ledger = ctx.remote.app.state::<Arc<crate::usage::UsageLedger>>().inner().clone();
    let since = crate::usage::utc_hour_days_ago(q.days.unwrap_or(30).clamp(1, 365) + 1);
    match tokio::task::spawn_blocking(move || ledger.report(&since)).await {
        Ok(rows) => Json(json!(rows)).into_response(),
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
struct NotifyBody {
    title: String,
    body: String,
}

async fn notify(State(ctx): State<Ctx>, Json(b): Json<NotifyBody>) -> Json<Value> {
    Json(json!({ "phones": ctx.remote.notify(&b.title, &b.body) }))
}

#[derive(Deserialize)]
struct PricingBody {
    models: Vec<String>,
}

async fn pricing(State(ctx): State<Ctx>, Json(b): Json<PricingBody>) -> Response {
    let prices = ctx.remote.app.state::<Arc<crate::pricing::Pricing>>().inner().clone();
    match tokio::task::spawn_blocking(move || prices.get("openai", &b.models)).await {
        Ok(p) => Json(json!(p)).into_response(),
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendBody {
    thread_id: Option<String>,
    text: String,
    model: Option<String>,
    effort: Option<String>,
    access: Option<String>,
    cwd: Option<String>,
    account: Option<String>,
}

/// Existing chats keep their own model/access/account unless overridden.
async fn send(State(ctx): State<Ctx>, Json(b): Json<SendBody>) -> Response {
    let accounts = accounts(&ctx);
    let existing = b.thread_id.as_deref().and_then(|id| store(&ctx).get(id));
    let engine = match &existing {
        Some(t) => accounts.engine_for(t),
        None => accounts.engine(b.account.as_deref().unwrap_or(DEFAULT)),
    };
    let engine = match engine {
        Ok(e) => e,
        Err(e) => return err(e),
    };
    let model = match b.model.or_else(|| existing.as_ref().map(|t| t.model.clone())) {
        Some(m) => m,
        None => match engine.models().await {
            Ok(list) => list
                .as_array()
                .and_then(|a| a.iter().find(|m| m["isDefault"] == true).or(a.first()))
                .and_then(|m| m["id"].as_str())
                .unwrap_or_default()
                .to_owned(),
            Err(e) => return err(e),
        },
    };
    let effort = b.effort.or_else(|| existing.as_ref().and_then(|t| t.effort.clone()));
    let access = b.access.or_else(|| existing.as_ref().map(|t| t.access.clone())).unwrap_or_else(|| "auto".into());
    let thread_id = b.thread_id.or_else(|| Some(uuid::Uuid::new_v4().to_string()));
    match engine.send(thread_id, b.text, model, effort, access, b.cwd, None).await {
        Ok(t) => Json(json!(t)).into_response(),
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopBody {
    thread_id: String,
}

async fn stop(State(ctx): State<Ctx>, Json(b): Json<StopBody>) -> Response {
    let Some(t) = store(&ctx).get(&b.thread_id) else { return err("no such chat") };
    match accounts(&ctx).engine_for(&t) {
        Ok(e) => match e.stop(&b.thread_id).await {
            Ok(()) => Json(json!({ "ok": true })).into_response(),
            Err(e) => err(e),
        },
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
struct ApprovalBody {
    key: String,
    decision: String,
}

async fn approval(State(ctx): State<Ctx>, Json(b): Json<ApprovalBody>) -> Response {
    if !["accept", "acceptForSession", "decline", "cancel"].contains(&b.decision.as_str()) {
        return err("bad decision");
    }
    match accounts(&ctx).running().into_iter().find(|e| e.has_approval(&b.key)) {
        Some(e) => match e.respond(&b.key, &b.decision).await {
            Ok(()) => Json(json!({ "ok": true })).into_response(),
            Err(e) => err(e),
        },
        None => Json(json!({ "ok": true })).into_response(),
    }
}

async fn events(State(ctx): State<Ctx>, ws: WebSocketUpgrade) -> Response {
    let rx = ctx.remote.events.subscribe();
    ws.on_upgrade(move |socket| pump(socket, rx))
}

async fn pump(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(text) => if socket.send(Message::Text(text.into())).await.is_err() { break },
                // Phone fell behind: tell it to refetch instead of silently missing deltas.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if socket.send(Message::Text(r#"{"type":"resync"}"#.into())).await.is_err() { break }
                }
                Err(_) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}

// ---------- projects, git changes and chat titles (mirrors the desktop commands) ----------

async fn projects() -> Response {
    match tokio::task::spawn_blocking(crate::projects::find).await {
        Ok(p) => Json(json!(p)).into_response(),
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

async fn project_icon(Query(q): Query<PathQuery>) -> Response {
    match tokio::task::spawn_blocking(move || crate::projects::icon(&q.path)).await {
        Ok(i) => Json(json!(i)).into_response(),
        Err(e) => err(e),
    }
}

async fn git_status(Query(q): Query<PathQuery>) -> Response {
    match tokio::task::spawn_blocking(move || crate::git::status(&q.path)).await {
        Ok(s) => Json(json!(s)).into_response(),
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
struct DiffQuery {
    root: String,
    path: String,
    old: Option<String>,
    #[serde(default)]
    untracked: bool,
}

/// Paths are relative to the repo root; refuse anything that could step outside it.
fn inside_repo(p: &str) -> bool {
    let p = std::path::Path::new(p);
    !p.is_absolute() && p.components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir))
}

async fn git_diff(Query(q): Query<DiffQuery>) -> Response {
    if !inside_repo(&q.path) || q.old.as_deref().is_some_and(|o| !inside_repo(o)) {
        return err("bad path");
    }
    match tokio::task::spawn_blocking(move || crate::git::file_diff(&q.root, &q.path, q.old.as_deref(), q.untracked)).await {
        Ok(d) => Json(json!({ "diff": d })).into_response(),
        Err(e) => err(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TitleBody {
    thread_id: String,
    model: String,
    effort: Option<String>,
}

/// Have `model` name a chat from its first message; phones also get a `thread-title` event.
async fn title(State(ctx): State<Ctx>, Json(b): Json<TitleBody>) -> Response {
    let Some(t) = store(&ctx).get(&b.thread_id) else { return err("no such chat") };
    let Some(first) = t.messages.iter().find(|m| m.role == "user").map(|m| m.text.clone()) else { return err("Chat has no message yet") };
    let engine = match accounts(&ctx).engine_for(&t) {
        Ok(e) => e,
        Err(e) => return err(e),
    };
    match engine.generate_title(b.thread_id, first, b.model, b.effort).await {
        Ok(title) => Json(json!({ "title": title })).into_response(),
        Err(e) => err(e),
    }
}

// ---------- terminal (shared with the desktop panel) ----------

fn terms(ctx: &Ctx) -> Arc<crate::terminal::Terminals> {
    ctx.remote.app.state::<Arc<crate::terminal::Terminals>>().inner().clone()
}

async fn terminals(State(ctx): State<Ctx>) -> Json<Value> {
    Json(json!(terms(&ctx).list()))
}

#[derive(Deserialize)]
struct OpenBody {
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

async fn terminal_open(State(ctx): State<Ctx>, Json(b): Json<OpenBody>) -> Response {
    match terms(&ctx).open(b.cwd, b.cols.unwrap_or(48), b.rows.unwrap_or(30)) {
        Ok(info) => Json(json!(info)).into_response(),
        Err(e) => err(e),
    }
}

async fn terminal_close(State(ctx): State<Ctx>, Path(id): Path<String>) -> Json<Value> {
    terms(&ctx).close(&id);
    Json(json!({ "ok": true }))
}

/// The phone's terminal view: xterm.js in a WebView, talking to `/ws` below.
async fn terminal_page() -> Response {
    static PAGE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let page = PAGE.get_or_init(|| {
        include_str!("assets/terminal.html")
            .replace("/*XTERM_CSS*/", include_str!("../../node_modules/@xterm/xterm/css/xterm.css"))
            .replace("/*XTERM_JS*/", include_str!("../../node_modules/@xterm/xterm/lib/xterm.js"))
            .replace("/*FIT_JS*/", include_str!("../../node_modules/@xterm/addon-fit/lib/addon-fit.js"))
    });
    ([(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"), (axum::http::header::CACHE_CONTROL, "no-store")], page.clone()).into_response()
}

async fn terminal_ws(State(ctx): State<Ctx>, Path(id): Path<String>, ws: WebSocketUpgrade) -> Response {
    let terms = terms(&ctx);
    ws.on_upgrade(move |socket| terminal_pump(socket, terms, id))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum TermIn {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

async fn terminal_pump(mut socket: WebSocket, terms: Arc<crate::terminal::Terminals>, id: String) {
    use crate::terminal::Out;
    let Ok((scrollback, alive, mut rx)) = terms.attach(&id) else {
        let _ = socket.send(Message::Text(json!({ "type": "gone" }).to_string().into())).await;
        return;
    };
    let size = terms.size(&id).unwrap_or((80, 24));
    if socket.send(Message::Text(json!({ "type": "init", "data": scrollback, "alive": alive, "cols": size.0, "rows": size.1 }).to_string().into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Ok(Out::Data(d)) => if socket.send(Message::Text(json!({ "type": "data", "data": d }).to_string().into())).await.is_err() { break },
                Ok(Out::Size(cols, rows)) => if socket.send(Message::Text(json!({ "type": "size", "cols": cols, "rows": rows }).to_string().into())).await.is_err() { break },
                Ok(Out::Exit) => { let _ = socket.send(Message::Text(json!({ "type": "exit" }).to_string().into())).await; }
                // Fell behind: start over from the scrollback rather than show a hole.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let Ok((sb, alive, fresh)) = terms.attach(&id) else { break };
                    rx = fresh;
                    if socket.send(Message::Text(json!({ "type": "init", "data": sb, "alive": alive }).to_string().into())).await.is_err() { break }
                }
                Err(_) => { let _ = socket.send(Message::Text(json!({ "type": "gone" }).to_string().into())).await; break }
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Text(t))) => match serde_json::from_str::<TermIn>(&t) {
                    Ok(TermIn::Input { data }) => { let _ = terms.write(&id, &data); }
                    Ok(TermIn::Resize { cols, rows }) => { let _ = terms.resize(&id, cols, rows); }
                    Err(_) => {}
                },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}
