mod accounts;
mod codex;
mod git;
mod pricing;
mod projects;
mod remote;
mod store;
mod usage;

use accounts::{AccountEntry, Accounts};
use pricing::{Price, Pricing};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use store::{Store, Summary, Thread};
use remote::Remote;
use tauri::{Listener, Manager, State};
use usage::{Row, UsageLedger};

type Acc<'a> = State<'a, Arc<Accounts>>;

#[tauri::command]
fn list_threads(store: State<Arc<Store>>) -> Vec<Summary> {
    store.list()
}

#[tauri::command]
fn get_thread(store: State<Arc<Store>>, accounts: Acc, id: String) -> Option<(Thread, Option<Value>)> {
    let t = store.get(&id)?;
    let live = accounts.engine_for(&t).ok().and_then(|e| e.live(&id));
    Some((t, live))
}

/// Current git branch of a folder (reads .git/HEAD; no git process).
#[tauri::command]
fn git_branch(path: String) -> Option<String> {
    let mut dir = Some(std::path::Path::new(&path));
    while let Some(d) = dir {
        let git = d.join(".git");
        // Worktrees and submodules use a `.git` file pointing at the real git dir.
        let head = if git.is_file() {
            let pointer = std::fs::read_to_string(&git).ok()?;
            let gitdir = pointer.trim().strip_prefix("gitdir:")?.trim();
            std::fs::read_to_string(d.join(gitdir).join("HEAD")).ok()?
        } else if git.is_dir() {
            std::fs::read_to_string(git.join("HEAD")).ok()?
        } else {
            dir = d.parent();
            continue;
        };
        let head = head.trim();
        return Some(match head.strip_prefix("ref: refs/heads/") {
            Some(branch) => branch.to_owned(),
            None => head.chars().take(7).collect(), // detached HEAD
        });
    }
    None
}

/// Per-file counts of the uncommitted changes in the repo containing `path` (no diff text).
#[tauri::command]
async fn git_status(path: String) -> Result<Option<git::Status>, String> {
    tauri::async_runtime::spawn_blocking(move || git::status(&path)).await.map_err(|e| e.to_string())
}

/// One file's uncommitted diff (None for binary or oversized files).
#[tauri::command]
async fn git_file_diff(root: String, path: String, old: Option<String>, untracked: bool) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || git::file_diff(&root, &path, old.as_deref(), untracked))
        .await
        .map_err(|e| e.to_string())
}

/// Project folders under the usual code locations in the home folder, for the project finder.
#[tauri::command]
async fn find_projects() -> Result<Vec<projects::Project>, String> {
    tauri::async_runtime::spawn_blocking(projects::find).await.map_err(|e| e.to_string())
}

/// A project's own icon, or what it's built with (framework, else main language).
#[tauri::command]
async fn project_icon(path: String) -> Result<projects::Icon, String> {
    tauri::async_runtime::spawn_blocking(move || projects::icon(&path)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_thread(store: State<Arc<Store>>, id: String, title: String) {
    store.update(&id, |t| t.title = title);
}

#[tauri::command]
async fn delete_thread(store: State<'_, Arc<Store>>, accounts: Acc<'_>, id: String) -> Result<(), String> {
    if let Some(engine) = store.get(&id).and_then(|t| accounts.engine_for(&t).ok()) {
        let _ = engine.stop(&id).await;
        engine.forget(&id);
    }
    store.delete(&id);
    Ok(())
}

/// Existing chats stay on their own account; new chats use `account`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn send_message(
    store: State<'_, Arc<Store>>,
    accounts: Acc<'_>,
    thread_id: Option<String>,
    text: String,
    model: String,
    effort: Option<String>,
    access: String,
    cwd: Option<String>,
    account: String,
) -> Result<Thread, String> {
    let existing = thread_id.as_deref().and_then(|id| store.get(id));
    let engine = match &existing {
        Some(t) => accounts.engine_for(t)?,
        None => accounts.engine(&account)?,
    };
    engine.send(thread_id, text, model, effort, access, cwd).await
}

/// Have `model` name a chat from its first message (runs on the chat's own account).
#[tauri::command]
async fn generate_title(
    store: State<'_, Arc<Store>>,
    accounts: Acc<'_>,
    thread_id: String,
    model: String,
    effort: Option<String>,
) -> Result<String, String> {
    let thread = store.get(&thread_id).ok_or("No such chat")?;
    let first = thread.messages.iter().find(|m| m.role == "user").map(|m| m.text.clone()).ok_or("Chat has no message yet")?;
    let engine = accounts.engine_for(&thread)?;
    engine.generate_title(thread_id, first, model, effort).await
}

#[tauri::command]
async fn respond_approval(accounts: Acc<'_>, key: String, decision: String) -> Result<(), String> {
    match accounts.running().into_iter().find(|e| e.has_approval(&key)) {
        Some(engine) => engine.respond(&key, &decision).await,
        None => Ok(()),
    }
}

#[tauri::command]
async fn stop_turn(store: State<'_, Arc<Store>>, accounts: Acc<'_>, thread_id: String) -> Result<(), String> {
    let Some(t) = store.get(&thread_id) else { return Ok(()) };
    accounts.engine_for(&t)?.stop(&thread_id).await
}

/// API prices for the given OpenAI model ids (cached from models.dev).
#[tauri::command]
async fn pricing(prices: State<'_, Arc<Pricing>>, models: Vec<String>) -> Result<HashMap<String, Price>, String> {
    let prices = prices.inner().clone();
    tauri::async_runtime::spawn_blocking(move || prices.get("openai", &models)).await.map_err(|e| e.to_string())
}

/// Usage rows (UTC hour buckets) for the last `days` days, from Codex transcripts and hrns chats.
#[tauri::command]
async fn usage_report(ledger: State<'_, Arc<UsageLedger>>, days: i64) -> Result<Vec<Row>, String> {
    let ledger = ledger.inner().clone();
    let since = usage::utc_hour_days_ago(days + 1);
    tauri::async_runtime::spawn_blocking(move || ledger.report(&since)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_models(accounts: Acc<'_>, account: String) -> Result<Value, String> {
    accounts.engine(&account)?.models().await
}

#[tauri::command]
fn accounts(accounts: Acc) -> Vec<AccountEntry> {
    accounts.list()
}

/// Live identity + usage for one account (starts its sidecar).
#[tauri::command]
async fn account_status(accounts: Acc<'_>, account: String) -> Result<Value, String> {
    let status = accounts.engine(&account)?.account().await?;
    let a = &status["account"];
    accounts.remember(&account, a["email"].as_str().map(Into::into), a["planType"].as_str().map(Into::into));
    Ok(status)
}

#[tauri::command]
fn add_account(accounts: Acc) -> Result<String, String> {
    accounts.add()
}

#[tauri::command]
async fn remove_account(accounts: Acc<'_>, account: String) -> Result<(), String> {
    accounts.remove(&account).await
}

#[tauri::command]
async fn login(accounts: Acc<'_>, account: String) -> Result<String, String> {
    accounts.engine(&account)?.login().await
}

#[tauri::command]
async fn logout(accounts: Acc<'_>, account: String) -> Result<(), String> {
    accounts.engine(&account)?.logout().await?;
    accounts.remember(&account, None, None);
    Ok(())
}

#[tauri::command]
fn remote_info(remote: State<Arc<Remote>>) -> Value {
    remote.info()
}

#[tauri::command]
async fn remote_enable(remote: State<'_, Arc<Remote>>) -> Result<Value, String> {
    remote.inner().start().await
}

#[tauri::command]
fn remote_disable(remote: State<Arc<Remote>>) -> Value {
    remote.disable()
}

/// Returns how many phones are connected and received it.
#[tauri::command]
fn remote_test_notify(remote: State<Arc<Remote>>) -> usize {
    remote.notify("hrns", "Notifications are working. You'll hear from Codex here.")
}

#[tauri::command]
async fn remote_rotate(remote: State<'_, Arc<Remote>>) -> Result<Value, String> {
    remote.inner().rotate().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let store = Arc::new(Store::open(data.join("threads")));
            app.manage(Arc::new(Pricing::new(data.join("pricing.json"))));
            let ledger = Arc::new(UsageLedger::new(&data));
            app.manage(Accounts::open(app.handle().clone(), store.clone(), ledger.clone(), data.join("accounts")));
            app.manage(ledger);

            // Mirror chat events to paired phones.
            let remote = Remote::new(app.handle().clone(), &data);
            for kind in ["chat", "rate-limits"] {
                let r = remote.clone();
                app.listen_any(kind, move |e| r.publish(kind, e.payload()));
            }
            if remote.should_autostart() {
                let r = remote.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = r.start().await;
                });
            }
            app.manage(remote);
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_threads,
            get_thread,
            git_branch,
            git_status,
            git_file_diff,
            find_projects,
            project_icon,
            rename_thread,
            delete_thread,
            send_message,
            generate_title,
            stop_turn,
            respond_approval,
            list_models,
            pricing,
            usage_report,
            accounts,
            account_status,
            add_account,
            remove_account,
            login,
            logout,
            remote_info,
            remote_enable,
            remote_disable,
            remote_rotate,
            remote_test_notify
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
