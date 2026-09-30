use browser_launcher::BrowserLauncher;
use multizen_core::{
    BrowserEngine as Engine, BusinessAccountKind as Kind, BusinessProfileScope as Scope,
    ChromixSettings, CreateProfileInput, Profile, SaveBusinessAccountInput, UpdateProfileInput,
};
use profile_manager::ProfileManager;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    pm: Arc<ProfileManager>,
    launcher: BrowserLauncher,
    global: ChromixSettings,
}
impl Fixture {
    fn new() -> Self {
        let d = TempDir::new().unwrap();
        #[allow(clippy::arc_with_non_send_sync)]
        let pm = Arc::new(
            ProfileManager::new(&d.path().join("p.db"), &d.path().join("profiles")).unwrap(),
        );
        Self {
            launcher: BrowserLauncher::new(pm.clone()),
            pm,
            _dir: d,
            global: ChromixSettings::default(),
        }
    }
    fn profile(&self, name: &str) -> Profile {
        self.pm
            .create(CreateProfileInput {
                name: name.into(),
                ..Default::default()
            })
            .unwrap()
    }
    fn reserve(&self, p: &Profile) {
        self.pm
            .business_accounts_save(SaveBusinessAccountInput {
                id: None,
                profile_id: p.id.clone(),
                kind: Kind::Jinniu,
                display_name: "J".into(),
                platform_user_id: None,
            })
            .unwrap();
    }
    fn override_dir(&self, p: &Profile, path: &std::path::Path) -> Profile {
        self.pm
            .update(
                &p.id,
                UpdateProfileInput {
                    chromix_options: Some(json!({"userDataDir":path}).as_object().unwrap().clone()),
                    ..Default::default()
                },
            )
            .unwrap()
    }
}

#[tokio::test]
async fn conflict_is_bidirectional_even_against_unscoped_profiles() {
    let f = Fixture::new();
    let j = f.profile("jinniu");
    let other = f.profile("unscoped");
    f.reserve(&j);
    let other = f.override_dir(
        &other,
        &std::path::Path::new(&j.data_dir).join("engines/chromix/subdir"),
    );
    for p in [&j, &other] {
        assert!(f
            .launcher
            .validate_business_directory(p, Engine::Chromix, &f.global, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("重叠"));
    }
    let unrelated = f.profile("unrelated");
    f.launcher
        .validate_business_directory(&unrelated, Engine::Chromix, &f.global, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn global_override_is_checked_with_actual_per_profile_shallow_merge() {
    let mut f = Fixture::new();
    let j = f.profile("jinniu");
    let other = f.profile("other");
    f.reserve(&j);
    f.global.options = json!({"userDataDir":f._dir.path().join("shared"),"future":false})
        .as_object()
        .unwrap()
        .clone();
    assert!(f
        .launcher
        .validate_business_directory(&j, Engine::Chromix, &f.global, None)
        .await
        .is_err());
    let j = f.override_dir(&j, &f._dir.path().join("separate"));
    f.launcher
        .validate_business_directory(&j, Engine::Chromix, &f.global, None)
        .await
        .unwrap();
    f.launcher
        .validate_business_directory(&other, Engine::Chromix, &f.global, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn binding_conflict_and_patch_conflict_leave_database_unchanged() {
    let f = Fixture::new();
    let j = f.profile("j");
    let other = f.profile("other");
    let j = f.override_dir(
        &j,
        &std::path::Path::new(&other.data_dir).join("engines/chromix"),
    );
    let input = SaveBusinessAccountInput {
        id: None,
        profile_id: j.id.clone(),
        kind: Kind::Jinniu,
        display_name: "J".into(),
        platform_user_id: None,
    };
    assert!(f
        .launcher
        .save_business_account(input, Engine::Chromix, &f.global)
        .await
        .is_err());
    assert!(f.pm.business_profile_scope(&j.id).unwrap().is_none());
    assert!(f.pm.business_accounts_list().unwrap().is_empty());
    let j = f.override_dir(&j, &f._dir.path().join("independent"));
    f.reserve(&j);
    let before = serde_json::to_value(f.pm.get(&other.id).unwrap()).unwrap();
    let patch = UpdateProfileInput {
        chromix_options: Some(j.chromix_options.clone()),
        ..Default::default()
    };
    assert!(f
        .launcher
        .update_profile_guarded(&other.id, patch, Engine::Chromix, &f.global)
        .await
        .is_err());
    assert_eq!(
        serde_json::to_value(f.pm.get(&other.id).unwrap()).unwrap(),
        before
    );
}

#[tokio::test]
async fn ordinary_profiles_keep_path_behavior_without_any_jinniu_scope() {
    let f = Fixture::new();
    let p = f.profile("ordinary");
    let q = f.profile("other");
    let p = f.override_dir(
        &p,
        &std::path::Path::new(&q.data_dir).join("engines/chromix"),
    );
    let input = SaveBusinessAccountInput {
        id: None,
        profile_id: p.id.clone(),
        kind: Kind::KuaishouShop,
        display_name: "K".into(),
        platform_user_id: None,
    };
    f.launcher
        .save_business_account(input, Engine::Chromix, &f.global)
        .await
        .unwrap();
    f.launcher
        .validate_business_directory(&p, Engine::Chromix, &f.global, None)
        .await
        .unwrap();
    // No Jinniu policy means no new filesystem validation of ordinary configurations.
    // The existing bridge remains responsible for rejecting unsupported launch options.
    let patch = UpdateProfileInput {
        chromix_options: Some(json!({"userDataDir":null}).as_object().unwrap().clone()),
        ..Default::default()
    };
    let p = f
        .launcher
        .update_profile_guarded(&p.id, patch, Engine::Chromix, &f.global)
        .await
        .unwrap();
    f.launcher
        .validate_business_directory(&p, Engine::Chromix, &f.global, None)
        .await
        .unwrap();
    let j = f.profile("jinniu");
    assert!(f
        .launcher
        .validate_business_directory(&j, Engine::Chromix, &f.global, Some(Scope::Jinniu))
        .await
        .is_err());
}

#[tokio::test]
async fn unbound_reservation_still_protects_directory_and_legacy_engine_roots() {
    let f = Fixture::new();
    let j = f.profile("j");
    let mut other = f.profile("other");
    f.reserve(&j);
    let a = f.pm.business_accounts_list().unwrap().pop().unwrap();
    f.launcher.unbind_business_account(&a.id).await.unwrap();
    assert_eq!(
        f.pm.business_profile_scope(&j.id).unwrap(),
        Some(Scope::Jinniu)
    );
    // The guard accepts a patch candidate, not only DB-loaded objects.
    other.data_dir = j.data_dir.clone();
    for engine in [Engine::Cft, Engine::Cloakbrowser, Engine::Chromix] {
        assert!(f
            .launcher
            .validate_business_directory(&other, engine, &f.global, None)
            .await
            .is_err());
    }
}
