//! 开播控制 IPC：heartbeat 占位推流 / 本地视频循环推流 / 凭据与前置检查。
//!
//! 薄适配层：类型化结果在 driver 侧，IPC 边界统一 stringify（中文 actionable
//! 文案由 driver 侧产出）。事件 `live-launch-state-changed` 由 driver 侧 emit。
//!
//! # mate（伴侣）模式
//!
//! 伴侣账号开播走 `liveMate`（**不启浏览器**），命令以 `mate_account_id` 为键：
//! - `live_launch_mate_credentials`：取流（三步开播）→ [`StreamCredentials`]；
//! - `live_launch_mate_stream_start`：取流 + 起 ffmpeg 正式推流；
//! - `live_launch_mate_stream_stop`：先 `stopLiveMatePush` 关播，再停 ffmpeg；
//! - `live_launch_mate_heartbeat`：best-effort 心跳（无内置定时器，按需调用）。

use tauri::{AppHandle, Manager, State};

use crate::driver::live_launch::{
    prerequisites_report, PrerequisitesReport, StreamCredentials, StreamingState,
};
use crate::AppState;

#[tauri::command]
pub async fn live_launch_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<StreamingState, String> {
    Ok(state.driver.live_launch_status(&profile_id).await)
}

#[tauri::command]
pub async fn live_launch_prerequisites(app: AppHandle) -> Result<PrerequisitesReport, String> {
    let resource_dir = app.path().resource_dir().ok();
    Ok(prerequisites_report(resource_dir.as_deref()))
}

#[tauri::command]
pub async fn live_launch_credentials(
    state: State<'_, AppState>,
    profile_id: String,
    control_url: Option<String>,
) -> Result<StreamCredentials, String> {
    state
        .driver
        .fetch_stream_credentials(&profile_id, control_url.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn live_launch_heartbeat_start(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    control_url: Option<String>,
) -> Result<StreamingState, String> {
    state
        .driver
        .live_launch_start_heartbeat(&app, &profile_id, control_url.as_deref())
        .await
}

#[tauri::command]
pub async fn live_launch_heartbeat_stop(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
) -> Result<StreamingState, String> {
    state.driver.live_launch_stop(&app, &profile_id).await
}

#[tauri::command]
pub async fn live_launch_stream_start(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    video_path: String,
    control_url: Option<String>,
) -> Result<StreamingState, String> {
    state
        .driver
        .live_launch_start_stream(&app, &profile_id, &video_path, control_url.as_deref())
        .await
}

#[tauri::command]
pub async fn live_launch_stream_stop(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
) -> Result<StreamingState, String> {
    state.driver.live_launch_stop(&app, &profile_id).await
}

// --- mate（伴侣）模式 --------------------------------------------------------

/// mate 取流：读 `mate_accounts` → `liveMate` 三步开播 → [`StreamCredentials`]。
#[tauri::command]
pub async fn live_launch_mate_credentials(
    state: State<'_, AppState>,
    mate_account_id: String,
) -> Result<StreamCredentials, String> {
    state
        .driver
        .fetch_mate_stream_credentials(&mate_account_id)
        .await
        .map_err(|e| e.to_string())
}

/// mate 开播：取流 + 起 ffmpeg 正式推流（视频循环）。
#[tauri::command]
pub async fn live_launch_mate_stream_start(
    state: State<'_, AppState>,
    app: AppHandle,
    mate_account_id: String,
    video_path: String,
) -> Result<StreamingState, String> {
    state
        .driver
        .live_launch_start_mate_stream(&app, &mate_account_id, &video_path)
        .await
}

/// mate 停播：先 `stopLiveMatePush` 关播，再停 ffmpeg。
#[tauri::command]
pub async fn live_launch_mate_stream_stop(
    state: State<'_, AppState>,
    app: AppHandle,
    mate_account_id: String,
) -> Result<StreamingState, String> {
    state
        .driver
        .live_launch_stop_mate_stream(&app, &mate_account_id)
        .await
}

/// mate 心跳（best-effort）。jieger 无周期调用方，故由调用方按需触发。
#[tauri::command]
pub async fn live_launch_mate_heartbeat(
    state: State<'_, AppState>,
    mate_account_id: String,
) -> Result<(), String> {
    state
        .driver
        .live_launch_mate_heartbeat(&mate_account_id)
        .await
        .map_err(|e| e.to_string())
}
