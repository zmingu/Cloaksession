//! Profile group commands. The pm itself lives on the dedicated launcher
//! thread (its rusqlite Connection is `!Send + !Sync`), so all group access
//! is routed through `TauriBrowserDriver`'s async helpers, which forward
//! `LauncherCmd` variants over the channel and await a oneshot reply.

use multizen_core::GroupInfo;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn profiles_list_groups(state: State<'_, AppState>) -> Result<Vec<GroupInfo>, String> {
    state.driver.list_groups().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profiles_set_profile_group(
    state: State<'_, AppState>,
    id: String,
    group: Option<String>,
) -> Result<(), String> {
    state
        .driver
        .set_profile_group(&id, group)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profiles_delete_group(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state
        .driver
        .delete_group(&name)
        .await
        .map_err(|e| e.to_string())
}