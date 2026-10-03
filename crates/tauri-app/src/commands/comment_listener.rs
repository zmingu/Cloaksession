//! IPC adapters for the jieger live comment listener.
//!
//! Thin wrappers (see `commands/profiles.rs` for the pattern): typed
//! `Result` internally, stringified at the IPC boundary. Browser/DB work
//! stays in `crate::comment_listener` and `profile_manager`.

use std::path::PathBuf;
use std::sync::Arc;

use cdp_driver::session::BrowserSession;
use tauri::{AppHandle, State};

use crate::comment_listener::{
    global_listener, CommentEvent, ListenerStatus,
};
use crate::driver::TauriBrowserDriver;
use crate::AppState;

/// Resolve `<data_dir>/profiles.db` from the driver's stored profiles root
/// (`<data_dir>/profiles/`), mirroring `resolve_paths` in `lib.rs`.
fn db_path_for(driver: &TauriBrowserDriver) -> PathBuf {
    driver
        .profiles_root()
        .parent()
        .map(|p| p.join("profiles.db"))
        .unwrap_or_else(|| PathBuf::from("profiles.db"))
}

/// Pick the live-room page: first attached page whose URL looks like a live
/// room, else the first attached page. Never opens or navigates anything.
async fn pick_live_target(session: &Arc<BrowserSession>) -> Result<String, String> {
    let pages = session
        .browser
        .pages()
        .await
        .map_err(|e| format!("列出页面失败: {e}"))?;
    if pages.is_empty() {
        return Err("当前没有可监听的页面".into());
    }
    for page in &pages {
        if let Ok(Some(url)) = page.url().await {
            let lower = url.to_lowercase();
            if lower.contains("live") {
                return Ok(page.target_id().as_ref().to_string());
            }
        }
    }
    Ok(pages[0].target_id().as_ref().to_string())
}

#[tauri::command]
pub async fn comment_listener_start(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    target_id: Option<String>,
) -> Result<ListenerStatus, String> {
    let slot = state
        .driver
        .registry()
        .slot(&profile_id)
        .await
        .ok_or_else(|| "环境未运行或会话未就绪，请先启动对应环境".to_string())?;
    let session = slot
        .session()
        .ok_or_else(|| "会话已关闭，请重新启动环境".to_string())?;
    let target = match target_id.filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => pick_live_target(&session).await?,
    };
    let db_path = db_path_for(&state.driver);
    global_listener().start(&profile_id, session, target, db_path, Some(app))
}

#[tauri::command]
pub async fn comment_listener_stop(profile_id: String) -> Result<bool, String> {
    Ok(global_listener().stop(&profile_id))
}

#[tauri::command]
pub async fn comment_listener_status(
    profile_id: String,
) -> Result<Option<ListenerStatus>, String> {
    Ok(global_listener().status(&profile_id))
}

#[tauri::command]
pub async fn comment_listener_status_all() -> Result<Vec<ListenerStatus>, String> {
    Ok(global_listener().status_all())
}

/// In-memory recent events (newest first), fed by the running listener.
#[tauri::command]
pub async fn comment_events_recent(
    profile_id: String,
    limit: Option<u32>,
) -> Result<Vec<CommentEvent>, String> {
    Ok(global_listener().recent(&profile_id, limit.unwrap_or(100).min(500) as usize))
}

/// Durable `live_events` rows (newest first) for one account.
#[tauri::command]
pub async fn comment_events_history(
    state: State<'_, AppState>,
    profile_id: String,
    limit: Option<u32>,
) -> Result<Vec<profile_manager::live_events::LiveEventRow>, String> {
    let db_path = db_path_for(&state.driver);
    profile_manager::live_events::recent_events(
        &db_path,
        &profile_id,
        limit.unwrap_or(100).min(500),
    )
    .map_err(|e| e.to_string())
}

/// Internal broadcast subscription for in-process auto-reply consumers.
/// Blocks until the next fresh event (or the timeout) and returns it.
#[tauri::command]
pub async fn comment_events_next(timeout_ms: Option<u64>) -> Result<Option<CommentEvent>, String> {
    let mut rx = global_listener().subscribe();
    let wait = timeout_ms.unwrap_or(5_000).min(30_000);
    match tokio::time::timeout(std::time::Duration::from_millis(wait), rx.recv()).await {
        Ok(Ok(event)) => Ok(Some(event)),
        Ok(Err(_)) => Ok(None),
        Err(_) => Ok(None),
    }
}
