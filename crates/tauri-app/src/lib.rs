//! tauri-app library core — the integration seam between Plan 2
//! (cdp-driver / browser-launcher) and Plan 3 (mcp-server BrowserDriver).
//!
//! The binary target (`src/main.rs`) wraps this lib for the Tauri shell.

pub mod commands;
pub mod driver;
pub mod mcp_embed;
pub mod registry;
pub mod token;
// Declared here (not in `driver.rs`) so `driver.rs` stays untouched:
// the file still lives at `driver/comment_listener.rs`.
#[path = "driver/comment_listener.rs"]
pub mod comment_listener;
/// jieger 自动弹品移植。不经 `driver.rs` 挂载（`#[path]` 直指文件），
/// 避免触碰并行工作流归属的 `driver.rs`；对外路径为 `crate::auto_popup`。
#[path = "driver/auto_popup.rs"]
pub mod auto_popup;

pub use driver::TauriBrowserDriver;
pub use registry::ProfileRegistry;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mcp_server::activity::ActivityLog;
use multizen_core::{AppSettings, BrowserEngine};
use settings_store::{default_settings_path, SettingsStore};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

use crate::commands::{
    activity::activity_recent,
    archive::{profiles_export_archive, profiles_import_archive},
    auto_message::{auto_message_start, auto_message_status, auto_message_stop},
    bind_creator::{
        bind_creator_get_authorize_list, bind_creator_start_authorize,
        bind_creator_sync_authorize_list,
    },
    auto_popup::{
        auto_popup_explain_once, auto_popup_goods, auto_popup_register_shortcuts,
        auto_popup_scan, auto_popup_start, auto_popup_status, auto_popup_stop,
        auto_popup_trigger_shortcut, auto_popup_unregister_shortcuts, auto_popup_update_config,
    },
    auto_reply::{auto_reply_history, auto_reply_preview, auto_reply_record},
    business_accounts::{
        business_accounts_list, business_accounts_profile_state, business_accounts_save,
        business_accounts_unbind,
    },
    comment_listener::{
        comment_events_history, comment_events_next, comment_events_recent,
        comment_listener_start, comment_listener_status, comment_listener_status_all,
        comment_listener_stop,
    },
    dialog::{dialog_pick_browser_binary, dialog_pick_directory},
    extensions::{
        extensions_add_from_file, extensions_add_from_folder, extensions_add_from_web_store,
        extensions_icon, extensions_list, extensions_prepare_from_file,
        extensions_prepare_from_folder, extensions_prepare_from_web_store, extensions_remove,
        extensions_store_entries, extensions_toggle,
    },
    fingerprint::{
        fingerprint_devices, fingerprint_generate, fingerprint_locale_for_country,
        fingerprint_locales, fingerprint_reconcile,
    },
    groups::{
        profiles_delete_group, profiles_list_groups, profiles_set_profile_group,
    },
    huibo_live::{cancel_huibo_task, get_huibo_video_list, get_shop_live_state, start_huibo_live},
    kuaishou_auth::{ensure_kuaishou_auth, kuaishou_connect, kuaishou_login, kuaishou_login_qr},
    kuaishou_identity::{
        kuaishou_identity_avatar, kuaishou_identity_detect, kuaishou_identity_list,
    },
    kuaishou_init::{
        kuaishou_init_retry, kuaishou_init_steps, kuaishou_ocr_availability,
        kuaishou_subject_attachment, kuaishou_subject_confirm, kuaishou_subject_correct,
        kuaishou_subject_detail, kuaishou_subject_list, kuaishou_subject_reocr,
    },
    mate_login::{
        mate_account_add, mate_account_remove, mate_account_rename, mate_accounts_list,
        mate_login_cancel, mate_login_start, mate_login_state,
    },
    jinniu::{
        jinniu_account_add, jinniu_account_get_active, jinniu_account_remove,
        jinniu_account_set_active, jinniu_accounts_list, jinniu_disconnect, jinniu_login,
        jinniu_status,
    },
    jinniu_promote::{
        jinniu_promote_apply_phase1, jinniu_promote_apply_phase2, jinniu_promote_live_users,
        jinniu_promote_open_store_create, jinniu_promote_select_live_user, jinniu_promote_submit,
    },
    live_launch::{
        live_launch_credentials, live_launch_heartbeat_start, live_launch_heartbeat_stop,
        live_launch_mate_credentials, live_launch_mate_heartbeat, live_launch_mate_stream_start,
        live_launch_mate_stream_stop, live_launch_prerequisites, live_launch_status,
        live_launch_stream_start, live_launch_stream_stop,
    },
    profiles::{
        profiles_close, profiles_create, profiles_delete, profiles_get, profiles_launch,
        profiles_list, profiles_update,
    },
    proxy::proxy_detect_geo,
    scene_play::{
        scene_add_line, scene_create, scene_delete, scene_delete_line, scene_get, scene_list,
        scene_play, scene_reorder_lines, scene_stop, scene_update, scene_update_line,
    },
    settings::{settings_get, settings_update},
    shop_helper::{
        shop_helper_add_to_cart, shop_helper_read_goods, shop_helper_remove_from_cart,
        shop_helper_switch_tab,
    },
    shop_product_script::{
        shop_product_script_add_line, shop_product_script_create, shop_product_script_delete,
        shop_product_script_delete_line, shop_product_script_get, shop_product_script_play,
        shop_product_script_reorder_lines, shop_product_script_stop,
        shop_product_script_update, shop_product_script_update_line, shop_product_scripts_list,
    },
    sub_account::{
        batch_login_sub_accounts, delete_sub_account, list_sub_accounts, save_sub_account,
        sub_account_enter_live_room, sub_account_interactions, sub_account_login,
        sub_account_send_danmaku, unbind_sub_account,
    },
    system::system_info,
    update::{update_check, update_download, update_install, update_last_checked, update_status},
};

/// Process-wide shared state injected into Tauri via `Builder::manage`.
///
/// `SettingsStore::load`/`update` need `&mut self`, so it's wrapped in a
/// `tokio::sync::Mutex`. `mcp_token` is similarly guarded — P4.4 generates
/// it; until then it stays `None`. `TauriBrowserDriver` and `ActivityLog`
/// are interior-sync and can live behind a plain `Arc`.
pub struct AppState {
    pub driver: Arc<TauriBrowserDriver>,
    pub settings: Mutex<SettingsStore>,
    pub activity: Arc<ActivityLog>,
    pub mcp_token: Mutex<Option<String>>,
    pub update: std::sync::Mutex<crate::commands::update::UpdateState>,
    /// profiles.db path, for commands that need short-lived direct
    /// `ProfileManager` access (auto_reply history/record). The
    /// authoritative manager still lives on the launcher thread.
    pub db_path: PathBuf,
    /// profiles root dir, paired with `db_path` for the above.
    pub profiles_root: PathBuf,
}

/// Resolve the on-disk paths for `profiles.db`, `profiles/`, and
/// `settings.json` from the Tauri app handle. Uses `app_local_data_dir`
/// (e.g. `%LOCALAPPDATA%\com.multizen.browser` on Windows) and falls back
/// to `app_config_dir` then the current working directory.
fn resolve_paths(app: &tauri::AppHandle) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let base = app
        .path()
        .app_local_data_dir()
        .or_else(|_| app.path().app_config_dir())
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let db_path = base.join("profiles.db");
    let profiles_root = base.join("profiles");
    let extensions_root = base.join("extensions");
    let settings_path = default_settings_path(&base);
    (db_path, profiles_root, extensions_root, settings_path)
}

/// Fallback browser binary path when settings has none. Looks for
/// `MULTIZEN_BROWSER_BINARY` env var first. Chromix is the only engine: with no
/// override the path is left empty so the SDK bridge resolves or downloads the
/// binary itself. The launcher surfaces the real error if resolution fails.
fn default_browser_binary(engine: BrowserEngine) -> PathBuf {
    if let Ok(path) = std::env::var("MULTIZEN_BROWSER_BINARY") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    match engine {
        // Chromix (the only engine) resolves its binary through the SDK.
        BrowserEngine::Chromix => PathBuf::new(),
    }
}

fn chromix_runtime_dir(app: &tauri::AppHandle) -> PathBuf {
    if cfg!(debug_assertions) {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/chromix");
    }
    app.path()
        .resource_dir()
        .unwrap_or_default()
        .join("chromix")
}

/// Build the `AppState` from the Tauri app handle: load settings,
/// construct the `TauriBrowserDriver` (spawns the dedicated launcher
/// thread), and allocate the shared `ActivityLog` + mcp token slot.
///
/// Returns the state plus the resolved data-dir path (used by callers
/// to locate the `mcp-token` file).
fn build_app_state(app: &tauri::AppHandle) -> (AppState, PathBuf) {
    let (db_path, profiles_root, extensions_root, settings_path) = resolve_paths(app);
    let data_dir = settings_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let mut store = SettingsStore::new(&settings_path);
    let settings: AppSettings = match store.load() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                "settings load failed at {}: {e}; using defaults",
                settings_path.display()
            );
            AppSettings::default()
        }
    };
    let engine = settings.browser_engine;
    let browser_binary = settings
        .browser_binary_path
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_browser_binary(engine));
    // Set up the companion extension (injects "Add to Cloaksession" button on
    // Chrome Web Store pages). Files are embedded at compile time and
    // written to the data dir on startup so they survive across launches.
    let companion_root = data_dir.join("companion");
    std::fs::create_dir_all(&companion_root).ok();
    let companion_manifest = include_str!("../resources/companion/manifest.json");
    let companion_cs = include_str!("../resources/companion/cs.js");
    let manifest_path = companion_root.join("manifest.json");
    let cs_path = companion_root.join("cs.js");
    // Only rewrite if content differs (avoid touching disk every launch).
    let needs_write = std::fs::read_to_string(&manifest_path).ok().as_deref()
        != Some(companion_manifest)
        || std::fs::read_to_string(&cs_path).ok().as_deref() != Some(companion_cs);
    if needs_write {
        std::fs::write(&manifest_path, companion_manifest).ok();
        std::fs::write(&cs_path, companion_cs).ok();
    }
    let companion_dir = Some(companion_root);

    // Ensure the shared extensions directory exists so extension commands
    // can just write into `extensions_root/<ext_id>/`.
    std::fs::create_dir_all(&extensions_root).ok();

    let registry = Arc::new(ProfileRegistry::new());
    let driver = TauriBrowserDriver::start(
        db_path.clone(),
        profiles_root.clone(),
        extensions_root,
        registry,
        engine,
        browser_binary,
        companion_dir,
    )
    .expect("TauriBrowserDriver::start")
    .with_chromix(
        settings.chromix,
        chromix_runtime_dir(app),
        settings.skip_browser_download,
    );

    let state = AppState {
        driver: Arc::new(driver),
        settings: Mutex::new(store),
        activity: Arc::new(ActivityLog::new()),
        mcp_token: Mutex::new(None),
        update: std::sync::Mutex::new(crate::commands::update::UpdateState::default()),
        db_path,
        profiles_root,
    };
    (state, data_dir)
}

/// Probe geo for every profile that has a proxy but no cached country code.
/// Runs sequentially (don't hammer the upstream proxy). Emits a
/// `profiles:proxy-country-updated` event after each successful probe so the
/// renderer refetches and re-renders flag chips. Failures are non-fatal.
async fn backfill_proxy_countries(app: tauri::AppHandle, driver: Arc<TauriBrowserDriver>) {
    let summaries = match driver.list_profiles().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = ?e, "backfill: list_profiles failed");
            return;
        }
    };
    for summary in summaries {
        // Skip profiles with no proxy or a cached country already.
        if summary.proxy.is_none() || summary.proxy_country.is_some() {
            continue;
        }
        let Some(proxy) = summary.proxy else { continue };
        let proxy_config = multizen_core::ProxyConfig {
            proxy_type: proxy.proxy_type,
            host: proxy.host,
            port: proxy.port,
            username: proxy.username,
            password: proxy.password,
        };
        match browser_launcher::proxy_geo::probe_proxy_geo(&proxy_config, 6_000).await {
            Ok(geo) => {
                let country = geo.country.to_lowercase();
                let _ = driver
                    .set_proxy_country(&summary.id, Some(country.clone()))
                    .await;
                let _ = app.emit(
                    "profiles:proxy-country-updated",
                    serde_json::json!({ "id": &summary.id, "country": country }),
                );
            }
            Err(e) => {
                tracing::debug!(error = ?e, profile = %summary.id, "backfill: probe failed");
            }
        }
    }
    tracing::info!("backfill_proxy_countries complete");
}

/// Delete extension directories in `extensions/` that no profile references.
/// Best-effort — errors are logged and swallowed.
async fn sweep_extension_orphans(driver: Arc<TauriBrowserDriver>) {
    let extensions_root = driver.extensions_root().to_path_buf();

    // Get all referenced extension dirs from profiles.
    let referenced = match driver.store_entries().await {
        Ok(entries) => entries
            .into_iter()
            .filter_map(|e| {
                let p = PathBuf::from(&e.dir);
                p.file_name().map(|n| n.to_string_lossy().to_string())
            })
            .collect::<std::collections::HashSet<_>>(),
        Err(e) => {
            tracing::warn!(error = ?e, "orphan sweep: store_entries failed");
            return;
        }
    };

    // Scan the extensions directory and remove unreferenced dirs.
    let read_dir = match tokio::fs::read_dir(&extensions_root).await {
        Ok(d) => d,
        Err(_) => return, // dir doesn't exist yet — nothing to sweep.
    };

    let mut removed = 0u32;
    let mut entries = read_dir;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        // Skip temp dirs from interrupted unpacks.
        if name.ends_with(".tmp_unpacked") {
            let _ = tokio::fs::remove_dir_all(&path).await;
            removed += 1;
            continue;
        }
        if !referenced.contains(&name) {
            if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                tracing::warn!(error = ?e, dir = %path.display(), "orphan sweep: remove failed");
            } else {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        tracing::info!(removed, "extension orphan sweep reclaimed");
    }
}

/// Build the Tauri app and run it. Lives in the lib crate (not the bin)
/// because `tauri::generate_handler!` expands to references to the
/// `__cmd__<name>` macros that `#[tauri::command]` generates in THIS
/// crate — keeping the handler list in the lib crate ensures the macro
/// can see those generated names.
pub fn run() {
    // Initialize tracing subscriber so `tracing::info!`/`warn!` actually
    // output to stderr. Controlled by RUST_LOG env var (default: info).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let (state, data_dir) = build_app_state(app.handle());

            // Wire the driver's `AppHandle` so launch/close can emit
            // `profiles:running-changed` / `chromium:status` push events.
            state.driver.set_app(app.handle().clone());
            // Identity starts first; initialization only claims eligible unfinished
            // steps after fresh account/session/scope checks. Both start once.
            // The sub-account login poller is read-only and independent.
            state.driver.start_kuaishou_monitors();
            state.driver.start_sub_account_login_monitor();
            // Reconciles the sync `running` cache with real process liveness and
            // pushes `Closed`/`stopped` when a browser exits on its own.
            state.driver.start_running_monitor();

            // Spawn a background task that bridges `ActivityLog`'s broadcast
            // stream to the Tauri frontend via `activity:event`. Every
            // `start_call` (pending) and `finish` (completed) event is emitted;
            // the frontend filters as needed. Broadcast lag is logged and
            // recovered (the next event resyncs the receiver).
            {
                let app_handle = app.handle().clone();
                let activity = state.activity.clone();
                tauri::async_runtime::spawn(async move {
                    let mut rx = activity.subscribe();
                    tracing::info!("activity:event bridge task started");
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                if let Err(e) = app_handle.emit("activity:event", &event) {
                                    tracing::warn!(
                                        error = %e,
                                        "emit activity:event failed"
                                    );
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    skipped = n,
                                    "activity:event bridge lagged; resyncing"
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::info!("activity:event bridge: sender closed; exiting");
                                break;
                            }
                        }
                    }
                });
            }

            // Bridge auto-popup run events (`Started/Stopped/Explained/…`) to
            // the frontend via `auto-popup:event`. Same best-effort pattern
            // as the activity bridge above: lag is logged and resynced,
            // close ends the task. Command-level snapshots use the separate
            // `auto-popup:state` event emitted directly by each command.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut rx = crate::auto_popup::subscribe();
                    tracing::info!("auto-popup:event bridge task started");
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                if let Err(e) = app_handle.emit("auto-popup:event", &event) {
                                    tracing::warn!(
                                        error = %e,
                                        "emit auto-popup:event failed"
                                    );
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    skipped = n,
                                    "auto-popup:event bridge lagged; resyncing"
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::info!("auto-popup:event bridge: sender closed; exiting");
                                break;
                            }
                        }
                    }
                });
            }

            // Load (or create) the MCP bearer token.
            match token::load_or_create_mcp_token(&data_dir) {
                Ok(tok) => {
                    tracing::info!(
                        "mcp token loaded from {}",
                        data_dir.join("mcp-token").display()
                    );
                    // Decide whether to spawn the HTTP server.
                    let port = {
                        let mut store = state.settings.blocking_lock();
                        let settings = store.load().unwrap_or_default();
                        if settings.mcp_http_enabled {
                            Some(settings.mcp_http_port)
                        } else {
                            None
                        }
                    };
                    if let Some(port) = port {
                        mcp_embed::start_embedded_mcp(
                            port,
                            tok.clone(),
                            state.driver.clone(),
                            state.activity.clone(),
                        );
                        tracing::info!("embedded mcp http server requested on port {port}");
                    } else {
                        tracing::info!("mcp http disabled in settings; not spawning server");
                    }
                    *state.mcp_token.blocking_lock() = Some(tok);
                }
                Err(e) => {
                    tracing::warn!(
                        "failed to load/create mcp token at {}: {e}",
                        data_dir.join("mcp-token").display()
                    );
                }
            }

            // Extract Arcs for background tasks before `state` is moved into
            // `app.manage`.
            let driver_for_backfill = state.driver.clone();
            let driver_for_sweep = state.driver.clone();

            app.manage(state);

            // Background: backfill proxy countries for profiles missing a
            // cached country code. Runs sequentially so we don't hammer the
            // same upstream proxy with parallel requests. Each probe emits
            // a `profiles:proxy-country-updated` event so the renderer
            // refetches and re-renders flag chips. Failures are non-fatal.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    backfill_proxy_countries(app_handle, driver_for_backfill).await;
                });
            }

            // Background: sweep the shared extensions/ dir for dirs no
            // profile references (left by a crash mid-delete or manual
            // removal). Best-effort, never blocks startup.
            {
                tauri::async_runtime::spawn(async move {
                    sweep_extension_orphans(driver_for_sweep).await;
                });
            }

            // Background: auto-check for updates 8s after launch if
            // `autoUpdate` is enabled in settings (mirrors the original
            // Electron `POST_LAUNCH_DELAY_MS` behavior). Non-fatal.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                    let auto_update = {
                        let app_state = app_handle.state::<AppState>();
                        let mut store = app_state.settings.lock().await;
                        store.load().unwrap_or_default().auto_update
                    };
                    if auto_update {
                        tracing::info!("auto-update: checking for updates");
                        let app_state = app_handle.state::<AppState>();
                        let _ = update_check(app_handle.clone(), app_state).await;
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // jieger keyword auto-reply
            auto_reply_preview,
            auto_reply_history,
            auto_reply_record,
            // business metadata (never a login command)
            business_accounts_list,
            business_accounts_profile_state,
            business_accounts_save,
            business_accounts_unbind,
            bind_creator_get_authorize_list,
            bind_creator_sync_authorize_list,
            bind_creator_start_authorize,
            // auto-message timeline (主播互动时间轴弹幕)
            auto_message_start,
            auto_message_stop,
            auto_message_status,
            // live comment listener (jieger)
            comment_listener_start,
            comment_listener_stop,
            comment_listener_status,
            comment_listener_status_all,
            comment_events_recent,
            comment_events_history,
            comment_events_next,
            kuaishou_identity_list,
            kuaishou_identity_detect,
            kuaishou_identity_avatar,
            // kuaishou platform auth primitives (connect / login / ensure)
            kuaishou_connect,
            kuaishou_login,
            kuaishou_login_qr,
            ensure_kuaishou_auth,
            // kuaishou subject archive + account initialization
            kuaishou_subject_list,
            kuaishou_subject_detail,
            kuaishou_subject_correct,
            kuaishou_subject_confirm,
            kuaishou_subject_reocr,
            kuaishou_subject_attachment,
            kuaishou_init_steps,
            kuaishou_init_retry,
            kuaishou_ocr_availability,
            // jinniu accounts (多大户管理: 状态机 + 单选切换 + 登录/断开)
            jinniu_accounts_list,
            jinniu_account_add,
            jinniu_account_remove,
            jinniu_account_set_active,
            jinniu_account_get_active,
            jinniu_login,
            jinniu_disconnect,
            jinniu_status,
            // jinniu promote (F1 live-user switch + plan-build F5/F6/F7)
            jinniu_promote_open_store_create,
            jinniu_promote_live_users,
            jinniu_promote_select_live_user,
            jinniu_promote_apply_phase1,
            jinniu_promote_apply_phase2,
            jinniu_promote_submit,
            // viewer sub-accounts (小号): CRUD + batch login + room + danmaku
            save_sub_account,
            list_sub_accounts,
            unbind_sub_account,
            delete_sub_account,
            sub_account_login,
            batch_login_sub_accounts,
            sub_account_enter_live_room,
            sub_account_send_danmaku,
            sub_account_interactions,
            // shop product scripts (jieger 商品话术库)
            shop_product_scripts_list,
            shop_product_script_get,
            shop_product_script_create,
            shop_product_script_update,
            shop_product_script_delete,
            shop_product_script_add_line,
            shop_product_script_update_line,
            shop_product_script_delete_line,
            shop_product_script_reorder_lines,
            shop_product_script_play,
            shop_product_script_stop,
            // scene play
            scene_create,
            scene_get,
            scene_list,
            scene_update,
            scene_delete,
            scene_add_line,
            scene_update_line,
            scene_delete_line,
            scene_reorder_lines,
            scene_play,
            scene_stop,
            // live-mate accounts + HTTP QR login (tokens persisted on success)
            mate_accounts_list,
            mate_account_add,
            mate_account_remove,
            mate_account_rename,
            mate_login_start,
            mate_login_cancel,
            mate_login_state,
            // live launch (jieger 开播控制)
            live_launch_status,
            live_launch_prerequisites,
            live_launch_credentials,
            live_launch_heartbeat_start,
            live_launch_heartbeat_stop,
            live_launch_stream_start,
            live_launch_stream_stop,
            // live launch — mate (伴侣账号，liveMate 取流，不启浏览器)
            live_launch_mate_credentials,
            live_launch_mate_stream_start,
            live_launch_mate_stream_stop,
            live_launch_mate_heartbeat,
            // profiles
            profiles_list,
            profiles_get,
            profiles_create,
            profiles_update,
            profiles_delete,
            profiles_launch,
            profiles_close,
            profiles_list_groups,
            profiles_set_profile_group,
            profiles_delete_group,
            // archive
            profiles_export_archive,
            profiles_import_archive,
            // settings
            settings_get,
            settings_update,
            // shop helper (follow-assistant cart; isolated from CPS shelving)
            shop_helper_read_goods,
            shop_helper_switch_tab,
            shop_helper_add_to_cart,
            shop_helper_remove_from_cart,
            // dialog
            dialog_pick_browser_binary,
            dialog_pick_directory,
            // fingerprint
            fingerprint_generate,
            fingerprint_devices,
            fingerprint_locales,
            fingerprint_reconcile,
            fingerprint_locale_for_country,
            // proxy
            proxy_detect_geo,
            // system
            system_info,
            // activity
            activity_recent,
            // update
            update_status,
            update_last_checked,
            update_check,
            update_install,
            update_download,
            // extensions
            extensions_list,
            extensions_add_from_web_store,
            extensions_add_from_file,
            extensions_add_from_folder,
            extensions_remove,
            extensions_toggle,
            extensions_store_entries,
            extensions_prepare_from_web_store,
            extensions_prepare_from_file,
            extensions_prepare_from_folder,
            extensions_icon,
            // auto-popup (jieger autoPopUp port)
            auto_popup_start,
            auto_popup_stop,
            auto_popup_status,
            auto_popup_update_config,
            auto_popup_goods,
            auto_popup_scan,
            auto_popup_explain_once,
            auto_popup_register_shortcuts,
            auto_popup_unregister_shortcuts,
            auto_popup_trigger_shortcut,
            get_huibo_video_list,
            start_huibo_live,
            get_shop_live_state,
            cancel_huibo_task,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<AppState>();
                state.driver.stop_kuaishou_identity_monitor();
                state.driver.stop_kuaishou_init_monitor();
                state.driver.stop_sub_account_login_monitor();
                state.driver.stop_jinniu();
                state.driver.stop_running_monitor();
            }
        });
}
