//! 直播伴侣账号管理 + QR 登录 IPC 适配层。
//!
//! 薄适配：账号 CRUD 与 token 持久化在 `driver::mate_login`（SQLite 经 launcher
//! 线程）；HTTP 4 步登录状态机在 `driver::mate_login::MateLoginRuntime`。伴侣账号
//! **不绑浏览器环境**，存储表为 `mate_accounts`。
//!
//! `mate_account_add` 的 `label` **可选**：省略 / `null` / 空串（含纯空白）→ 存储层落
//! 占位别名（`profile_manager::MATE_PLACEHOLDER_LABEL`），待该账号**首次**扫码登录
//! 成功后由 `mate_account_record_login` 用平台昵称覆盖（仅首次生效，之后重登不覆盖）。
//!
//! 事件（由驱动层 emit，前端订阅）：
//! - `mate-login-state-changed`：`MateLoginState` 快照（**不含 token**）
use crate::driver::MateLoginState;
use crate::AppState;
use profile_manager::MateAccount;
use tauri::State;

// --- 账号 CRUD ---------------------------------------------------------------

#[tauri::command]
pub async fn mate_accounts_list(state: State<'_, AppState>) -> Result<Vec<MateAccount>, String> {
    state
        .driver
        .mate_accounts_list()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mate_account_add(
    state: State<'_, AppState>,
    label: Option<String>,
) -> Result<MateAccount, String> {
    let label = label.filter(|value| !value.trim().is_empty());
    state
        .driver
        .mate_account_add(label.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mate_account_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .driver
        .mate_account_remove(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mate_account_rename(
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<MateAccount, String> {
    state
        .driver
        .mate_account_rename(&id, &label)
        .await
        .map_err(|e| e.to_string())
}

// --- QR 登录状态机 -----------------------------------------------------------

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
            "mate_accounts_list",
            "mate_account_add",
            "mate_account_remove",
            "mate_account_rename",
            "mate_login_start",
            "mate_login_cancel",
            "mate_login_state",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
        let adapter = include_str!("mate_login.rs");
        assert_eq!(adapter.matches("\n#[tauri::command]").count(), 7);
    }
}
