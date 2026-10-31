//! Shell sessions in a pseudo-terminal, shared by the desktop terminal panel and paired phones.
//!
//! Each session has one blocking reader thread (no polling: it sleeps in `read` until the shell
//! prints), a capped scrollback so a phone that connects later sees recent output, and a
//! broadcast channel for live output. Desktop windows get `term-output` events; phones get
//! the same bytes over a WebSocket (see remote.rs).

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// Recent output kept per session for late joiners (characters).
const SCROLLBACK: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub enum Out {
    Data(String),
    /// The PTY was resized (by any client): others re-assert their own size before typing.
    Size(u16, u16),
    Exit,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub id: String,
    pub cwd: String,
    pub title: String,
    pub alive: bool,
    pub created_at: u64,
}

struct Session {
    info: Mutex<Info>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    scrollback: Mutex<String>,
    alive: AtomicBool,
    size: Mutex<(u16, u16)>,
    tx: broadcast::Sender<Out>,
}

pub struct Terminals {
    app: AppHandle,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// Startup files that add command markers to zsh (None if they couldn't be written).
    zdotdir: Option<std::path::PathBuf>,
}

/// Write the zsh integration files into the app's data folder.
fn install_zsh(data: &std::path::Path) -> Option<std::path::PathBuf> {
    let dir = data.join("shell").join("zsh");
    std::fs::create_dir_all(&dir).ok()?;
    for (name, body) in [
        (".zshenv", include_str!("assets/zsh/zshenv")),
        (".zprofile", include_str!("assets/zsh/zprofile")),
        (".zshrc", include_str!("assets/zsh/zshrc")),
    ] {
        std::fs::write(dir.join(name), body).ok()?;
    }
    Some(dir)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Bytes → text without splitting a UTF-8 character across two reads.
fn decode(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    match std::str::from_utf8(pending) {
        Ok(s) => {
            let s = s.to_owned();
            pending.clear();
            s
        }
        Err(e) if e.error_len().is_none() => {
            // Incomplete character at the end: keep it for the next read.
            let valid = e.valid_up_to();
            let s = String::from_utf8_lossy(&pending[..valid]).into_owned();
            pending.drain(..valid);
            s
        }
        Err(_) => {
            let s = String::from_utf8_lossy(pending).into_owned();
            pending.clear();
            s
        }
    }
}

impl Terminals {
    pub fn new(app: AppHandle, data: &std::path::Path) -> Arc<Self> {
        Arc::new(Self { app, sessions: Mutex::new(HashMap::new()), zdotdir: install_zsh(data) })
    }

    pub fn list(&self) -> Vec<Info> {
        let mut out: Vec<Info> = self.sessions.lock().unwrap().values().map(|s| s.info.lock().unwrap().clone()).collect();
        out.sort_by_key(|i| i.created_at);
        out
    }

    fn changed(&self) {
        let _ = self.app.emit("terminals", json!(self.list()));
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        self.sessions.lock().unwrap().get(id).cloned().ok_or_else(|| "No such terminal".into())
    }

    /// Start the user's login shell in `cwd` (home when missing or not a folder).
    pub fn open(self: &Arc<Self>, cwd: Option<String>, cols: u16, rows: u16) -> Result<Info, String> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        let cwd = cwd.filter(|c| std::path::Path::new(c).is_dir()).unwrap_or(home);
        let size = PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 };
        let pair = native_pty_system().openpty(size).map_err(|e| format!("Couldn't open a terminal: {e}"))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut cmd = CommandBuilder::new(&shell);
        // Login shell: GUI apps start with a bare PATH, the user's profile fixes it.
        cmd.arg("-l");
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "hrns");
        if std::env::var_os("LANG").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        // zsh gets command markers through our startup files, which load the user's own first.
        if let (Some(dir), true) = (&self.zdotdir, shell.ends_with("zsh")) {
            if let Some(user) = std::env::var_os("ZDOTDIR") {
                cmd.env("HRNS_USER_ZDOTDIR", user);
            }
            cmd.env("ZDOTDIR", dir);
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| format!("Couldn't start {shell}: {e}"))?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        let title = std::path::Path::new(&cwd).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "~".into());
        let info = Info { id: id.clone(), cwd, title, alive: true, created_at: now_ms() };
        let (tx, _) = broadcast::channel(1024);
        let session = Arc::new(Session {
            info: Mutex::new(info.clone()),
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            scrollback: Mutex::new(String::new()),
            alive: AtomicBool::new(true),
            size: Mutex::new((size.cols, size.rows)),
            tx,
        });
        self.sessions.lock().unwrap().insert(id.clone(), session.clone());

        let me = self.clone();
        std::thread::Builder::new()
            .name(format!("term-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                let mut pending = Vec::new();
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let text = decode(&mut pending, &buf[..n]);
                    if text.is_empty() {
                        continue;
                    }
                    let _ = me.app.emit("term-output", json!({ "id": id, "data": text }));
                    // Scrollback and live send under one lock, so an attaching phone sees each byte exactly once.
                    let mut sb = session.scrollback.lock().unwrap();
                    sb.push_str(&text);
                    if sb.len() > SCROLLBACK {
                        let mut cut = sb.len() - SCROLLBACK;
                        while !sb.is_char_boundary(cut) {
                            cut += 1;
                        }
                        sb.drain(..cut);
                    }
                    let _ = session.tx.send(Out::Data(text));
                }
                session.alive.store(false, Ordering::SeqCst);
                let _ = session.child.lock().unwrap().wait();
                session.info.lock().unwrap().alive = false;
                let _ = me.app.emit("term-exit", json!({ "id": id }));
                let _ = session.tx.send(Out::Exit);
                me.changed();
            })
            .map_err(|e| e.to_string())?;

        self.changed();
        Ok(info)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let s = self.get(id)?;
        let mut w = s.writer.lock().unwrap();
        w.write_all(data.as_bytes()).and_then(|_| w.flush()).map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let s = self.get(id)?;
        let size = PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 };
        {
            let mut cur = s.size.lock().unwrap();
            if *cur == (size.cols, size.rows) {
                return Ok(());
            }
            *cur = (size.cols, size.rows);
        }
        let r = s.master.lock().unwrap().resize(size).map_err(|e| e.to_string());
        let _ = self.app.emit("term-size", json!({ "id": id, "cols": size.cols, "rows": size.rows }));
        let _ = s.tx.send(Out::Size(size.cols, size.rows));
        r
    }

    pub fn size(&self, id: &str) -> Option<(u16, u16)> {
        self.get(id).ok().map(|s| *s.size.lock().unwrap())
    }

    /// Kill the shell (if still running) and forget the session.
    pub fn close(&self, id: &str) {
        let s = self.sessions.lock().unwrap().remove(id);
        if let Some(s) = s {
            if s.alive.load(Ordering::SeqCst) {
                let _ = s.child.lock().unwrap().kill();
            }
        }
        self.changed();
    }

    /// Recent output plus a receiver for what comes next (for a phone attaching to a session).
    pub fn attach(&self, id: &str) -> Result<(String, bool, broadcast::Receiver<Out>), String> {
        let s = self.get(id)?;
        // Under the scrollback lock: output lands either in the copy or on the channel, never both.
        let guard = s.scrollback.lock().unwrap();
        let rx = s.tx.subscribe();
        let sb = guard.clone();
        drop(guard);
        Ok((sb, s.alive.load(Ordering::SeqCst), rx))
    }

    /// Recent output, for a desktop panel reopening a session.
    pub fn scrollback(&self, id: &str) -> Result<String, String> {
        Ok(self.get(id)?.scrollback.lock().unwrap().clone())
    }

    pub fn close_all(&self) {
        let ids: Vec<String> = self.sessions.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.close(&id);
        }
    }
}
