//! 自动弹品 IPC 适配层：薄命令封装，逻辑归 `crate::auto_popup`。
//!
//! 命名遵循 IPC 契约（`commands/mod.rs`）：函数名 snake_case，
//! 参数对象键 camelCase（`profileId`），结构体载荷走 serde camelCase。

use std::collections::HashMap;

use tauri::{AppHandle, Emitter, State};

use crate::AppState;
use crate::auto_popup::{
    self, AutoPopUpConfig, AutoPopUpConfigPatch, AutoPopUpStatus, GoodsInfo, ScanReport,
    ShortcutConfig, ShortcutRegisterResult,
};

/// 广播运行状态快照（`auto-popup:state`）。尽力而为：发送失败只记日志。
fn broadcast_state(app: &AppHandle, status: &AutoPopUpStatus) {
    if let Err(e) = app.emit("auto-popup:state", status) {
        tracing::warn!(error = %e, "emit auto-popup:state failed");
    }
}

#[tauri::command]
pub async fn auto_popup_start(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    config: AutoPopUpConfig,
) -> Result<AutoPopUpStatus, String> {
    let status = state
        .driver
        .auto_popup_start(&profile_id, config)
        .await?;
    broadcast_state(&app, &status);
    Ok(status)
}

#[tauri::command]
pub async fn auto_popup_stop(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    reason: Option<String>,
) -> Result<AutoPopUpStatus, String> {
    let status = state
        .driver
        .auto_popup_stop(&profile_id, reason.as_deref().unwrap_or("manual"))
        .await?;
    broadcast_state(&app, &status);
    Ok(status)
}

#[tauri::command]
pub async fn auto_popup_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AutoPopUpStatus, String> {
    Ok(state.driver.auto_popup_status(&profile_id))
}

#[tauri::command]
pub async fn auto_popup_update_config(
    state: State<'_, AppState>,
    app: AppHandle,
    profile_id: String,
    patch: AutoPopUpConfigPatch,
) -> Result<AutoPopUpStatus, String> {
    let status = state
        .driver
        .auto_popup_update_config(&profile_id, patch)
        .await?;
    broadcast_state(&app, &status);
    Ok(status)
}

#[tauri::command]
pub async fn auto_popup_goods(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<GoodsInfo>, String> {
    state.driver.auto_popup_goods(&profile_id).await
}

#[tauri::command]
pub async fn auto_popup_scan(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ScanReport, String> {
    state.driver.auto_popup_scan(&profile_id).await
}

#[tauri::command]
pub async fn auto_popup_explain_once(
    state: State<'_, AppState>,
    profile_id: String,
    goods_id: String,
) -> Result<(), String> {
    state
        .driver
        .auto_popup_explain_once(&profile_id, &goods_id)
        .await
}

#[tauri::command]
pub async fn auto_popup_register_shortcuts(
    profile_id: String,
    bindings: HashMap<String, String>,
) -> Result<ShortcutRegisterResult, String> {
    // 注册表是进程级纯内存结构，不需要 driver/AppState。
    // OS 级全局注册需用户明确授权后由应用层接线，不在此静默完成。
    Ok(auto_popup::register_shortcuts(
        &profile_id,
        ShortcutConfig { bindings },
    ))
}

#[tauri::command]
pub async fn auto_popup_unregister_shortcuts(profile_id: String) -> Result<(), String> {
    auto_popup::unregister_shortcuts(&profile_id);
    Ok(())
}

#[tauri::command]
pub async fn auto_popup_trigger_shortcut(
    state: State<'_, AppState>,
    profile_id: String,
    accelerator: String,
) -> Result<String, String> {
    state
        .driver
        .auto_popup_trigger_shortcut(&profile_id, &accelerator)
        .await
}
