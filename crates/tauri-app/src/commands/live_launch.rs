//! 开播控制 IPC：heartbeat 占位推流 / 本地视频循环推流 / 凭据与前置检查。
//!
//! 薄适配层：类型化结果在 driver 侧，IPC 边界统一 stringify（中文 actionable
//! 文案由 driver 侧产出）。事件 `live-launch-state-changed` 由 driver 侧 emit。

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
