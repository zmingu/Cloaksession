//! huibo-live IPC 薄适配层 (jieger `huiboLiveHandler.ts` 对应).
//!
//! - `get_huibo_video_list` ← `CHANNELS.tasks.huiboLive.getVideoList`
//! - `start_huibo_live` ← `CHANNELS.tasks.huiboLive.startLive`
//! - `get_shop_live_state` — 开播后复核入口 (jieger 由 `startHuiboLive`
//!   内部调用 `getShopLiveState`; 此处同时暴露为独立命令便于 UI 单独刷新)。
//! - `cancel_huibo_task` — 协同取消进行中的跟播任务。
use crate::driver::huibo_live::{HuiboVideo, ShopLiveState};
use crate::AppState;
use multizen_core::MultizenError;
use tauri::State;

fn huibo_error(error: MultizenError) -> String {
    format!("跟播/回播操作失败：{error}。请确认浏览器已启动且小店已登录，刷新后重试；仍失败请重启应用。")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_errors_retain_cause_and_actionable_chinese_context() {
        let message = huibo_error(MultizenError::Cdp("huibo-live: timed out".into()));
        assert!(message.contains("huibo-live"));
        assert!(message.contains("跟播/回播操作失败"));
        assert!(message.contains("重启应用"));
    }
}

#[tauri::command]
pub async fn get_huibo_video_list(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<HuiboVideo>, String> {
    state
        .driver
        .huibo_video_list(&profile_id)
        .await
        .map_err(huibo_error)
}

#[tauri::command]
pub async fn start_huibo_live(
    state: State<'_, AppState>,
    profile_id: String,
    replay_id: String,
) -> Result<ShopLiveState, String> {
    state
        .driver
        .huibo_start_live(&profile_id, &replay_id)
        .await
        .map_err(huibo_error)
}

#[tauri::command]
pub async fn get_shop_live_state(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ShopLiveState, String> {
    state
        .driver
        .huibo_shop_live_state(&profile_id)
        .await
        .map_err(huibo_error)
}

#[tauri::command]
pub async fn cancel_huibo_task(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<bool, String> {
    Ok(state.driver.huibo_cancel(&profile_id))
}
