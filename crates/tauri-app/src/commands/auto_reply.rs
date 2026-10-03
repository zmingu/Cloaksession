//! jieger auto-reply Tauri commands: keyword-hit preview plus
//! `auto_reply_records` history/manual record (SQLite via `ProfileManager`).
//!
//! The live consumer (`driver::auto_reply::run_auto_reply_consumer`, wired
//! when comment-listener lands) resolves and records through its own seams;
//! these commands serve UI preview/testing and history inspection. Each DB
//! command opens a short-lived `ProfileManager` (WAL, idempotent
//! migrations) on a blocking thread — `ProfileManager` is `!Send` and lives
//! on the launcher thread, so it cannot be cached on `AppState`.

use crate::driver::auto_reply::{resolve_reply, GoodsKnowledge, ReplyResult};
use crate::AppState;
use profile_manager::AutoReplyRecord;
use tauri::State;

/// Pure keyword-hit preview: resolve `question` against caller-supplied
/// goods knowledge. No DB, no listener, no danmaku.
#[tauri::command]
pub async fn auto_reply_preview(question: String, goods: Vec<GoodsKnowledge>) -> ReplyResult {
    resolve_reply(&question, &goods, None)
}

/// Recent persisted resolutions, newest first. Empty `account_id` lists all
/// accounts. `limit` defaults to 50 (clamped to 1..=200 in storage).
#[tauri::command]
pub async fn auto_reply_history(
    state: State<'_, AppState>,
    account_id: String,
    limit: Option<u64>,
) -> Result<Vec<AutoReplyRecord>, String> {
    let db_path = state.db_path.clone();
    let profiles_root = state.profiles_root.clone();
    tokio::task::spawn_blocking(move || {
        let pm = profile_manager::ProfileManager::new(&db_path, &profiles_root)
            .map_err(|e| e.to_string())?;
        pm.auto_reply_list(&account_id, limit.unwrap_or(50))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Manual/test write to `auto_reply_records` (preview verification, seeding).
/// Live replies are recorded by the consumer's recorder, not here.
#[tauri::command]
pub async fn auto_reply_record(
    state: State<'_, AppState>,
    account_id: String,
    content: String,
    reply: String,
    source: String,
    goods_id: Option<String>,
) -> Result<AutoReplyRecord, String> {
    let db_path = state.db_path.clone();
    let profiles_root = state.profiles_root.clone();
    tokio::task::spawn_blocking(move || {
        let pm = profile_manager::ProfileManager::new(&db_path, &profiles_root)
            .map_err(|e| e.to_string())?;
        pm.auto_reply_record(
            &account_id,
            &content,
            &reply,
            &source,
            goods_id.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
