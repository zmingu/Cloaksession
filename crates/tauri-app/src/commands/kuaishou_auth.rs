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
    KS_SELECTOR_POLL,
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

/// Read the shop login QR from the running profile's page.
///
/// The login page renders the QR as an inline `data:image/png;base64,…` `<img>`,
/// so the exact image is lifted from the DOM instead of screenshotting (and
/// cropping) the whole page. Used by the shop account wizard while the browser
/// window stays off-screen (see `profiles_launch { hidden: true }`). Returns the
/// bare base64 payload, or `Ok(None)` when the profile has no running session or
/// the QR has not rendered yet — the wizard keeps polling instead of treating
/// either as an error.
#[tauri::command]
pub async fn kuaishou_login_qr(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Option<String>, String> {
    let Some(session) = state.driver.registry().get(&profile_id).await else {
        return Ok(None);
    };
    let pages = session
        .browser
        .pages()
        .await
        .map_err(|e| format!("二维码页面读取失败：{e}"))?;
    // Try every attached page: the login tab is not necessarily the first one,
    // and a page that is blank, mid-navigation, or not the login page simply has
    // no QR — keep looking rather than failing the poll.
    for page in pages {
        let target_id = page.target_id().as_ref().to_string();
        match kuaishou::qr_image(&session, &target_id, TaskCancel::new(), KS_SELECTOR_POLL).await {
            Ok(Some(image)) => return Ok(Some(image)),
            Ok(None) | Err(_) => continue,
        }
    }
    Ok(None)
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
