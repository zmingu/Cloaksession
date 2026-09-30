//! Thin IPC adapters; SQLite and validation stay on the launcher thread.
use crate::AppState;
use multizen_core::{
    BusinessAccount, BusinessProfileState, MultizenError, SaveBusinessAccountInput,
};
use tauri::State;

fn business_error(error: MultizenError) -> String {
    format!("业务账号操作失败：{error}。请核对输入及Profile状态、刷新列表后重试；数据库或线程错误请重启应用后重试。")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_errors_retain_cause_and_actionable_chinese_context() {
        let message = business_error(MultizenError::Mcp("launcher thread closed".into()));
        assert!(message.contains("launcher thread closed"));
        assert!(message.contains("业务账号操作失败"));
        assert!(message.contains("重启应用"));
    }
}

#[tauri::command]
pub async fn business_accounts_list(
    state: State<'_, AppState>,
) -> Result<Vec<BusinessAccount>, String> {
    state
        .driver
        .business_accounts_list()
        .await
        .map_err(business_error)
}

#[tauri::command]
pub async fn business_accounts_profile_state(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<BusinessProfileState, String> {
    state
        .driver
        .business_accounts_profile_state(&profile_id)
        .await
        .map_err(business_error)
}

#[tauri::command]
pub async fn business_accounts_save(
    state: State<'_, AppState>,
    input: SaveBusinessAccountInput,
) -> Result<BusinessAccount, String> {
    state
        .driver
        .business_accounts_save(input)
        .await
        .map_err(business_error)
}

#[tauri::command]
pub async fn business_accounts_unbind(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .driver
        .business_accounts_unbind(&id)
        .await
        .map_err(business_error)
}
