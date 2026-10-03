//! Auto-message (主播互动时间轴弹幕) commands. Thin IPC adapters over
//! `TauriBrowserDriver::auto_message_*`: preserve typed `Result` internally,
//! stringify at the IPC boundary (see `commands/groups.rs`).

use tauri::State;

use crate::driver::auto_message::{
    AutoMessageOptions, AutoMessageStarted, AutoMessageState, MessageLine,
};
use crate::AppState;

/// Start a timeline run. `lines` fire at `started_at + offset_sec` against
/// each line's `account_id` session; `start_at` optionally pins the shared
/// epoch-ms origin (multi-script alignment), otherwise now is used.
#[tauri::command]
pub async fn auto_message_start(
    state: State<'_, AppState>,
    lines: Vec<MessageLine>,
    insert_random_space: Option<bool>,
    nickname: Option<String>,
    anchor: Option<String>,
    start_at: Option<u64>,
) -> Result<AutoMessageStarted, String> {
    state
        .driver
        .auto_message_start(
            lines,
            AutoMessageOptions {
                insert_random_space: insert_random_space.unwrap_or(false),
                nickname,
                anchor,
                start_at,
            },
        )
        .await
        .map_err(|e| e.to_string())
}

/// Stop a run; pending lines are suppressed via `TaskCancel`.
/// Idempotent — stopping an unknown/finished run is still `Ok`.
#[tauri::command]
pub async fn auto_message_stop(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<(), String> {
    state
        .driver
        .auto_message_stop(&run_id)
        .await
        .map_err(|e| e.to_string())
}

/// Snapshot of a running run. Finished/stopped runs are removed, so
/// querying them errors with `not running`.
#[tauri::command]
pub async fn auto_message_status(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<AutoMessageState, String> {
    state
        .driver
        .auto_message_status(&run_id)
        .await
        .map_err(|e| e.to_string())
}
