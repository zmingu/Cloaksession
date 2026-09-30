use super::*;
use multizen_core::{
    BusinessAccountKind as Kind, BusinessProfileScope as Scope, SaveBusinessAccountInput,
};
use serde_json::json;
use tempfile::TempDir;

pub(super) fn fixture(global: ChromixSettings) -> (TempDir, TauriBrowserDriver) {
    let dir = TempDir::new().unwrap();
    // Protocol-only fake bridge. No SDK, CDP websocket, platform request or browser process.
    std::fs::write(dir.path().join("bridge.mjs"), r#"
import { createInterface } from 'node:readline';
import { writeFileSync } from 'node:fs';
const lines = createInterface({input:process.stdin});
lines.on('line', line => {
  const request = JSON.parse(line);
  if (request.type === 'launch') {
    if (process.env.CAPTURE_FILE) writeFileSync(process.env.CAPTURE_FILE, JSON.stringify(request));
    process.stdout.write(JSON.stringify({type:'ready',cdpEndpoint:`http://127.0.0.1:${request.cdpPort}`})+'\n');
  } else if (request.type === 'close') {
    process.stdout.write('{"type":"closed"}\n'); process.exit(0);
  }
});
"#).unwrap();
    let driver = TauriBrowserDriver::start(
        dir.path().join("p.db"),
        dir.path().join("profiles"),
        dir.path().join("extensions"),
        Arc::new(ProfileRegistry::new()),
        BrowserEngine::Chromix,
        PathBuf::new(),
        None,
    )
    .unwrap()
    .with_chromix(global, dir.path().into(), true);
    (dir, driver)
}

async fn profile(d: &TauriBrowserDriver, name: &str) -> Profile {
    d.create_profile(CreateProfileInput {
        name: name.into(),
        ..Default::default()
    })
    .await
    .unwrap()
}
fn input(p: &Profile, kind: Kind) -> SaveBusinessAccountInput {
    SaveBusinessAccountInput {
        id: None,
        profile_id: p.id.clone(),
        kind,
        display_name: "manual".into(),
        platform_user_id: None,
    }
}
pub(super) async fn launch_without_cdp(
    d: &TauriBrowserDriver,
    p: &Profile,
) -> Result<LaunchedProfile> {
    let (resp, receive) = oneshot::channel();
    d.launcher_tx
        .send(LauncherCmd::Launch {
            profile_id: p.id.clone(),
            binary: d.browser_binary.clone(),
            engine: d.engine,
            companion: None,
            chromix: d.chromix.clone(),
            chromix_runtime: d.chromix_runtime.clone(),
            skip_download: true,
            resp,
        })
        .await
        .unwrap();
    receive.await.unwrap()
}

#[tokio::test]
async fn driver_crud_preserves_unbound_records_and_nulls() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "P").await;
    assert!(d.business_accounts_list().await.unwrap().is_empty());
    let a = d
        .business_accounts_save(input(&p, Kind::KuaishouShop))
        .await
        .unwrap();
    assert_eq!(
        d.business_accounts_profile_state(&p.id)
            .await
            .unwrap()
            .account,
        Some(a.clone())
    );
    d.business_accounts_unbind(&a.id).await.unwrap();
    let state = d.business_accounts_profile_state(&p.id).await.unwrap();
    assert_eq!(state.scope, Some(Scope::Kuaishou));
    assert!(state.account.is_none());
    assert!(d.business_accounts_list().await.unwrap()[0]
        .profile_id
        .is_none());
    d.delete_profile(&p.id).await.unwrap();
    assert_eq!(d.business_accounts_list().await.unwrap().len(), 1);
    assert!(d.business_accounts_profile_state(&p.id).await.is_err());
    d.shutdown().await;
}

#[tokio::test]
async fn live_launcher_not_stale_ui_cache_controls_business_writes() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let p = profile(&d, "P").await;
    let a = d
        .business_accounts_save(input(&p, Kind::Jinniu))
        .await
        .unwrap();
    launch_without_cdp(&d, &p).await.unwrap();
    assert!(
        !d.is_running(&p.id),
        "test deliberately leaves Tauri cached state empty"
    );
    let mut edit = input(&p, Kind::Jinniu);
    edit.id = Some(a.id.clone());
    edit.display_name = "edited".into();
    assert!(d
        .business_accounts_save(edit.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("先关闭"));
    assert!(d.business_accounts_unbind(&a.id).await.is_err());
    assert!(d.delete_profile(&p.id).await.is_err());
    // Same options and non-directory options remain editable; directory changes are atomic errors.
    d.update_profile(
        &p.id,
        UpdateProfileInput {
            chromix_options: Some(p.chromix_options.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let changed = json!({"unknown":{"keep":false}})
        .as_object()
        .unwrap()
        .clone();
    d.update_profile(
        &p.id,
        UpdateProfileInput {
            chromix_options: Some(changed.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let before = d.get_profile(&p.id).await.unwrap().unwrap();
    let options = json!({"userDataDir":std::path::Path::new(&p.data_dir).join("new")})
        .as_object()
        .unwrap()
        .clone();
    assert!(d
        .update_profile(
            &p.id,
            UpdateProfileInput {
                chromix_options: Some(options),
                ..Default::default()
            }
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("正在运行"));
    assert_eq!(
        d.get_profile(&p.id).await.unwrap().unwrap().chromix_options,
        before.chromix_options
    );
    d.close(&p.id).await.unwrap();
    assert_eq!(
        d.business_accounts_save(edit).await.unwrap().display_name,
        "edited"
    );
    d.business_accounts_unbind(&a.id).await.unwrap();
    launch_without_cdp(&d, &p).await.unwrap();
    assert!(d
        .business_accounts_save(input(&p, Kind::Jinniu))
        .await
        .is_err());
    d.close(&p.id).await.unwrap();
    d.shutdown().await;
}

#[tokio::test]
async fn active_unscoped_directory_snapshot_cannot_be_hidden_by_options_edit() {
    let (_dir, d) = fixture(ChromixSettings::default());
    let other = profile(&d, "unscoped").await;
    launch_without_cdp(&d, &other).await.unwrap();
    let old = std::path::Path::new(&other.data_dir).join("engines/chromix");
    d.update_profile(
        &other.id,
        UpdateProfileInput {
            chromix_options: Some(
                json!({"userDataDir":std::path::Path::new(&other.data_dir).join("next")})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let j = profile(&d, "J").await;
    d.update_profile(
        &j.id,
        UpdateProfileInput {
            chromix_options: Some(json!({"userDataDir":old}).as_object().unwrap().clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(d
        .business_accounts_save(input(&j, Kind::Jinniu))
        .await
        .unwrap_err()
        .to_string()
        .contains("重叠"));
    assert!(d.business_accounts_list().await.unwrap().is_empty());
    assert!(d
        .business_accounts_profile_state(&j.id)
        .await
        .unwrap()
        .scope
        .is_none());
    d.close(&other.id).await.unwrap();
    d.business_accounts_save(input(&j, Kind::Jinniu))
        .await
        .unwrap();
    d.shutdown().await;
}

#[tokio::test]
async fn shared_launch_preflight_blocks_both_directions_before_any_spawn_or_mark_opened() {
    let (dir, mut d) = fixture(ChromixSettings::default());
    let j = profile(&d, "J").await;
    d.business_accounts_save(input(&j, Kind::Jinniu))
        .await
        .unwrap();
    // Create/import are allowed to store options; ALL launches still cross the common gate.
    let other = d
        .create_profile(CreateProfileInput {
            name: "unscoped".into(),
            chromix_options: Some(
                json!({"userDataDir":std::path::Path::new(&j.data_dir).join("engines/chromix")})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            ..Default::default()
        })
        .await
        .unwrap();
    let marker = dir.path().join("must-not-start");
    d.chromix
        .environment
        .insert("CAPTURE_FILE".into(), marker.display().to_string());
    for p in [&j, &other] {
        assert!(d
            .launch(&p.id)
            .await
            .unwrap_err()
            .to_string()
            .contains("重叠"));
        assert!(d
            .get_profile(&p.id)
            .await
            .unwrap()
            .unwrap()
            .last_opened_at
            .is_none());
        assert!(d.registry.get(&p.id).await.is_none());
    }
    assert!(!marker.exists());
    let unrelated = profile(&d, "unrelated").await;
    launch_without_cdp(&d, &unrelated).await.unwrap();
    assert!(marker.exists());
    d.close(&unrelated.id).await.unwrap();
    d.shutdown().await;
}

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
        "business_accounts_list",
        "business_accounts_profile_state",
        "business_accounts_save",
        "business_accounts_unbind",
    ] {
        assert!(
            handler
                .lines()
                .any(|line| line.trim() == format!("{command},")),
            "{command}"
        );
    }
    let adapter = include_str!("../commands/business_accounts.rs");
    assert!(adapter.contains("profile_id: String"));
    assert_eq!(adapter.matches("#[tauri::command]").count(), 4);
}
