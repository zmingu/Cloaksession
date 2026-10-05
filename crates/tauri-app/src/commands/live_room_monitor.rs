//! 直播间监控 IPC：启动 / 停止 / 查状态 / 列举全部槽。
//!
//! 薄适配层：参数透传 driver，错误 stringify（ actionable 中文由 driver 侧
//! 拼装）；`live-room-monitor-state-changed` 事件由 driver 侧 emit，
//! payload 含 `profileId`，前端据此分发到对应账号。
//!
//! 监控按 profile 分槽，支持多账号同时监控：`stop` / `get` 均需传
//! `profileId`，`list` 无参返回全部槽快照。

use tauri::{AppHandle, State};

use crate::driver::live_room_monitor::{LiveRoomMonitorState, MonitorConfig};
use crate::AppState;

#[tauri::command]
pub async fn start_live_room_monitor(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    config: MonitorConfig,
) -> Result<LiveRoomMonitorState, String> {
    state
        .driver
        .start_live_room_monitor(&app, &profile_id, config)
        .await
}

#[tauri::command]
pub async fn stop_live_room_monitor(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
) -> Result<LiveRoomMonitorState, String> {
    state.driver.stop_live_room_monitor(&app, &profile_id).await
}

#[tauri::command]
pub fn get_live_room_monitor_state(profile_id: String) -> LiveRoomMonitorState {
    // 静态运行时快照：阻塞读低频调用，可接受（同 driver::start 路径）。
    // 未知 profile → 默认快照（`profileId: null`，与“未启动”一致）。
    crate::driver::live_room_monitor::current_state_blocking(&profile_id)
}

#[tauri::command]
pub fn list_live_room_monitor_states() -> Vec<LiveRoomMonitorState> {
    // 同步列举全部槽快照，供前端一次拉全（无参）。
    crate::driver::live_room_monitor::list_states_blocking()
}
