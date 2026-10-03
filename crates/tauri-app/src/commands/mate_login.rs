//! 直播伴侣 QR login IPC adapters; HTTP flow and state stay in the driver.
use crate::driver::MateLoginState;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn mate_login_start(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<MateLoginState, String> {
    state
        .driver
        .mate_login_start(&account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mate_login_cancel(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<MateLoginState, String> {
    state
        .driver
        .mate_login_cancel(&account_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mate_login_state(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<MateLoginState, String> {
    state
        .driver
        .mate_login_state(&account_id)
        .await
        .map_err(|e| e.to_string())
}
