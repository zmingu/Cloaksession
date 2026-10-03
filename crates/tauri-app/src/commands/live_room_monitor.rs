//! 直播间监控 IPC：启动 / 停止 / 查状态。
//!
//! 薄适配层：参数透传 driver，错误 stringify（ actionable 中文由 driver 侧
//! 拼装）；`live-room-monitor-state-changed` 事件由 driver 侧 emit。

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
}

#[tauri::command]
pub async fn stop_live_room_monitor(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<LiveRoomMonitorState, String> {
    state.driver.stop_live_room_monitor(&app).await
}

#[tauri::command]
pub fn get_live_room_monitor_state() -> LiveRoomMonitorState {
    // 静态运行时快照：阻塞读低频调用，可接受（同 driver::start 路径）。
    crate::driver::live_room_monitor::current_state_blocking()
}
