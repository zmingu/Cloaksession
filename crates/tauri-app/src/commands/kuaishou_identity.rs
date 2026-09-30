use crate::AppState;
use multizen_core::KuaishouIdentitySnapshot;
use tauri::State;

#[tauri::command]
pub async fn kuaishou_identity_list(
    state: State<'_, AppState>,
) -> Result<Vec<KuaishouIdentitySnapshot>, String> {
    state
        .driver
        .kuaishou_identity_list()
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn kuaishou_identity_detect(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<KuaishouIdentitySnapshot, String> {
    Ok(state.driver.kuaishou_identity_detect(&profile_id).await)
}
#[tauri::command]
pub async fn kuaishou_identity_avatar(
    state: State<'_, AppState>,
    avatar_key: String,
) -> Result<Option<String>, String> {
    Ok(state.driver.kuaishou_identity_avatar(&avatar_key).await)
}
