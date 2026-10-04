use multizen_core::{
    BusinessAccountKind, BusinessProfileState, CreateProfileInput,
    KuaishouIdentityObservation as Observation, KuaishouIdentitySnapshot as Snapshot,
    KuaishouIdentityStatus as Status, SaveBusinessAccountInput,
};
use profile_manager::ProfileManager;
use tempfile::TempDir;
fn fixture() -> (TempDir, ProfileManager, String) {
    let dir = TempDir::new().unwrap();
    let pm = ProfileManager::new(&dir.path().join("p.db"), &dir.path().join("profiles")).unwrap();
    let p = pm
        .create(CreateProfileInput {
            name: "P".into(),
            ..Default::default()
        })
        .unwrap();
    (dir, pm, p.id)
}
fn observation(id: &str, platform: &str) -> Observation {
    let mut s = Snapshot::empty(id, Status::Detected);
    s.platform_user_id = Some(platform.into());
    s.nickname = Some("Name".into());
    s.avatar_key = Some(format!("{}.gif", "a".repeat(64)));
    s.checked_at = Some("2026-09-30T00:00:00Z".into());
    Observation {
        snapshot: s,
        session_id: Some("generation".into()),
        avatar_url: Some("https://yximgs.com/avatar.gif".into()),
    }
}
#[test]
fn persistence_roundtrip_and_archive_survives_profile_delete() {
    let (dir, pm, id) = fixture();
    let state = pm.business_accounts_profile_state(&id).unwrap();
    let saved = pm
        .kuaishou_identity_save(observation(&id, "12345"), &state)
        .unwrap()
        .unwrap();
    assert!(saved.snapshot.last_seen_at.is_some());
    drop(pm);
    let pm = ProfileManager::new(&dir.path().join("p.db"), &dir.path().join("profiles")).unwrap();
    assert_eq!(
        pm.kuaishou_identity_observation(&id).unwrap().unwrap(),
        saved
    );
    pm.delete(&id).unwrap();
    assert!(pm.kuaishou_identity_observations().unwrap().is_empty());
    assert!(pm
        .kuaishou_identity_avatar_referenced(saved.snapshot.avatar_key.as_ref().unwrap())
        .unwrap());
    assert!(pm
        .kuaishou_identity_save(observation(&id, "12345"), &state)
        .unwrap()
        .is_none());
}
#[test]
fn no_id_and_errors_preserve_history_but_new_id_has_no_old_avatar() {
    let (_dir, pm, id) = fixture();
    let state = pm.business_accounts_profile_state(&id).unwrap();
    pm.kuaishou_identity_save(observation(&id, "12345"), &state)
        .unwrap();
    let mut next = observation(&id, "99999");
    next.snapshot.status = Status::NotDetected;
    let saved = pm.kuaishou_identity_save(next, &state).unwrap().unwrap();
    assert_eq!(saved.snapshot.status, Status::NotDetected);
    assert_eq!(saved.snapshot.platform_user_id.as_deref(), Some("12345"));
    assert!(saved.snapshot.avatar_key.is_some());
    let mut next = observation(&id, "99999");
    next.snapshot.avatar_key = None;
    let saved = pm.kuaishou_identity_save(next, &state).unwrap().unwrap();
    assert_eq!(saved.snapshot.platform_user_id.as_deref(), Some("99999"));
    assert!(saved.snapshot.avatar_key.is_none());
    assert!(saved.avatar_url.is_none());
}
#[test]
fn manual_mismatch_and_unbind_reject_stale_writes() {
    let (_dir, pm, id) = fixture();
    let a = pm
        .business_accounts_save(SaveBusinessAccountInput {
            id: None,
            profile_id: id.clone(),
            kind: BusinessAccountKind::KuaishouShop,
            display_name: "Manual".into(),
            platform_user_id: Some("54321".into()),
        })
        .unwrap();
    let state = pm.business_accounts_profile_state(&id).unwrap();
    let saved = pm
        .kuaishou_identity_save(observation(&id, "12345"), &state)
        .unwrap()
        .unwrap();
    assert_eq!(saved.snapshot.status, Status::Conflict);
    assert_eq!(pm.business_account_get(&a.id).unwrap().unwrap(), a);
    pm.business_accounts_unbind(&a.id).unwrap();
    assert!(pm
        .kuaishou_identity_save(observation(&id, "12345"), &state)
        .unwrap()
        .is_none());
}
#[test]
fn jinniu_scope_and_nonshop_are_skipped() {
    for kind in [
        BusinessAccountKind::Jinniu,
        BusinessAccountKind::KuaishouLive,
    ] {
        let (_dir, pm, id) = fixture();
        pm.business_accounts_save(SaveBusinessAccountInput {
            id: None,
            profile_id: id.clone(),
            kind,
            display_name: "Manual".into(),
            platform_user_id: None,
        })
        .unwrap();
        let state = pm.business_accounts_profile_state(&id).unwrap();
        assert_eq!(
            pm.kuaishou_identity_save(observation(&id, "12345"), &state)
                .unwrap()
                .unwrap()
                .snapshot
                .status,
            Status::Skipped
        );
    }
}
#[test]
fn original_schema_upgrade_and_repeated_migration() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,notes TEXT,tags TEXT NOT NULL DEFAULT '[]',proxy TEXT,fingerprint TEXT NOT NULL,data_dir TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,last_opened_at TEXT);").unwrap();
    profile_manager::migrate::run_migrations(&conn).unwrap();
    profile_manager::migrate::run_migrations(&conn).unwrap();
    for name in [
        "kuaishou_identities",
        "kuaishou_identity_observations",
        "business_accounts",
    ] {
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name=?",
                [name],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
#[test]
fn malformed_success_is_rejected() {
    let (_dir, pm, id) = fixture();
    let state = BusinessProfileState {
        account: None,
        scope: None,
    };
    // Unbound profiles accept the main-site alphanumeric id form, so use a value
    // that is invalid under BOTH the numeric shop rule and the viewer rule.
    assert!(pm
        .kuaishou_identity_save(observation(&id, "12 34"), &state)
        .is_err());
    let mut o = observation(&id, "12345");
    o.session_id = None;
    assert!(pm.kuaishou_identity_save(o, &state).is_err());
    assert!(pm.kuaishou_identity_observations().unwrap().is_empty());
}
#[test]
fn viewer_sub_binding_accepts_main_site_id_while_shop_stays_numeric() {
    let (_dir, pm, id) = fixture();
    pm.business_accounts_save(SaveBusinessAccountInput {
        id: None,
        profile_id: id.clone(),
        kind: BusinessAccountKind::KuaishouSub,
        display_name: "Sub".into(),
        platform_user_id: None,
    })
    .unwrap();
    let state = pm.business_accounts_profile_state(&id).unwrap();
    let saved = pm
        .kuaishou_identity_save(observation(&id, "3x7abcdef"), &state)
        .unwrap()
        .unwrap();
    assert_eq!(saved.snapshot.status, Status::Detected);
    assert_eq!(saved.snapshot.platform_user_id.as_deref(), Some("3x7abcdef"));

    let (_dir2, pm2, id2) = fixture();
    pm2.business_accounts_save(SaveBusinessAccountInput {
        id: None,
        profile_id: id2.clone(),
        kind: BusinessAccountKind::KuaishouShop,
        display_name: "Shop".into(),
        platform_user_id: None,
    })
    .unwrap();
    let shop_state = pm2.business_accounts_profile_state(&id2).unwrap();
    assert!(pm2
        .kuaishou_identity_save(observation(&id2, "3x7abcdef"), &shop_state)
        .is_err());
}
