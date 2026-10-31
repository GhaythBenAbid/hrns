mod accounts;
mod codex;
mod pricing;
mod store;
mod usage;

use accounts::{AccountEntry, Accounts};
use pricing::{Price, Pricing};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use store::{Store, Summary, Thread};
use tauri::{Manager, State};
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
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_threads,
            get_thread,
            git_branch,
            rename_thread,
            delete_thread,
            send_message,
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
            logout
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
