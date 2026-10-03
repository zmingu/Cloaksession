//! Thin IPC adapters for viewer sub-accounts (小号); SQLite and browser
//! work stay in the driver. Errors are stringified at the IPC boundary.
use crate::AppState;
use multizen_core::{BusinessAccount, SaveBusinessAccountInput};
use profile_manager::SubAccountInteraction;
use tauri::State;

use crate::driver::SubAccountLoginResult;

#[tauri::command]
pub async fn save_sub_account(
    state: State<'_, AppState>,
    input: SaveBusinessAccountInput,
) -> Result<BusinessAccount, String> {
    state.driver.save_sub_account(input).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_sub_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<BusinessAccount>, String> {
    state.driver.list_sub_accounts().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unbind_sub_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.driver.unbind_sub_account(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sub_account_login(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<SubAccountLoginResult, String> {
    Ok(state.driver.login_account(&account_id).await)
}

#[tauri::command]
pub async fn batch_login_sub_accounts(
    state: State<'_, AppState>,
    account_ids: Vec<String>,
) -> Result<Vec<SubAccountLoginResult>, String> {
    Ok(state.driver.batch_login(account_ids).await)
}

#[tauri::command]
pub async fn sub_account_enter_live_room(
    state: State<'_, AppState>,
    account_id: String,
    live_url: String,
) -> Result<(), String> {
    state
        .driver
        .enter_live_room(&account_id, &live_url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sub_account_send_danmaku(
    state: State<'_, AppState>,
    account_id: String,
    content: String,
) -> Result<SubAccountInteraction, String> {
    state
        .driver
        .send_danmaku(&account_id, &content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sub_account_interactions(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<Vec<SubAccountInteraction>, String> {
    state
        .driver
        .sub_account_interactions(&account_id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn commands_are_registered_in_actual_tauri_handler() {
        let source = include_str!("../lib.rs");
        let handler = source
            .split(".invoke_handler(tauri::generate_handler![")
            .nth(1)
            .unwrap()
            .split("])")
            .next()
            .unwrap();
        for command in [
            "save_sub_account",
            "list_sub_accounts",
            "unbind_sub_account",
            "sub_account_login",
            "batch_login_sub_accounts",
            "sub_account_enter_live_room",
            "sub_account_send_danmaku",
            "sub_account_interactions",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
        let adapter = include_str!("sub_account.rs");
        assert_eq!(adapter.matches("\n#[tauri::command]").count(), 8);
    }
}
