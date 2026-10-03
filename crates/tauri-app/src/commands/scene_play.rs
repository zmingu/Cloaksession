//! Scene play commands. Thin IPC adapters over
//! `TauriBrowserDriver::scene_*` / `play_scene` / `stop_scene`; SQLite and
//! playback scheduling stay in `driver::scene_play`.

use std::sync::Arc;

use profile_manager::scenes::{Scene, SceneLine, SceneLineAction, TriggerMode};
use tauri::State;

use crate::driver::scene_play::{PlaySceneOptions, PlayStarted};
use crate::AppState;

fn scene_error(message: impl std::fmt::Display) -> String {
    format!("场景操作失败：{message}。请核对场景与台词后重试；存储或线程错误请重启应用后重试。")
}

#[tauri::command]
pub async fn scene_create(
    state: State<'_, AppState>,
    name: String,
    trigger_mode: Option<TriggerMode>,
    group_id: Option<String>,
) -> Result<Scene, String> {
    state
        .driver
        .scene_create(name, trigger_mode.unwrap_or(TriggerMode::RelativeTime), group_id)
        .await
        .map_err(scene_error)
}

#[tauri::command]
pub async fn scene_get(state: State<'_, AppState>, id: i64) -> Result<Option<Scene>, String> {
    state.driver.scene_get(id).await.map_err(scene_error)
}

#[tauri::command]
pub async fn scene_list(state: State<'_, AppState>) -> Result<Vec<Scene>, String> {
    state.driver.scene_list().await.map_err(scene_error)
}

#[tauri::command]
pub async fn scene_update(
    state: State<'_, AppState>,
    id: i64,
    name: Option<String>,
    trigger_mode: Option<TriggerMode>,
    group_id: Option<Option<String>>,
) -> Result<Scene, String> {
    state
        .driver
        .scene_update(id, name, trigger_mode, group_id)
        .await
        .map_err(scene_error)
}

#[tauri::command]
pub async fn scene_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.driver.scene_delete(id).await.map_err(scene_error)
}

#[tauri::command]
pub async fn scene_add_line(
    state: State<'_, AppState>,
    scene_id: i64,
    message: String,
    time_offset_sec: i64,
    action_type: Option<SceneLineAction>,
) -> Result<SceneLine, String> {
    state
        .driver
        .scene_add_line(
            scene_id,
            message,
            time_offset_sec,
            action_type.unwrap_or(SceneLineAction::Danmaku),
        )
        .await
        .map_err(scene_error)
}

#[tauri::command]
pub async fn scene_update_line(
    state: State<'_, AppState>,
    id: i64,
    message: Option<String>,
    time_offset_sec: Option<i64>,
    action_type: Option<SceneLineAction>,
) -> Result<SceneLine, String> {
    state
        .driver
        .scene_update_line(id, message, time_offset_sec, action_type)
        .await
        .map_err(scene_error)
}

#[tauri::command]
pub async fn scene_delete_line(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    state.driver.scene_delete_line(id).await.map_err(scene_error)
}

#[tauri::command]
pub async fn scene_reorder_lines(
    state: State<'_, AppState>,
    scene_id: i64,
    line_ids: Vec<i64>,
) -> Result<Vec<SceneLine>, String> {
    state
        .driver
        .scene_reorder_lines(scene_id, line_ids)
        .await
        .map_err(scene_error)
}

#[tauri::command]
pub async fn scene_play(
    state: State<'_, AppState>,
    scene_id: i64,
    options: Option<PlaySceneOptions>,
) -> Result<PlayStarted, String> {
    // `play_scene` needs `&Arc<Self>` (tasks hold an owned clone).
    let driver: &Arc<crate::TauriBrowserDriver> = &state.driver;
    driver
        .play_scene(scene_id, options.unwrap_or_default())
        .await
        .map_err(scene_error)
}

#[tauri::command]
pub async fn scene_stop(state: State<'_, AppState>, scene_id: i64) -> Result<bool, String> {
    state.driver.stop_scene(scene_id).await.map_err(scene_error)
}
