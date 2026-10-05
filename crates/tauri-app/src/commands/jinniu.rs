//! 磁力金牛大户管理 IPC 适配层。
//!
//! 薄适配：状态机、URL 判定、单活跃会话约束都在 `driver::jinniu`；SQLite 操作在
//! launcher 线程。错误在此边界 stringify。
//!
//! 事件（由驱动层 emit，前端订阅）：
//! - `jinniu-status-changed`：`JinniuStatePayload` 快照
//! - `jinniu-accounts-changed`：大户列表变化（无 payload）
use crate::driver::jinniu::{JinniuAccountWithStatus, JinniuLoginOptions, JinniuStatePayload};
use crate::AppState;
use tauri::State;

#[tauri::command]
pub async fn jinniu_accounts_list(
    state: State<'_, AppState>,
) -> Result<Vec<JinniuAccountWithStatus>, String> {
    state
        .driver
        .jinniu_list_accounts()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_account_add(
    state: State<'_, AppState>,
    label: String,
) -> Result<JinniuAccountWithStatus, String> {
    state
        .driver
        .jinniu_add_account(&label)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_account_remove(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .driver
        .jinniu_remove_account(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_account_set_active(
    state: State<'_, AppState>,
    id: Option<String>,
) -> Result<Option<String>, String> {
    state
        .driver
        .jinniu_set_active(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_account_get_active(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    state
        .driver
        .jinniu_get_active()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_login(
    state: State<'_, AppState>,
    id: String,
    options: Option<JinniuLoginOptions>,
) -> Result<JinniuStatePayload, String> {
    state
        .driver
        .jinniu_login(&id, options.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_disconnect(
    state: State<'_, AppState>,
    id: String,
) -> Result<JinniuStatePayload, String> {
    state
        .driver
        .jinniu_disconnect(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn jinniu_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<JinniuStatePayload, String> {
    state
        .driver
        .jinniu_status(&id)
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
            "jinniu_accounts_list",
            "jinniu_account_add",
            "jinniu_account_remove",
            "jinniu_account_set_active",
            "jinniu_account_get_active",
            "jinniu_login",
            "jinniu_disconnect",
            "jinniu_status",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
        let adapter = include_str!("jinniu.rs");
        assert_eq!(adapter.matches("\n#[tauri::command]").count(), 8);
    }
}
