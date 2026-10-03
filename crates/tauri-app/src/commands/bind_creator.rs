//! Thin IPC adapters for bindCreator; SQLite stays on the launcher thread,
//! browser work runs on TaskPage leases. Errors are actionable Chinese strings.
use crate::driver::{AuthorizeListResult, BindCreatorResult};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn bind_creator_get_authorize_list(
    state: State<'_, AppState>,
    jinniu_id: String,
) -> Result<AuthorizeListResult, String> {
    let data = state
        .driver
        .bind_creator_list(&jinniu_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(AuthorizeListResult::ok(data))
}

#[tauri::command]
pub async fn bind_creator_sync_authorize_list(
    state: State<'_, AppState>,
    profile_id: String,
    jinniu_id: String,
    account_id: Option<String>,
) -> Result<AuthorizeListResult, String> {
    state
        .driver
        .bind_creator_sync(&profile_id, &jinniu_id, account_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn bind_creator_start_authorize(
    state: State<'_, AppState>,
    profile_id: String,
    jinniu_id: String,
    kuaishou_id: String,
    skip_confirm: Option<bool>,
    account_id: Option<String>,
) -> Result<BindCreatorResult, String> {
    state
        .driver
        .bind_creator_authorize(
            &profile_id,
            &jinniu_id,
            &kuaishou_id,
            skip_confirm.unwrap_or(false),
            account_id.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}
