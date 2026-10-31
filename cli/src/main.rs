//! hrns in the terminal. The same core as the desktop app (compiled from its sources), so chats,
//! accounts and usage are shared with the desktop and the phone.

#![allow(dead_code)]

#[path = "../../src-tauri/src/accounts.rs"]
mod accounts;
#[path = "../../src-tauri/src/codex.rs"]
mod codex;
#[path = "../../src-tauri/src/events.rs"]
mod events;
#[path = "../../src-tauri/src/pricing.rs"]
mod pricing;
#[path = "../../src-tauri/src/store.rs"]
mod store;
#[path = "../../src-tauri/src/usage.rs"]
mod usage;

mod app;
mod fuzzy;
mod md;
mod ui;

use accounts::{Accounts, DEFAULT};
use app::{App, Msg};
use futures_util::StreamExt;
use ratatui::crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, EventStream};
use ratatui::crossterm::execute;
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The desktop app's data folder (Tauri's app data dir for `dev.hrns.app`).
pub fn data_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/dev.hrns.app")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share")).join("dev.hrns.app")
    };
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Core events go into a channel the UI reads.
struct ChannelEvents(mpsc::UnboundedSender<(String, Value)>);
impl events::Sink for ChannelEvents {
    fn emit(&self, kind: &str, payload: Value) {
        let _ = self.0.send((kind.to_owned(), payload));
    }
}

struct Args {
    cmd: Option<String>,
    prompt: Vec<String>,
    model: Option<String>,
    access: Option<String>,
    account: Option<String>,
    dir: Option<String>,
    days: i64,
    cont: bool,
}

fn usage_text() -> String {
    format!(
        "hrns {VERSION} · the real Codex agent on your ChatGPT plan

Usage:
  hrns [prompt]            open a chat in this folder (and send the prompt)
  hrns resume              pick up an earlier chat
  hrns -c, --continue      continue the latest chat in this folder
  hrns exec <prompt>       run one turn without the UI, print the answer
  hrns chats               list chats
  hrns usage [--days N]    tokens and cost by model
  hrns login               sign in with ChatGPT (default account)

Options:
  -m, --model <id>         model for new chats
  -a, --access <mode>      read-only | auto | full   (default: auto)
      --account <email>    ChatGPT account for new chats
  -C, --cd <dir>           work in this folder
  -h, --help               this help
  -V, --version            version

Chats, accounts and usage are shared with the hrns desktop app and phone."
    )
}

fn parse() -> Result<Args, String> {
    let mut a = Args { cmd: None, prompt: vec![], model: None, access: None, account: None, dir: None, days: 7, cont: false };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "-h" | "--help" => {
                println!("{}", usage_text());
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("hrns {VERSION}");
                std::process::exit(0);
            }
            "-m" | "--model" => a.model = Some(it.next().ok_or("--model needs a value")?),
            "-a" | "--access" => {
                let v = it.next().ok_or("--access needs a value")?;
                if !["read-only", "auto", "full"].contains(&v.as_str()) {
                    return Err("--access is read-only, auto or full".into());
                }
                a.access = Some(v);
            }
            "--account" => a.account = Some(it.next().ok_or("--account needs a value")?),
            "-C" | "--cd" => a.dir = Some(it.next().ok_or("--cd needs a folder")?),
            "--days" => a.days = it.next().and_then(|d| d.parse().ok()).ok_or("--days needs a number")?,
            "-c" | "--continue" => a.cont = true,
            "resume" | "exec" | "chats" | "usage" | "login" if a.cmd.is_none() && a.prompt.is_empty() => a.cmd = Some(x),
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("Unknown option {s}. See hrns --help.")),
            _ => a.prompt.push(x),
        }
    }
    Ok(a)
}

#[tokio::main]
async fn main() {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hrns: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(args).await {
        eprintln!("hrns: {e}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), String> {
    let data = data_dir().ok_or("Couldn't find a home folder for hrns data")?;
    let cwd = match &args.dir {
        Some(d) => std::fs::canonicalize(d).map_err(|e| format!("{d}: {e}"))?,
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    }
    .to_string_lossy()
    .into_owned();

    let (core_tx, mut core_rx) = mpsc::unbounded_channel::<(String, Value)>();
    let store = Arc::new(store::Store::open(data.join("threads")));
    let ledger = Arc::new(usage::UsageLedger::new(&data));
    let pricing = Arc::new(pricing::Pricing::new(data.join("pricing.json")));
    let accounts = Accounts::open(Arc::new(ChannelEvents(core_tx)), store.clone(), ledger.clone(), data.join("accounts"));

    let account = match &args.account {
        None => DEFAULT.to_owned(),
        Some(want) => accounts
            .list()
            .into_iter()
            .find(|a| a.id == *want || a.email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(want) || e.split('@').next() == Some(want.as_str())))
            .map(|a| a.id)
            .ok_or_else(|| format!("No account “{want}”. Add accounts in the hrns desktop app."))?,
    };

    match args.cmd.as_deref() {
        Some("chats") => return list_chats(&store, &cwd),
        Some("usage") => return print_usage(&ledger, &pricing, args.days),
        Some("login") => return login(&accounts, &account).await,
        Some("exec") => return exec(&accounts, &store, &mut core_rx, &account, &cwd, &args).await,
        _ => {}
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let mut app = App::new(tx, accounts.clone(), store.clone(), ledger, pricing, cwd.clone(), account, args.model.clone(), args.access.clone().unwrap_or_else(|| "auto".into()));
    app.load_account();
    if args.cont {
        if let Some(s) = store.list().into_iter().find(|s| s.cwd.as_deref() == Some(cwd.as_str())) {
            app.open_thread(&s.id);
        }
    }
    if args.cmd.as_deref() == Some("resume") {
        app.open_resume();
    }
    let mut pending = (!args.prompt.is_empty()).then(|| args.prompt.join(" "));

    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste);
    let mut events = EventStream::new();

    let result: Result<(), String> = async {
        loop {
            // A prompt from the command line goes out once the models are known.
            if pending.is_some() && !app.c.models.is_empty() {
                app.send(pending.take().unwrap());
            }
            if app.toast.as_ref().is_some_and(|(_, t)| t.elapsed().as_secs() >= 4) {
                app.toast = None;
            }
            terminal.draw(|f| ui::draw(f, &mut app)).map_err(|e| e.to_string())?;
            // Redraw on a timer only while something animates; otherwise sleep until an event.
            let animating = app.any_busy() || app.toast.is_some();
            let tick = if animating { Duration::from_millis(100) } else { Duration::from_secs(3600) };
            tokio::select! {
                ev = events.next() => match ev {
                    Some(Ok(ev)) => app.on_event(ev),
                    Some(Err(e)) => return Err(e.to_string()),
                    None => break,
                },
                Some((kind, v)) = core_rx.recv() => {
                    app.on_core(&kind, v);
                    // Streaming sends many small events: take everything queued before redrawing.
                    while let Ok((k, v)) = core_rx.try_recv() {
                        app.on_core(&k, v);
                    }
                }
                Some(msg) = rx.recv() => app.on_msg(msg),
                _ = tokio::time::sleep(tick) => {}
            }
            if animating {
                app.tick = app.tick.wrapping_add(1);
            }
            if app.quit {
                break;
            }
        }
        Ok(())
    }
    .await;

    let _ = execute!(std::io::stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
    // Stop anything still running in either pane (what streamed so far is saved).
    if app.busy() {
        app.stop();
    }
    if app.other.as_ref().is_some_and(|o| o.busy()) {
        app.focus_other();
        app.stop();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    for e in accounts.running() {
        e.shutdown().await;
    }
    if let Some(t) = &app.c.thread {
        println!("\x1b[2mhrns · {} · resume with `hrns resume`\x1b[0m", t.title);
    }
    result
}

fn list_chats(store: &store::Store, cwd: &str) -> Result<(), String> {
    let list = store.list();
    if list.is_empty() {
        println!("No chats yet. Start one with `hrns`.");
    }
    for s in list.iter().take(40) {
        let here = s.cwd.as_deref() == Some(cwd);
        let folder = s.cwd.as_deref().map(|c| c.rsplit('/').next().unwrap_or(c)).unwrap_or("-");
        println!("{} {:<14} {:<48} \x1b[2m{:>4}  {}\x1b[0m", if here { "\x1b[35m●\x1b[0m" } else { " " }, folder.chars().take(14).collect::<String>(), s.title.chars().take(48).collect::<String>(), app::ago(s.updated_at), s.model);
    }
    Ok(())
}

fn print_usage(ledger: &usage::UsageLedger, pricing: &pricing::Pricing, days: i64) -> Result<(), String> {
    let rows = ledger.report(&usage::utc_hour_days_ago(days.clamp(1, 365) + 1));
    let mut by: std::collections::BTreeMap<String, (u64, u64, u64)> = Default::default();
    for r in rows {
        let e = by.entry(r.model).or_default();
        e.0 += r.input;
        e.1 += r.cached;
        e.2 += r.output;
    }
    let prices = pricing.get("openai", &by.keys().cloned().collect::<Vec<_>>());
    let mut total = 0.0;
    println!("\x1b[1m{:<24} {:>10} {:>10} {:>10}\x1b[0m", "model", "input", "output", "cost");
    for (m, (i, c, o)) in &by {
        let cost = prices.get(m).map(|p| ((i - c.min(i)) as f64 * p.input.unwrap_or(0.0) + *c as f64 * p.cached.or(p.input).unwrap_or(0.0) + *o as f64 * p.output.unwrap_or(0.0)) / 1e6).unwrap_or(0.0);
        total += cost;
        println!("{:<24} {:>10} {:>10} {:>10}", m, app::tok(*i), app::tok(*o), format!("${cost:.2}"));
    }
    println!("\x1b[2mlast {days} days · ${total:.2} at API prices (covered by your ChatGPT plan)\x1b[0m");
    Ok(())
}

async fn login(accounts: &Arc<Accounts>, account: &str) -> Result<(), String> {
    let engine = accounts.engine(account)?;
    let url = engine.login().await?;
    println!("Opening your browser to sign in with ChatGPT…\n{url}");
    let _ = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" }).arg(&url).status();
    println!("Finish signing in in the browser, then run `hrns`.");
    tokio::time::sleep(Duration::from_secs(120)).await;
    Ok(())
}

/// One turn without the UI: the answer streams to stdout, steps to stderr. Approvals are
/// declined (nobody is there to answer) unless you pass `--access full`.
async fn exec(accounts: &Arc<Accounts>, store: &Arc<store::Store>, rx: &mut mpsc::UnboundedReceiver<(String, Value)>, account: &str, cwd: &str, args: &Args) -> Result<(), String> {
    let prompt = args.prompt.join(" ");
    if prompt.trim().is_empty() {
        return Err("exec needs a prompt: hrns exec \"…\"".into());
    }
    let engine = accounts.engine(account)?;
    let model = match &args.model {
        Some(m) => m.clone(),
        None => engine.models().await?.as_array().and_then(|a| a.iter().find(|m| m["isDefault"] == true).or(a.first())).and_then(|m| m["id"].as_str()).ok_or("No models available")?.to_owned(),
    };
    let id = app::new_id();
    let access = args.access.clone().unwrap_or_else(|| "auto".into());
    engine.send(Some(id.clone()), prompt, model, None, access, Some(cwd.to_owned())).await?;
    let mut printed: std::collections::HashMap<String, usize> = Default::default();
    let mut failed = false;
    while let Some((kind, v)) = rx.recv().await {
        if kind != "chat" || v["threadId"].as_str() != Some(&id) {
            continue;
        }
        match v["kind"].as_str() {
            Some("delta") if v["field"] == "text" => {
                print!("{}", v["text"].as_str().unwrap_or_default());
                let _ = std::io::stdout().flush();
                *printed.entry(v["partId"].as_str().unwrap_or_default().to_owned()).or_default() += 1;
            }
            Some("part") => {
                let p = &v["part"];
                match p["type"].as_str() {
                    Some("command") if p["status"] != "inProgress" => eprintln!("\x1b[2m$ {}  {}\x1b[0m", p["command"].as_str().unwrap_or_default(), p["exitCode"]),
                    Some("fileChange") => {
                        for c in p["changes"].as_array().into_iter().flatten() {
                            eprintln!("\x1b[2m✎ {}\x1b[0m", c["path"].as_str().unwrap_or_default());
                        }
                    }
                    Some("text") if !printed.contains_key(p["id"].as_str().unwrap_or_default()) => {
                        if let Some(t) = p["text"].as_str().filter(|t| !t.is_empty()) {
                            print!("{t}");
                            printed.insert(p["id"].as_str().unwrap_or_default().to_owned(), 1);
                        }
                    }
                    _ => {}
                }
            }
            Some("approval") => {
                let a = &v["approval"];
                eprintln!("\x1b[33mdeclined: {}\x1b[0m (run with --access full, or use `hrns` to approve)", a["command"].as_str().unwrap_or("a change"));
                let key = a["key"].as_str().unwrap_or_default().to_owned();
                let _ = engine.respond(&key, "decline").await;
            }
            Some("done") => {
                if let Some(e) = v["message"]["error"].as_str() {
                    eprintln!("\x1b[31m{e}\x1b[0m");
                    failed = true;
                }
                break;
            }
            _ => {}
        }
    }
    println!();
    let _ = store;
    engine.shutdown().await;
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// Renders a split screen with two chats into a buffer and prints it.
    #[tokio::test]
    async fn split_screen_snapshot() {
        let dir = std::env::temp_dir().join(format!("hrns-cli-test-{}", std::process::id()));
        let (core_tx, _rx) = mpsc::unbounded_channel();
        let store = Arc::new(store::Store::open(dir.join("threads")));
        let ledger = Arc::new(usage::UsageLedger::new(&dir));
        let pricing = Arc::new(pricing::Pricing::new(dir.join("pricing.json")));
        let accounts = Accounts::open(Arc::new(ChannelEvents(core_tx)), store.clone(), ledger.clone(), dir.join("accounts"));
        let (tx, _r) = mpsc::unbounded_channel();
        let mut app = App::new(tx, accounts, store, ledger, pricing, "/Users/me/code/hrns".into(), DEFAULT.into(), None, "auto".into());
        let model = app::Model { id: "gpt-6.1-sol".into(), name: "GPT-6.1-Sol".into(), default: true, efforts: vec!["medium".into()], default_effort: Some("medium".into()) };
        app.c.models = vec![model.clone()];
        let msg = |role: &str, text: &str, parts: Option<Vec<Value>>| store::Message { id: app::new_id(), role: role.into(), text: text.into(), parts, model: None, error: None, created_at: 0 };
        let thread = |title: &str, msgs: Vec<store::Message>| store::Thread { id: app::new_id(), title: title.into(), model: "gpt-6.1-sol".into(), effort: None, access: "auto".into(), cwd: Some("/Users/me/code/hrns".into()), account: None, usage: Default::default(), created_at: 0, updated_at: 0, messages: msgs };
        app.c.thread = Some(thread("Add a dark mode toggle", vec![
            msg("user", "Add a dark mode toggle to settings", None),
            msg("assistant", "", Some(vec![
                serde_json::json!({"type":"command","id":"1","command":"rg theme","actions":[{"type":"search","query":"theme","command":"rg theme"}],"exitCode":0,"status":"completed"}),
                serde_json::json!({"type":"fileChange","id":"2","changes":[{"path":"src/Settings.svelte","kind":"update","diff":"@@\n+a\n+b\n-c"}],"status":"completed"}),
                serde_json::json!({"type":"text","id":"3","text":"Added a **Theme** setting: System, Light or Dark."}),
            ])),
        ]));
        app.split(None);
        app.c.models = vec![model];
        app.c.thread = Some(thread("Fix the failing tests", vec![msg("user", "Why do the store tests fail?", None)]));
        app.c.live = Some(app::Live { parts: vec![serde_json::json!({"type":"command","id":"x","command":"cargo test","status":"inProgress"})], approvals: vec![], notice: None });
        let mut term = ratatui::Terminal::new(TestBackend::new(120, 22)).unwrap();
        term.draw(|f| ui::draw(f, &mut app)).unwrap();
        let buf = term.backend().buffer().clone();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect();
            println!("{line}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
