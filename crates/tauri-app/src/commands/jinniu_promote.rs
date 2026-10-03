use crate::driver::jinniu_promote::{
    JinniuLiveUser, JinniuLiveUsers, StoreCreatePhase1Config, StoreCreateTab,
};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn jinniu_promote_open_store_create(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<StoreCreateTab, String> {
    state
        .driver
        .jinniu_open_store_create_tab(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_promote_live_users(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<JinniuLiveUsers, String> {
    state
        .driver
        .jinniu_live_users(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_promote_select_live_user(
    state: State<'_, AppState>,
    profile_id: String,
    uid: String,
) -> Result<JinniuLiveUser, String> {
    state
        .driver
        .jinniu_select_live_user(&profile_id, &uid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_promote_apply_phase1(
    state: State<'_, AppState>,
    profile_id: String,
    config: StoreCreatePhase1Config,
) -> Result<(), String> {
    state
        .driver
        .jinniu_apply_phase1(&profile_id, &config)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_promote_apply_phase2(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<String>, String> {
    state
        .driver
        .jinniu_apply_phase2(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_promote_submit(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    state
        .driver
        .jinniu_submit_store_create(&profile_id)
        .await
        .map_err(|e| e.to_string())
}
