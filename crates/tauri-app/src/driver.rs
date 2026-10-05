//! TauriBrowserDriver — `mcp_server::BrowserDriver` impl bridging Plan 2
//! (`cdp_driver::BrowserSession` + `browser_launcher::BrowserLauncher`) to
//! Plan 3 (`mcp_server::BrowserDriver` trait).
//!
//! # Design
//!
//! `BrowserLauncher` holds an `Arc<ProfileManager>` whose
//! `rusqlite::Connection` is `!Send + !Sync` (sqlite uses `RefCell`
//! internally), making `BrowserLauncher` itself `!Send + !Sync`. The
//! `BrowserDriver` trait requires `Self: Send + Sync`, so
//! `TauriBrowserDriver` CANNOT own a `BrowserLauncher` directly, wrap it in
//! `tokio::sync::Mutex` (which needs `T: Send`), or move it into
//! `tokio::task::spawn` / `std::thread::spawn` (both require `F: Send`).
//!
//! Resolution: a dedicated OS thread owns the `ProfileManager` + the
//! `BrowserLauncher` for their entire lifetime. The thread constructs both
//! from `db_path` + `profiles_root` (which are `Send`) inside its own
//! `current_thread` tokio runtime + `LocalSet`, then runs a command loop
//! over an `mpsc` channel. `TauriBrowserDriver` holds only the
//! `mpsc::Sender<LauncherCmd>` (which is `Send + Sync` regardless of the
//! inner type) plus a local sync `running` cache, so the driver itself is
//! `Send + Sync`. Each `BrowserDriver` method that needs the launcher sends
//! a command + a `oneshot::Sender` and awaits the reply.
//!
//! # Method resolutions
//!
//! | Trait method        | Resolution                                              |
//! |---------------------|---------------------------------------------------------|
//! | `launch`            | Send `LauncherCmd::Launch` to the launcher thread → receive `LaunchedProfile` → `registry.get_or_connect(endpoint, engine)`. Update `running` cache. |
//! | `close`             | `registry.remove` (drops CDP `Arc<BrowserSession>` → CDP closes) **then** `LauncherCmd::Close` (kills process). `BrowserSession::close(mut self)` is consuming and cannot be called through `Arc`, so the drop path is the intended teardown. |
//! | `is_running` (SYNC) | `BrowserLauncher::is_running_async` is async. The driver maintains a local `std::sync::Mutex<HashSet<ProfileId>>` updated by `launch`/`close`. The bounded `running_monitor` task reconciles it with the launcher's authoritative probe and emits `Closed`/`stopped` when a process exits on its own. |
//! | `navigate`          | `registry.get` → `session.navigate(url, timeout_ms)` → returns `NavResult.url`. |
//! | `click`             | `session.click(selector)`. |
//! | `type_text`         | `session.type_text(selector, text)`. |
//! | `extract`           | `session.extract()`. |
//! | `screenshot`        | `session.screenshot()` (base64 PNG string). |
//! | `cdp_send`          | `BrowserSession` has no raw-CDP dispatch. Only `Runtime.evaluate` is supported — extract `expression` from `params`, delegate to `session.evaluate`. Other methods return `MultizenError::Mcp`. Full raw CDP dispatch deferred. |
//!
//! # Construction
//!
//! `TauriBrowserDriver::start(db_path, profiles_root, ...)` spawns the
//! dedicated thread and returns the driver. `browser_binary` and
//! `companion_dir` are stored on the driver (from `AppSettings`) because
//! `BrowserDriver::launch(&self, profile_id, hidden)` has no parameter for the
//! binary/engine/companion paths; those come from the driver's own fields.

pub(crate) mod auto_message;
pub mod auto_reply;
mod business;
pub(crate) mod live_launch;
pub(crate) mod live_room_monitor;
pub use sub_account::SubAccountLoginResult;
pub(crate) mod scene_play;
pub mod bind_creator;
pub(crate) mod huibo_live;
pub(crate) mod shop_helper;
pub use bind_creator::{AuthorizeItem, AuthorizeListResult, BindCreatorResult};
pub mod jinniu_promote;
pub mod jinniu;
pub mod shop_product_script;
mod sub_account;
#[cfg(test)]
mod business_tests;
#[cfg(test)]
mod shop_login_tests;
mod identity;
mod account_init;
mod mate_login;
mod running_monitor;
#[cfg(test)]
mod running_monitor_tests;

pub use mate_login::{MateLoginStage, MateLoginState, MateLoginUser};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use cdp_driver::session::BrowserSession;
use mcp_server::driver::BrowserDriver;
use multizen_core::{
    BrowserEngine, ChromixSettings, CreateProfileInput, LaunchedProfile, MultizenError, Profile,
    ProfileSummary, Result, UpdateProfileInput,
};
use serde::Serialize;
use tauri::Emitter;
use tokio::sync::{mpsc, oneshot};

use crate::registry::ProfileRegistry;

const NAV_TIMEOUT_MS: u64 = 30_000;
const CMD_CHANNEL_SIZE: usize = 64;

/// Payload for the `profiles:running-changed` push event.
///
/// Discriminated union tagged on `kind`, matching the frontend
/// `RunningStateChange` type in `ui/src/types.ts`. Emitted from
/// `TauriBrowserDriver::launch` (with `kind: "launched"`) and
/// `TauriBrowserDriver::close` (with `kind: "closed"`) after the
/// operation succeeds. The frontend uses `change.kind` to drive its
/// running-indicator UI — `launched`/`closed` end any "Terminating…"
/// safety timer, `closing` would start one (we don't currently emit a
/// separate closing phase; the atomic `close()` path emits `closed`).
#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RunningStateChange {
    /// Emitted after a successful launch — the profile is now running.
    Launched { profile_id: String },
    /// Reserved for a future non-atomic close flow where the process is
    /// winding down but hasn't fully exited. Not currently emitted.
    Closing { profile_id: String },
    /// Emitted after a successful close — the profile is no longer running.
    /// `reason: "user-close"` is the explicit close path; `"external-exit"` is
    /// emitted by the bounded `running_monitor` when it detects a process that
    /// exited on its own.
    Closed {
        profile_id: String,
        reason: &'static str,
    },
}

/// Payload for the `chromium:status` push event.
///
/// Emitted alongside `profiles:running-changed` to give the frontend a
/// finer-grained lifecycle signal: `started` on successful launch, `stopped`
/// after successful close, or `failed` with an error message when launch
/// fails (the process may still be left running; `close` is the recovery
/// path).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromiumStatus {
    pub profile_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Commands sent to the dedicated launcher thread.
enum LauncherCmd {
    Identity(identity::IdentityCmd),
    Init(account_init::InitCmd),
    Launch {
        profile_id: String,
        binary: PathBuf,
        engine: BrowserEngine,
        companion: Option<PathBuf>,
        chromix: ChromixSettings,
        chromix_runtime: PathBuf,
        skip_download: bool,
        /// Start with the window off-screen (headed but not visible) so a caller
        /// can capture the page without a browser window appearing on screen.
        hidden: bool,
        resp: oneshot::Sender<Result<LaunchedProfile>>,
    },
    Close {
        profile_id: String,
        resp: oneshot::Sender<Result<()>>,
    },
    /// Authoritative liveness probe routed onto the launcher thread (the only
    /// owner of the `BrowserLauncher`). The reply is `true` while the process
    /// is genuinely alive. Used by the bounded running monitor so the sync
    /// `running` cache can be reconciled with reality.
    HealthCheck {
        profile_id: String,
        resp: oneshot::Sender<bool>,
    },
    ClosePrepared {
        profile_id: String,
        slot: Arc<crate::registry::SessionSlot>,
        resp: oneshot::Sender<Result<bool>>,
    },
    /// Ask the launcher thread to shut down (runs `close_all` then exits).
    Shutdown,
    // --- Profile commands (P4.3) ---------------------------------------
    // ProfileManager lives on the launcher thread (its rusqlite Connection
    // is `!Send + !Sync`). These variants route synchronous pm calls onto
    // that thread; each carries a oneshot for the reply.
    ListProfiles {
        resp: oneshot::Sender<Result<Vec<ProfileSummary>>>,
    },
    GetProfile {
        id: String,
        resp: oneshot::Sender<Result<Option<Profile>>>,
    },
    CreateProfile {
        input: CreateProfileInput,
        resp: oneshot::Sender<Result<Profile>>,
    },
    UpdateProfile {
        id: String,
        patch: UpdateProfileInput,
        engine: BrowserEngine,
        chromix: ChromixSettings,
        resp: oneshot::Sender<Result<Profile>>,
    },
    BusinessAccountsList {
        resp: oneshot::Sender<Result<Vec<multizen_core::BusinessAccount>>>,
    },
    BusinessProfileState {
        profile_id: String,
        resp: oneshot::Sender<Result<multizen_core::BusinessProfileState>>,
    },
    SaveBusinessAccount {
        input: multizen_core::SaveBusinessAccountInput,
        engine: BrowserEngine,
        chromix: ChromixSettings,
        resp: oneshot::Sender<Result<multizen_core::BusinessAccount>>,
    },
    UnbindBusinessAccount {
        id: String,
        resp: oneshot::Sender<Result<()>>,
    },
    DeleteBusinessAccount {
        id: String,
        resp: oneshot::Sender<Result<()>>,
    },
    RecordInteraction {
        input: profile_manager::RecordInteractionInput,
        resp: oneshot::Sender<Result<profile_manager::SubAccountInteraction>>,
    },
    ListInteractions {
        account_id: String,
        resp: oneshot::Sender<Result<Vec<profile_manager::SubAccountInteraction>>>,
    },
    DeleteProfile {
        id: String,
        resp: oneshot::Sender<Result<()>>,
    },
    InsertImported {
        profile: multizen_core::Profile,
        resp: oneshot::Sender<Result<multizen_core::Profile>>,
    },
    // --- Extensions -----------------------------------------------------
    ListExtensions {
        id: String,
        resp: oneshot::Sender<Result<Vec<multizen_core::ExtensionConfig>>>,
    },
    SetExtensions {
        id: String,
        exts: Vec<multizen_core::ExtensionConfig>,
        resp: oneshot::Sender<Result<Vec<multizen_core::ExtensionConfig>>>,
    },
    StoreEntries {
        resp: oneshot::Sender<Result<Vec<multizen_core::ExtensionConfig>>>,
    },
    SetProxyCountry {
        id: String,
        country: Option<String>,
        resp: oneshot::Sender<Result<()>>,
    },
    ListGroups {
        resp: oneshot::Sender<Result<Vec<multizen_core::GroupInfo>>>,
    },
    SetProfileGroup {
        id: String,
        group: Option<String>,
        resp: oneshot::Sender<Result<()>>,
    },
    DeleteGroup {
        name: String,
        resp: oneshot::Sender<Result<()>>,
    },
    BindCreator {
        operation: Box<dyn FnOnce(&profile_manager::ProfileManager) + Send>,
    },
    /// 金牛大户管理的一次 ProfileManager 操作（SQLite 留在 launcher 线程）。
    JinniuDb {
        operation: Box<dyn FnOnce(&profile_manager::ProfileManager) + Send>,
    },
    Scene(scene_play::SceneCmd),
    // --- Shop product scripts (jieger 商品话术库) ---------------------------
    // Storage primitives live in profile-manager; playback stays in
    // `driver/shop_product_script.rs`. Routed here like other manager ops
    // because the pm Connection lives on this thread.
    ShopProductScriptsList {
        resp: oneshot::Sender<Result<Vec<profile_manager::ShopProductScript>>>,
    },
    ShopProductScriptGet {
        id: String,
        resp: oneshot::Sender<Result<Option<profile_manager::ShopProductScriptDetail>>>,
    },
    ShopProductScriptCreate {
        input: profile_manager::CreateShopProductScriptInput,
        resp: oneshot::Sender<Result<profile_manager::ShopProductScript>>,
    },
    ShopProductScriptUpdate {
        id: String,
        patch: profile_manager::UpdateShopProductScriptInput,
        resp: oneshot::Sender<Result<profile_manager::ShopProductScript>>,
    },
    ShopProductScriptDelete {
        id: String,
        resp: oneshot::Sender<Result<()>>,
    },
    ShopProductScriptAddLine {
        input: profile_manager::AddShopProductScriptLineInput,
        resp: oneshot::Sender<Result<profile_manager::ShopProductScriptLine>>,
    },
    ShopProductScriptUpdateLine {
        id: String,
        patch: profile_manager::UpdateShopProductScriptLineInput,
        resp: oneshot::Sender<Result<profile_manager::ShopProductScriptLine>>,
    },
    ShopProductScriptDeleteLine {
        id: String,
        resp: oneshot::Sender<Result<()>>,
    },
    ShopProductScriptReorderLines {
        script_id: String,
        ordered_ids: Vec<String>,
        resp: oneshot::Sender<Result<Vec<profile_manager::ShopProductScriptLine>>>,
    },
}

pub struct TauriBrowserDriver {
    /// Channel to the dedicated launcher thread. `mpsc::Sender` is
    /// `Send + Sync` regardless of the inner type, so this field keeps
    /// `TauriBrowserDriver: Send + Sync` even though `BrowserLauncher`
    /// itself is `!Send + !Sync`.
    launcher_tx: mpsc::Sender<LauncherCmd>,
    identity: Arc<identity::IdentityRuntime>,
    account_init: Arc<account_init::InitRuntime>,
    mate_login: Arc<mate_login::MateLoginRuntime>,
    /// 磁力金牛多大户运行时（状态机 + 单活跃会话槽位）。
    pub(crate) jinniu: Arc<jinniu::JinniuRuntime>,
    sub_account: Arc<sub_account::SubAccountRuntime>,
    registry: Arc<ProfileRegistry>,
    engine: BrowserEngine,
    browser_binary: PathBuf,
    companion_dir: Option<PathBuf>,
    chromix: ChromixSettings,
    chromix_runtime: PathBuf,
    skip_download: bool,
    /// Shared extensions directory (`<data_dir>/extensions/`). Each
    /// extension is unpacked into `<extensions_root>/<ext_id>/` and
    /// referenced by `ExtensionConfig.dir` across profiles.
    extensions_root: PathBuf,
    /// Profiles root directory (`<data_dir>/profiles/`). Each profile's
    /// user data dir lives at `<profiles_root>/<profile_id>/`.
    profiles_root: PathBuf,
    /// Sync cache of profile ids believed to be running. Updated on
    /// `launch`/`close`. Used by the sync `is_running` trait method because
    /// `BrowserLauncher::is_running_async` cannot be awaited from a sync
    /// context.
    running: StdMutex<HashSet<String>>,
    /// Bounded health-check task that reconciles `running` with the launcher's
    /// authoritative liveness probe and pushes `Closed`/`stopped` for a
    /// process that exited on its own. See `driver/running_monitor.rs`.
    running_monitor: Arc<running_monitor::RunningMonitorRuntime>,
    /// Optional Tauri `AppHandle` used to emit push events
    /// (`profiles:running-changed`, `chromium:status`). Populated by
    /// `set_app` during `run()` setup. `None` in unit tests / before
    /// setup completes; emits are silently skipped in that case.
    app: StdMutex<Option<tauri::AppHandle>>,
}

impl TauriBrowserDriver {
    /// Spawn the dedicated launcher thread and return a driver wired to it.
    /// The thread constructs a `ProfileManager` from `db_path` +
    /// `profiles_root` and a `BrowserLauncher` wrapping it, then runs a
    /// `current_thread` tokio runtime + `LocalSet` command loop.
    pub fn start(
        db_path: PathBuf,
        profiles_root: PathBuf,
        extensions_root: PathBuf,
        registry: Arc<ProfileRegistry>,
        engine: BrowserEngine,
        browser_binary: PathBuf,
        companion_dir: Option<PathBuf>,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel(CMD_CHANNEL_SIZE);
        let builder = std::thread::Builder::new().name("tauri-launcher".into());
        let profiles_root_for_thread = profiles_root.clone();
        let registry_for_thread = registry.clone();
        let identity = Arc::new(identity::IdentityRuntime::new(
            db_path.with_file_name("kuaishou-avatars"),
        ));
        let account_init = Arc::new(account_init::InitRuntime::new(
            db_path.with_file_name("kuaishou-subjects"),
        ));
        let handle = builder
            .spawn(move || {
                launcher_thread_main(db_path, profiles_root_for_thread, rx, registry_for_thread)
            })
            .map_err(|e| MultizenError::Launch(format!("launcher thread spawn: {e}")))?;
        let _ = handle; // detached; exits on Shutdown or channel close
        Ok(Self {
            launcher_tx: tx,
            identity,
            account_init,
            mate_login: Arc::new(mate_login::MateLoginRuntime::new()),
            jinniu: Arc::new(jinniu::JinniuRuntime::new()),
            sub_account: Arc::new(sub_account::SubAccountRuntime::new()),
            registry,
            engine,
            browser_binary,
            companion_dir,
            chromix: ChromixSettings::default(),
            chromix_runtime: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/chromix"),
            skip_download: false,
            extensions_root,
            profiles_root,
            running: StdMutex::new(HashSet::new()),
            running_monitor: Arc::new(running_monitor::RunningMonitorRuntime::new()),
            app: StdMutex::new(None),
        })
    }

    pub fn with_chromix(
        mut self,
        settings: ChromixSettings,
        runtime_dir: PathBuf,
        skip_download: bool,
    ) -> Self {
        self.chromix = settings;
        self.chromix_runtime = runtime_dir;
        self.skip_download = skip_download;
        self
    }

    /// Get the shared extensions directory.
    pub fn extensions_root(&self) -> &Path {
        &self.extensions_root
    }

    /// Get the profile session registry (used by the companion poller to
    /// obtain the `BrowserSession` for CDP polling).
    pub fn registry(&self) -> &Arc<ProfileRegistry> {
        &self.registry
    }

    /// Profiles root directory (`<data_dir>/profiles/`). Used by archive
    /// import to compute the target data-dir path.
    pub fn profiles_root(&self) -> &Path {
        &self.profiles_root
    }

    /// Inject the Tauri `AppHandle` so `launch`/`close` can emit push events
    /// to the frontend. Called once from `run()`'s `setup` hook. Safe to
    /// call before or after the driver is `manage`d; the field is a
    /// `StdMutex<Option<_>>` and emits no-op when `None` (e.g. unit tests).
    pub fn set_app(&self, app: tauri::AppHandle) {
        self.mate_login.set_app(app.clone());
        self.jinniu.set_app(app.clone());
        *self.app.lock().unwrap() = Some(app);
    }

    /// Emit a push event. No-op when no `AppHandle` is set (unit tests,
    /// pre-setup). Errors from `emit` are logged at warn level and swallowed
    /// — push events are best-effort and must not break the launch/close
    /// path.
    fn emit<E: Serialize + Clone>(&self, event: &str, payload: &E) {
        let guard = self.app.lock().unwrap();
        if let Some(app) = guard.as_ref() {
            if let Err(e) = app.emit(event, payload) {
                tracing::warn!(event = event, error = %e, "tauri emit failed");
            }
        }
    }

    /// Best-effort graceful shutdown: tell the launcher thread to exit. The
    /// thread runs `close_all` on its `BrowserLauncher` before terminating.
    pub async fn shutdown(&self) {
        self.identity.stop.cancel();
        self.account_init.stop.cancel();
        self.mate_login.cancel_all();
        self.jinniu.stop.cancel();
        self.jinniu.cancel_all();
        self.sub_account.stop.cancel();
        self.running_monitor.stop.cancel();
        self.registry.clear().await;
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.launcher_tx.send(LauncherCmd::Shutdown),
        )
        .await;
    }
}

/// The launcher thread entry point. Constructs `ProfileManager` +
/// `BrowserLauncher` on THIS thread (so the `!Send` sqlite Connection never
/// crosses threads), then runs the command loop on a `current_thread`
/// runtime inside a `LocalSet`.
fn launcher_thread_main(
    db_path: PathBuf,
    profiles_root: PathBuf,
    mut rx: mpsc::Receiver<LauncherCmd>,
    registry: Arc<ProfileRegistry>,
) {
    let pm = match profile_manager::ProfileManager::new(&db_path, &profiles_root) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = ?e, "launcher thread: ProfileManager::new failed");
            return;
        }
    };
    // Must run once before any initialization worker can claim a step:
    // leftover `running` steps from a previous process become
    // `failed(interrupted-needs-verification)` and are re-verified later.
    if let Err(e) = pm.kuaishou_init_recover_interrupted() {
        tracing::warn!(error = %e, "launcher thread: kuaishou init recovery failed");
    }
    // `BrowserLauncher::new` takes `Arc<ProfileManager>`. `ProfileManager`
    // is `!Sync` (sqlite Connection), so the `Arc` is `!Send + !Sync`, but
    // the launcher is single-threaded by construction (lives only on this
    // launcher thread). Suppress the clippy lint — same pattern as the
    // browser-launcher integration test.
    #[allow(clippy::arc_with_non_send_sync)]
    let pm_arc = Arc::new(pm);
    let launcher = browser_launcher::BrowserLauncher::new(pm_arc.clone());

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = ?e, "launcher thread: runtime build failed");
            return;
        }
    };
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, launcher_task(launcher, pm_arc, &mut rx, registry));
}

/// Command loop run on the launcher thread's `LocalSet`. Owns the
/// `BrowserLauncher` and processes commands one at a time. Also owns an
/// `Arc<ProfileManager>` clone for synchronous profile CRUD (the pm lives
/// on this thread; commands are routed over the channel by
/// `TauriBrowserDriver`).
async fn launcher_task(
    launcher: browser_launcher::BrowserLauncher,
    pm: Arc<profile_manager::ProfileManager>,
    rx: &mut mpsc::Receiver<LauncherCmd>,
    registry: Arc<ProfileRegistry>,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            LauncherCmd::Identity(cmd) => identity::handle(cmd, &pm, &registry, &launcher).await,
            LauncherCmd::Init(cmd) => account_init::handle(cmd, &pm, &registry, &launcher).await,
            LauncherCmd::Launch {
                profile_id,
                binary,
                engine,
                companion,
                chromix,
                chromix_runtime,
                skip_download,
                hidden,
                resp,
            } => {
                let result = async {
                    let profile = pm
                        .get(&profile_id)?
                        .ok_or_else(|| MultizenError::NotFound(profile_id.clone()))?;
                    // Shared UI + embedded MCP launch gate, before mark_opened, proxy or spawn.
                    launcher
                        .validate_business_directory(&profile, engine, &chromix, None)
                        .await?;
                    // Chromix is the only engine: always launch through the SDK bridge.
                    let config = chromix.with_profile_options(&profile.chromix_options);
                    launcher
                        .launch_with_chromix(
                            &profile_id,
                            &binary,
                            companion.as_deref(),
                            &config,
                            &chromix_runtime,
                            skip_download,
                            hidden,
                        )
                        .await
                }
                .await;
                if let Ok(launched) = &result {
                    if let Ok(Some(profile)) = pm.get(&profile_id) {
                        registry
                            .prepare(
                                &profile_id,
                                &launched.cdp_endpoint,
                                engine,
                                &format!("{}:{}", launched.started_at, launched.pid),
                                Some(crate::registry::NetworkSnapshot {
                                    profile,
                                    engine,
                                    chromix,
                                    environment_uncertain: std::env::vars_os().any(|(k, v)| {
                                        let key = k.to_string_lossy().to_ascii_lowercase();
                                        !v.is_empty()
                                            && (key.contains("proxy") || key == "node_options")
                                    }),
                                }),
                            )
                            .await;
                    }
                }
                let _ = resp.send(result);
            }
            LauncherCmd::Close { profile_id, resp } => {
                registry.remove(&profile_id).await;
                let result = launcher.close(&profile_id).await;
                let _ = resp.send(result);
            }
            LauncherCmd::HealthCheck { profile_id, resp } => {
                // `is_running_async` filters through `BrowserHandle::is_alive`,
                // so a process that exited on its own already reads `false`.
                let _ = resp.send(launcher.is_running_async(&profile_id).await);
            }
            LauncherCmd::ClosePrepared {
                profile_id,
                slot,
                resp,
            } => {
                // Serialized with launch: stale attach failure must not close a replacement process.
                let result = if registry.remove_current(&profile_id, &slot).await {
                    launcher.close(&profile_id).await.map(|()| true)
                } else {
                    Ok(false)
                };
                let _ = resp.send(result);
            }
            LauncherCmd::Shutdown => {
                registry.clear().await;
                launcher.close_all().await;
                break;
            }
            LauncherCmd::ListProfiles { resp } => {
                let _ = resp.send(pm.list());
            }
            LauncherCmd::GetProfile { id, resp } => {
                let _ = resp.send(pm.get(&id));
            }
            LauncherCmd::CreateProfile { input, resp } => {
                let _ = resp.send(pm.create(input));
            }
            LauncherCmd::UpdateProfile {
                id,
                patch,
                engine,
                chromix,
                resp,
            } => {
                let _ = resp.send(
                    launcher
                        .update_profile_guarded(&id, patch, engine, &chromix)
                        .await,
                );
            }
            LauncherCmd::BusinessAccountsList { resp } => {
                let _ = resp.send(pm.business_accounts_list());
            }
            LauncherCmd::BusinessProfileState { profile_id, resp } => {
                let _ = resp.send(pm.business_accounts_profile_state(&profile_id));
            }
            LauncherCmd::SaveBusinessAccount {
                input,
                engine,
                chromix,
                resp,
            } => {
                let _ = resp.send(
                    launcher
                        .save_business_account(input, engine, &chromix)
                        .await,
                );
            }
            LauncherCmd::UnbindBusinessAccount { id, resp } => {
                let _ = resp.send(launcher.unbind_business_account(&id).await);
            }
            LauncherCmd::DeleteBusinessAccount { id, resp } => {
                let _ = resp.send(launcher.delete_business_account(&id).await);
            }
            LauncherCmd::RecordInteraction { input, resp } => {
                let _ = resp.send(pm.record_interaction(&input));
            }
            LauncherCmd::ListInteractions { account_id, resp } => {
                let _ = resp.send(pm.list_interactions(None, Some(account_id.as_str()), 500, 0));
            }
            LauncherCmd::DeleteProfile { id, resp } => {
                // Do not drop a live cookie-scope reservation through IPC deletion.
                let result = async {
                    if pm.business_profile_scope(&id)?.is_some() {
                        launcher.require_stopped(&id).await?;
                    }
                    pm.delete(&id)?;
                    registry.remove(&id).await;
                    Ok(())
                }
                .await;
                let _ = resp.send(result);
            }
            LauncherCmd::InsertImported { profile, resp } => {
                let _ = resp.send(pm.insert_imported(profile));
            }
            LauncherCmd::ListExtensions { id, resp } => {
                let _ = resp.send(
                    pm.get(&id)
                        .map(|opt| opt.and_then(|p| p.extensions).unwrap_or_default()),
                );
            }
            LauncherCmd::SetExtensions { id, exts, resp } => {
                let result = pm
                    .update(
                        &id,
                        UpdateProfileInput {
                            extensions: Some(exts),
                            ..Default::default()
                        },
                    )
                    .map(|p| p.extensions.unwrap_or_default());
                let _ = resp.send(result);
            }
            LauncherCmd::StoreEntries { resp } => {
                let result = pm.all_extension_refs().map(|refs| {
                    let mut seen = std::collections::HashSet::new();
                    let mut out = Vec::new();
                    for r in refs {
                        if seen.insert(r.ext.id.clone()) {
                            out.push(r.ext);
                        }
                    }
                    out
                });
                let _ = resp.send(result);
            }
            LauncherCmd::SetProxyCountry { id, country, resp } => {
                let _ = resp.send(pm.set_proxy_country(&id, country.as_deref()));
            }
            LauncherCmd::ListGroups { resp } => {
                let _ = resp.send(pm.list_groups());
            }
            LauncherCmd::SetProfileGroup { id, group, resp } => {
                let _ = resp.send(pm.set_profile_group(&id, group));
            }
            LauncherCmd::DeleteGroup { name, resp } => {
                let _ = resp.send(pm.delete_group(&name));
            }
            LauncherCmd::BindCreator { operation } => {
                operation(&pm);
            }
            LauncherCmd::JinniuDb { operation } => {
                operation(&pm);
            }
            LauncherCmd::Scene(cmd) => scene_play::handle(cmd, &pm).await,
            LauncherCmd::ShopProductScriptsList { resp } => {
                let _ = resp.send(pm.shop_product_scripts_list());
            }
            LauncherCmd::ShopProductScriptGet { id, resp } => {
                let _ = resp.send(pm.shop_product_script_get(&id));
            }
            LauncherCmd::ShopProductScriptCreate { input, resp } => {
                let _ = resp.send(pm.shop_product_script_create(input));
            }
            LauncherCmd::ShopProductScriptUpdate { id, patch, resp } => {
                let _ = resp.send(pm.shop_product_script_update(&id, patch));
            }
            LauncherCmd::ShopProductScriptDelete { id, resp } => {
                let _ = resp.send(pm.shop_product_script_delete(&id));
            }
            LauncherCmd::ShopProductScriptAddLine { input, resp } => {
                let _ = resp.send(pm.shop_product_script_add_line(input));
            }
            LauncherCmd::ShopProductScriptUpdateLine { id, patch, resp } => {
                let _ = resp.send(pm.shop_product_script_update_line(&id, patch));
            }
            LauncherCmd::ShopProductScriptDeleteLine { id, resp } => {
                let _ = resp.send(pm.shop_product_script_delete_line(&id));
            }
            LauncherCmd::ShopProductScriptReorderLines {
                script_id,
                ordered_ids,
                resp,
            } => {
                let _ = resp.send(pm.shop_product_script_reorder_lines(&script_id, ordered_ids));
            }
        }
    }
}

#[async_trait]
impl BrowserDriver for TauriBrowserDriver {
    async fn launch(&self, profile_id: &str, hidden: bool) -> Result<LaunchedProfile> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::Launch {
                profile_id: profile_id.to_string(),
                binary: self.browser_binary.clone(),
                engine: self.engine,
                companion: self.companion_dir.clone(),
                chromix: self.chromix.clone(),
                chromix_runtime: self.chromix_runtime.clone(),
                skip_download: self.skip_download,
                hidden,
                resp: resp_tx,
            })
            .await
            .map_err(|e| {
                // Launch channel failure → emit chromium:status failed before
                // propagating the error.
                self.emit(
                    "chromium:status",
                    &ChromiumStatus {
                        profile_id: profile_id.to_string(),
                        status: "failed".into(),
                        error: Some(e.to_string()),
                    },
                );
                MultizenError::Mcp("launcher thread closed".into())
            })?;
        let launched = match resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))
        {
            Ok(r) => match r {
                Ok(l) => l,
                Err(e) => {
                    self.emit(
                        "chromium:status",
                        &ChromiumStatus {
                            profile_id: profile_id.to_string(),
                            status: "failed".into(),
                            error: Some(e.to_string()),
                        },
                    );
                    return Err(e);
                }
            },
            Err(e) => {
                self.emit(
                    "chromium:status",
                    &ChromiumStatus {
                        profile_id: profile_id.to_string(),
                        status: "failed".into(),
                        error: Some(e.to_string()),
                    },
                );
                return Err(e);
            }
        };

        let slot = self
            .registry
            .prepared_slot(
                profile_id,
                &format!("{}:{}", launched.started_at, launched.pid),
            )
            .await?;
        // Chromix owns a persistent SDK context; close only this generation if attachment fails.
        let session = match self
            .registry
            .connect_prepared(
                profile_id,
                &format!("{}:{}", launched.started_at, launched.pid),
            )
            .await
        {
            Ok(session) => session,
            Err(e) => {
                // Chromix owns a persistent SDK context; close only this generation if attachment fails.
                let (resp, receive) = oneshot::channel();
                if self
                    .launcher_tx
                    .send(LauncherCmd::ClosePrepared {
                        profile_id: profile_id.to_string(),
                        slot: slot.clone(),
                        resp,
                    })
                    .await
                    .is_ok()
                    && matches!(receive.await, Ok(Ok(true)))
                {
                    self.registry
                        .with_absent(profile_id, || {
                            self.running.lock().unwrap().remove(profile_id);
                        })
                        .await;
                }
                self.emit(
                    "chromium:status",
                    &ChromiumStatus {
                        profile_id: profile_id.to_string(),
                        status: "failed".into(),
                        error: Some(e.to_string()),
                    },
                );
                return Err(e);
            }
        };

        // Apply CDP bootstrap (fingerprint preload, UA override, locale) so
        // the persistent fingerprint actually reaches the browser runtime.
        // Without this the preload script and UA/Accept-Language overrides
        // defined in cdp-driver were never executed after launch.
        let profile = self
            .get_profile(profile_id)
            .await?
            .ok_or_else(|| MultizenError::NotFound(profile_id.to_string()))?;
        if let Err(e) = cdp_driver::bootstrap::bootstrap_targets(
            session.as_ref(),
            &profile.fingerprint,
            self.engine,
            None,
        )
        .await
        {
            self.emit(
                "chromium:status",
                &ChromiumStatus {
                    profile_id: profile_id.to_string(),
                    status: "failed".into(),
                    error: Some(e.to_string()),
                },
            );
            return Err(e);
        }

        self.registry
            .with_current(profile_id, &slot, || {
                self.running.lock().unwrap().insert(profile_id.to_string());
                // Success → notify frontend. `profiles:running-changed` carries the
                // authoritative running state; `chromium:status` carries the
                // lifecycle signal.
                self.emit(
                    "profiles:running-changed",
                    &RunningStateChange::Launched {
                        profile_id: profile_id.to_string(),
                    },
                );
                self.emit(
                    "chromium:status",
                    &ChromiumStatus {
                        profile_id: profile_id.to_string(),
                        status: "started".into(),
                        error: None,
                    },
                );
            })
            .await
            .ok_or_else(|| {
                MultizenError::Cdp("launch session was invalidated during bootstrap".into())
            })?;
        Ok(launched)
    }

    async fn close(&self, profile_id: &str) -> Result<()> {
        // 1. Drop the CDP session (Arc<BrowserSession>). When the last Arc
        //    goes away the chromiumoxide Browser + its CDP connection drop,
        //    closing the WebSocket. `BrowserSession::close(mut self)` is
        //    consuming and cannot be called through Arc, so the drop path is
        //    the intended teardown for shared sessions.
        self.registry.remove(profile_id).await;
        // 2. Kill the browser process (also stops the socks5 bridge) via the
        //    dedicated launcher thread.
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::Close {
                profile_id: profile_id.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))??;
        self.registry
            .with_absent(profile_id, || {
                self.running.lock().unwrap().remove(profile_id);
                // Success → notify frontend that the profile is no longer running
                // and the chromium process has stopped.
                self.emit(
                    "profiles:running-changed",
                    &RunningStateChange::Closed {
                        profile_id: profile_id.to_string(),
                        reason: "user-close",
                    },
                );
                self.emit(
                    "chromium:status",
                    &ChromiumStatus {
                        profile_id: profile_id.to_string(),
                        status: "stopped".into(),
                        error: None,
                    },
                );
            })
            .await;
        Ok(())
    }

    fn is_running(&self, profile_id: &str) -> bool {
        // Sync trait method — cannot await launcher.is_running_async.
        // Use the local cache, which the bounded `running_monitor` task
        // reconciles with the launcher's authoritative liveness probe (see
        // `driver/running_monitor.rs`), so an externally-exited process
        // converges to `false` within one poll interval.
        self.running.lock().unwrap().contains(profile_id)
    }

    async fn navigate(&self, profile_id: &str, url: &str) -> Result<String> {
        let session = self.require_session(profile_id).await?;
        let nav = session.navigate(url, NAV_TIMEOUT_MS).await?;
        Ok(nav.url)
    }

    async fn click(&self, profile_id: &str, selector: &str) -> Result<()> {
        let session = self.require_session(profile_id).await?;
        session.click(selector).await
    }

    async fn type_text(&self, profile_id: &str, selector: &str, text: &str) -> Result<()> {
        let session = self.require_session(profile_id).await?;
        session.type_text(selector, text).await
    }

    async fn extract(&self, profile_id: &str) -> Result<serde_json::Value> {
        let session = self.require_session(profile_id).await?;
        session.extract().await
    }

    async fn screenshot(&self, profile_id: &str) -> Result<String> {
        let session = self.require_session(profile_id).await?;
        session.screenshot().await
    }

    async fn cdp_send(
        &self,
        profile_id: &str,
        method: &str,
        params: Option<serde_json::Value>,
        session_id: Option<&str>,
        _safe: bool,
    ) -> Result<serde_json::Value> {
        let session = self.require_session(profile_id).await?;
        session.cdp_send(method, params, session_id).await
    }
}

impl TauriBrowserDriver {
    /// Fetch the active session for `profile_id` or return an error. Does NOT
    /// auto-launch — `launch` is the explicit entry point. If a caller
    /// invokes a tool method before `launch`, that's a usage error.
    async fn require_session(&self, profile_id: &str) -> Result<Arc<BrowserSession>> {
        self.registry.get(profile_id).await.ok_or_else(|| {
            MultizenError::Mcp(format!(
                "no active browser session for profile `{profile_id}`; \
                 call launch first"
            ))
        })
    }

    // --- Profile CRUD (P4.3) -------------------------------------------
    // Each method sends a `LauncherCmd` variant + oneshot to the launcher
    // thread, where `ProfileManager` lives. The pm methods are synchronous;
    // they execute on the launcher thread and reply via the oneshot.

    pub async fn list_profiles(&self) -> Result<Vec<ProfileSummary>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ListProfiles { resp: resp_tx })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn get_profile(&self, id: &str) -> Result<Option<Profile>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::GetProfile {
                id: id.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn activate_tab(&self, profile_id: &str, tab_id: &str) -> Result<()> {
        let session = self.require_session(profile_id).await?;
        session.activate_page(tab_id).await
    }

    pub async fn new_tab(&self, profile_id: &str, url: &str) -> Result<String> {
        let session = self.require_session(profile_id).await?;
        session.new_page(url).await
    }

    pub async fn close_tab(&self, profile_id: &str, tab_id: &str) -> Result<()> {
        let session = self.require_session(profile_id).await?;
        session.close_page(tab_id).await
    }

    pub async fn create_profile(&self, input: CreateProfileInput) -> Result<Profile> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::CreateProfile {
                input,
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn update_profile(&self, id: &str, patch: UpdateProfileInput) -> Result<Profile> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::UpdateProfile {
                id: id.to_string(),
                patch,
                engine: self.engine,
                chromix: self.chromix.clone(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn delete_profile(&self, id: &str) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::DeleteProfile {
                id: id.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn insert_imported(
        &self,
        profile: multizen_core::Profile,
    ) -> Result<multizen_core::Profile> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::InsertImported {
                profile,
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    // --- Extensions -----------------------------------------------------

    pub async fn list_extensions(&self, id: &str) -> Result<Vec<multizen_core::ExtensionConfig>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ListExtensions {
                id: id.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn set_extensions(
        &self,
        id: &str,
        exts: Vec<multizen_core::ExtensionConfig>,
    ) -> Result<Vec<multizen_core::ExtensionConfig>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::SetExtensions {
                id: id.to_string(),
                exts,
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn store_entries(&self) -> Result<Vec<multizen_core::ExtensionConfig>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::StoreEntries { resp: resp_tx })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn set_proxy_country(&self, id: &str, country: Option<String>) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::SetProxyCountry {
                id: id.to_string(),
                country,
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn list_groups(&self) -> Result<Vec<multizen_core::GroupInfo>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ListGroups { resp: resp_tx })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn set_profile_group(&self, id: &str, group: Option<String>) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::SetProfileGroup {
                id: id.to_string(),
                group,
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn delete_group(&self, name: &str) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::DeleteGroup {
                name: name.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        resp_rx
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }
}

impl Drop for TauriBrowserDriver {
    fn drop(&mut self) {
        self.identity.stop.cancel();
        self.account_init.stop.cancel();
        self.jinniu.stop.cancel();
        self.sub_account.stop.cancel();
        self.running_monitor.stop.cancel();
    }
}
