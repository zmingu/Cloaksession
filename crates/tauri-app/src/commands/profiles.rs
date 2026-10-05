//! Profile CRUD + launch/close commands. The pm itself lives on the
//! dedicated launcher thread (its rusqlite Connection is `!Send + !Sync`),
//! so all profile CRUD is routed through `TauriBrowserDriver`'s async
//! helpers, which forward `LauncherCmd` variants over the channel and
//! await a oneshot reply. `launch`/`close` go through the `BrowserDriver`
//! trait impl. `is_running` (sync) is read from the driver's local cache.

use mcp_server::driver::BrowserDriver;
use multizen_core::{CreateProfileInput, LaunchedProfile, Profile, ProfileSummary, UpdateProfileInput};
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn profiles_list(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileSummary>, String> {
    let mut summaries = state.driver.list_profiles().await.map_err(|e| e.to_string())?;
    // Refresh the sync `is_running` cache flag for each summary. The driver
    // cache is updated on launch/close; an externally-killed process would
    // show stale `true` until the next close — tracked in P4.8.
    for s in &mut summaries {
        s.is_running = state.driver.is_running(&s.id);
    }
    Ok(summaries)
}

#[tauri::command]
pub async fn profiles_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<Profile>, String> {
    state.driver.get_profile(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profiles_create(
    state: State<'_, AppState>,
    input: CreateProfileInput,
) -> Result<Profile, String> {
    state
        .driver
        .create_profile(input)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profiles_update(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateProfileInput,
) -> Result<Profile, String> {
    state
        .driver
        .update_profile(&id, patch)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn profiles_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.driver.delete_profile(&id).await.map_err(|e| e.to_string())
}

/// Only these explicit entries open a platform login page; generic launch stays unchanged.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchEntry {
    KuaishouShop,
    /// Kuaishou main-site viewer (互动账号 / 小号) entry.
    KuaishouSub,
}

impl LaunchEntry {
    /// URL opened in a new tab for this explicit launch entry. Shop keeps its
    /// post-login store home redirect; the main-site viewer opens `www.kuaishou.com`.
    pub fn login_url(&self) -> &'static str {
        match self {
            Self::KuaishouShop => KUAISHOU_LOGIN_URL,
            Self::KuaishouSub => KUAISHOU_SUB_LOGIN_URL,
        }
    }
}

pub const KUAISHOU_LOGIN_URL: &str = "https://login.kwaixiaodian.com/?biz=zone&redirect_url=https%3A%2F%2Fs.kwaixiaodian.com%2Fzone%2Fhome";

/// Kuaishou main-site viewer (互动账号 / 小号) login entry.
///
/// The homepage shows no QR by itself — the QR appears only after **立即登录**
/// is clicked (navigating to the passport QR page). The QR reader drives that
/// click on each poll until the image renders, so opening the main site (with
/// `startUrl`/restored tabs) and clicking 立即登录 in the UI both lead to a
/// scannable QR. After a scan the viewer identity is read from `www.kuaishou.com`.
pub const KUAISHOU_SUB_LOGIN_URL: &str = "https://www.kuaishou.com/new-reco";

#[tauri::command]
pub async fn profiles_launch(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    id: String,
    entry: Option<LaunchEntry>,
    hidden: Option<bool>,
) -> Result<LaunchedProfile, String> {
    if entry.is_some() {
        state.driver.require_kuaishou_login_scope(&id).await.map_err(|e| e.to_string())?;
    }
    // `hidden` keeps the window off-screen for the account wizard's QR capture;
    // it never changes the saved startUrl or the fingerprint.
    let hidden = hidden.unwrap_or(false);
    // `BrowserDriver::launch` returns the full LaunchedProfile.
    let launched = mcp_server::driver::BrowserDriver::launch(state.driver.as_ref(), &id, hidden)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(entry) = entry.as_ref() {
        // Recheck after launch, then open a NEW tab. Never overwrite startUrl or restored pages.
        state.driver.require_kuaishou_login_scope(&id).await.map_err(|e| e.to_string())?;
        let tab = state.driver.new_tab(&id, entry.login_url()).await
            .map_err(|e| format!("浏览器已启动，登录扫码页未打开，可重试：{e}"))?;
        state.driver.activate_tab(&id, &tab).await
            .map_err(|e| format!("登录扫码页已打开但未激活，请切换标签页：{e}"))?;
    }

    // Spawn the companion poller for this profile — it watches Chrome Web
    // Store pages for the "Add to Cloaksession" button signal and installs
    // extensions when clicked. Non-fatal: if it fails, the user can still
    // install extensions via the toolbar UI.
    crate::commands::companion::spawn_companion_poller(
        app,
        state.driver.clone(),
        state.driver.registry().clone(),
        id,
    );

    Ok(launched)
}

#[tauri::command]
pub async fn profiles_close(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    mcp_server::driver::BrowserDriver::close(state.driver.as_ref(), &id)
        .await
        .map_err(|e| e.to_string())
}
