//! Thin IPC adapters over `cdp_driver::platforms::kuaishou`.
//!
//! The driver layer owns navigation/verification/waiting; these commands only
//! resolve the profile's [`BrowserSession`] from the registry and forward
//! phase callbacks as the `kuaishou-auth-phase` push event
//! (`{ profileId, phase }`, snake_case phases from
//! [`AuthPhase::as_str`](cdp_driver::platforms::kuaishou::AuthPhase::as_str)).
//! A missing/closed session is a plain string error: callers launch the
//! profile first. Headless restore after `scanned: true` belongs to the
//! caller, never to this layer.

use std::sync::Arc;

use cdp_driver::platforms::kuaishou::{
    self, AuthPhase, EnsureAuthOptions, EnsureAuthResult, KS_CONNECT_TIMEOUT, KS_LOGIN_TIMEOUT,
};
use cdp_driver::session::BrowserSession;
use cdp_driver::TaskCancel;
use tauri::{AppHandle, Emitter, State};

use crate::AppState;

/// Push event emitted around auth work.
pub const KUAISHOU_AUTH_PHASE_EVENT: &str = "kuaishou-auth-phase";

fn missing_session(profile_id: &str) -> String {
    format!("Profile {profile_id} 的浏览器环境未运行；请先启动该 Profile 后再进行快手登录。")
}

async fn session_for(
    state: &State<'_, AppState>,
    profile_id: &str,
) -> Result<Arc<BrowserSession>, String> {
    state
        .driver
        .registry()
        .get(profile_id)
        .await
        .ok_or_else(|| missing_session(profile_id))
}

fn emit_phase(app: &AppHandle, profile_id: &str, phase: AuthPhase) {
    let _ = app.emit(
        KUAISHOU_AUTH_PHASE_EVENT,
        serde_json::json!({ "profileId": profile_id, "phase": phase.as_str() }),
    );
}

/// Verify the shop session on `target_id`: navigates to the control page and
/// races the login-URL vs control-marker legs. Returns `true` when already
/// authenticated, `false` when a scan is required.
#[tauri::command]
pub async fn kuaishou_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
) -> Result<bool, String> {
    let session = session_for(&state, &profile_id).await?;
    emit_phase(&app, &profile_id, AuthPhase::VerifyingSession);
    kuaishou::connect(&session, &target_id, TaskCancel::new(), KS_CONNECT_TIMEOUT)
        .await
        .map_err(|e| format!("快手连接校验失败：{e}"))
}

/// Wait for the user to finish the shop QR scan on `target_id`.
#[tauri::command]
pub async fn kuaishou_login(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
) -> Result<(), String> {
    let session = session_for(&state, &profile_id).await?;
    emit_phase(&app, &profile_id, AuthPhase::WaitingForLogin);
    kuaishou::login(&session, &target_id, TaskCancel::new(), KS_LOGIN_TIMEOUT)
        .await
        .map_err(|e| format!("快手扫码登录失败或超时：{e}"))
}

/// Full flow: cookie-reuse verify first (`scanned: false`), otherwise wait
/// for a scan (`scanned: true`). Never relaunches headless itself.
#[tauri::command]
pub async fn ensure_kuaishou_auth(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
) -> Result<EnsureAuthResult, String> {
    let session = session_for(&state, &profile_id).await?;
    emit_phase(&app, &profile_id, AuthPhase::LaunchingBrowser);
    let emit_app = app.clone();
    let emit_profile = profile_id.clone();
    let options =
        EnsureAuthOptions::new(TaskCancel::new()).with_phase_callback(Arc::new(move |phase| {
            let _ = emit_app.emit(
                KUAISHOU_AUTH_PHASE_EVENT,
                serde_json::json!({ "profileId": emit_profile, "phase": phase.as_str() }),
            );
        }));
    Ok(kuaishou::ensure_auth(&session, &target_id, options).await)
}
