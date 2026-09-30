use multizen_core::{
    BusinessAccountKind as Kind, BusinessProfileScope as Scope, CreateProfileInput,
    SaveBusinessAccountInput,
};
use profile_manager::ProfileManager;
use tempfile::TempDir;

fn manager(dir: &TempDir) -> ProfileManager {
    ProfileManager::new(
        &dir.path().join("profiles.db"),
        &dir.path().join("profiles"),
    )
    .unwrap()
}
fn profile(pm: &ProfileManager) -> String {
    pm.create(CreateProfileInput {
        name: "test".into(),
        ..Default::default()
    })
    .unwrap()
    .id
}
fn input(profile_id: &str, kind: Kind, platform: Option<&str>) -> SaveBusinessAccountInput {
    SaveBusinessAccountInput {
        id: None,
        profile_id: profile_id.into(),
        kind,
        display_name: "  手工别名  ".into(),
        platform_user_id: platform.map(Into::into),
    }
}

#[test]
fn new_database_reopen_unbind_retains_cookie_root_and_scope() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    assert!(pm.business_accounts_list().unwrap().is_empty());
    let state = pm.business_accounts_profile_state(&p).unwrap();
    assert!(state.account.is_none() && state.scope.is_none());
    let cookie = std::path::Path::new(&pm.get(&p).unwrap().unwrap().data_dir).join("cookie-marker");
    std::fs::write(&cookie, "keep").unwrap();
    let a = pm
        .business_accounts_save(input(&p, Kind::Jinniu, Some("  123  ")))
        .unwrap();
    assert_eq!(a.display_name, "手工别名");
    assert_eq!(a.platform_user_id.as_deref(), Some("123"));
    chrono::DateTime::parse_from_rfc3339(&a.created_at).unwrap();
    drop(pm);
    let pm = manager(&dir);
    assert_eq!(pm.business_accounts_list().unwrap(), vec![a.clone()]);
    pm.business_accounts_unbind(&a.id).unwrap();
    pm.business_accounts_unbind(&a.id).unwrap();
    let state = pm.business_accounts_profile_state(&p).unwrap();
    assert_eq!(state.scope, Some(Scope::Jinniu));
    assert!(state.account.is_none());
    assert_eq!(std::fs::read_to_string(&cookie).unwrap(), "keep");
    assert!(pm.business_accounts_list().unwrap()[0].profile_id.is_none());
    assert!(pm
        .business_accounts_save(input(&p, Kind::KuaishouShop, None))
        .unwrap_err()
        .to_string()
        .contains("scope"));
    let mut bind = input(&p, Kind::Jinniu, Some("123"));
    bind.id = Some(a.id.clone());
    assert_eq!(
        pm.business_accounts_save(bind).unwrap().created_at,
        a.created_at
    );
}

#[test]
fn unique_binding_id_kind_and_platform_errors_are_atomic() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    let q = profile(&pm);
    let a = pm
        .business_accounts_save(input(&p, Kind::KuaishouShop, Some("id")))
        .unwrap();
    let original = pm.business_accounts_list().unwrap();
    assert!(pm
        .business_accounts_save(input(&p, Kind::KuaishouLive, None))
        .is_err());
    assert!(pm
        .business_accounts_save(input(&q, Kind::KuaishouShop, Some("id")))
        .unwrap_err()
        .to_string()
        .contains("已登记"));
    assert!(pm.business_profile_scope(&q).unwrap().is_none());
    let mut moved = input(&q, Kind::KuaishouShop, Some("id"));
    moved.id = Some(a.id.clone());
    assert!(pm
        .business_accounts_save(moved.clone())
        .unwrap_err()
        .to_string()
        .contains("显式解绑"));
    moved.profile_id = p.clone();
    moved.kind = Kind::KuaishouLive;
    assert!(pm
        .business_accounts_save(moved)
        .unwrap_err()
        .to_string()
        .contains("类型不能修改"));
    assert_eq!(pm.business_accounts_list().unwrap(), original);
    // Different kinds never merge, even with an equal platform identifier.
    let b = pm
        .business_accounts_save(input(&q, Kind::KuaishouLive, Some("id")))
        .unwrap();
    assert_ne!(a.id, b.id);
    pm.business_accounts_unbind(&a.id).unwrap();
    let r = profile(&pm);
    assert!(pm
        .business_accounts_save(input(&r, Kind::KuaishouShop, Some("id")))
        .is_err());
    let mut rebind = input(&r, Kind::KuaishouShop, Some("id"));
    rebind.id = Some(a.id);
    assert_eq!(
        pm.business_accounts_save(rebind).unwrap().profile_id,
        Some(r)
    );
}

#[test]
fn invalid_input_and_nonexistent_profile_leave_no_rows() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    for name in [" ".into(), "x".repeat(101), "a\nb".into()] {
        let mut v = input(&p, Kind::Jinniu, None);
        v.display_name = name;
        assert!(pm.business_accounts_save(v).is_err());
    }
    let mut v = input(&p, Kind::Jinniu, Some(&"x".repeat(129)));
    assert!(pm.business_accounts_save(v.clone()).is_err());
    v.platform_user_id = None;
    v.id = Some("missing".into());
    assert!(pm.business_accounts_save(v).is_err());
    assert!(pm
        .business_accounts_save(input("missing", Kind::Jinniu, None))
        .is_err());
    assert!(pm.business_accounts_profile_state("missing").is_err());
    assert!(pm.business_accounts_unbind("missing").is_err());
    for id in ["", " \t ", "missing"] {
        let mut v = input(&p, Kind::Jinniu, None);
        v.id = Some(id.into());
        assert!(
            pm.business_accounts_save(v).is_err(),
            "an explicit id must never create a new record"
        );
    }
    assert!(pm.business_accounts_list().unwrap().is_empty());
    assert!(pm.business_profile_scope(&p).unwrap().is_none());
    let v = input(&p, Kind::Jinniu, Some(" \t "));
    assert!(pm
        .business_accounts_save(v)
        .unwrap()
        .platform_user_id
        .is_none());
}

#[test]
fn delete_profile_retains_account_and_cascades_scope() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    let a = pm
        .business_accounts_save(input(&p, Kind::Jinniu, None))
        .unwrap();
    pm.delete(&p).unwrap();
    assert!(pm.business_profile_scope(&p).unwrap().is_none());
    let saved = pm.business_account_get(&a.id).unwrap().unwrap();
    assert!(saved.profile_id.is_none());
    assert_eq!(saved.created_at, a.created_at);
    assert_eq!(pm.business_accounts_list().unwrap().len(), 1);
    drop(pm);
    assert_eq!(manager(&dir).business_accounts_list().unwrap()[0], saved);
}

#[test]
fn sql_failure_after_reservation_insert_rolls_back_everything() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    let conn = rusqlite::Connection::open(dir.path().join("profiles.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_account BEFORE INSERT ON business_accounts BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(pm
        .business_accounts_save(input(&p, Kind::Jinniu, None))
        .is_err());
    assert!(pm.business_accounts_list().unwrap().is_empty());
    assert!(pm.business_profile_scope(&p).unwrap().is_none());
    conn.execute_batch("DROP TRIGGER reject_account;").unwrap();
    let a = pm
        .business_accounts_save(input(&p, Kind::Jinniu, None))
        .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_update BEFORE UPDATE ON business_accounts BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(pm.business_accounts_unbind(&a.id).is_err());
    assert_eq!(pm.business_account_get(&a.id).unwrap(), Some(a));
}

#[test]
fn updates_clear_platform_id_without_replacing_record_and_preserve_created_at() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    let a = pm
        .business_accounts_save(input(&p, Kind::KuaishouShop, Some("old-id")))
        .unwrap();
    // Actual IPC-shaped JSON, not nested Rust Option patch semantics.
    let edit: SaveBusinessAccountInput = serde_json::from_value(serde_json::json!({
        "id": a.id, "profileId": p, "kind": "kuaishou-shop",
        "displayName": "  新别名  ", "platformUserId": null
    }))
    .unwrap();
    let saved = pm.business_accounts_save(edit.clone()).unwrap();
    assert_eq!(saved.id, a.id);
    assert_eq!(saved.created_at, a.created_at);
    assert_eq!(saved.display_name, "新别名");
    assert_eq!(saved.platform_user_id, None);
    let mut edit = edit;
    edit.platform_user_id = Some("new-id".into());
    pm.business_accounts_save(edit.clone()).unwrap();
    edit.platform_user_id = Some(" \t ".into());
    assert!(pm
        .business_accounts_save(edit)
        .unwrap()
        .platform_user_id
        .is_none());
    let q = profile(&pm);
    pm.business_accounts_save(input(&q, Kind::KuaishouShop, Some("old-id")))
        .unwrap();
    let r = profile(&pm);
    pm.business_accounts_save(input(&r, Kind::KuaishouShop, None))
        .unwrap();
    drop(pm);
    let pm = manager(&dir);
    assert_eq!(pm.business_accounts_list().unwrap().len(), 3);
    assert_eq!(
        pm.business_account_get(&a.id)
            .unwrap()
            .unwrap()
            .platform_user_id,
        None
    );
}

#[test]
fn failed_rebind_update_rolls_back_new_scope_and_preserves_orphan() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let p = profile(&pm);
    let q = profile(&pm);
    let a = pm
        .business_accounts_save(input(&p, Kind::Jinniu, Some("id")))
        .unwrap();
    pm.business_accounts_unbind(&a.id).unwrap();
    let before = pm.business_account_get(&a.id).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("profiles.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_rebind BEFORE UPDATE ON business_accounts BEGIN SELECT RAISE(ABORT,'fixture rebind failure'); END;").unwrap();
    let mut rebind = input(&q, Kind::Jinniu, Some("id"));
    rebind.id = Some(a.id.clone());
    assert!(pm.business_accounts_save(rebind).is_err());
    assert!(pm.business_profile_scope(&q).unwrap().is_none());
    assert_eq!(pm.business_profile_scope(&p).unwrap(), Some(Scope::Jinniu));
    assert_eq!(pm.business_account_get(&a.id).unwrap(), before);
}

#[test]
fn original_schema_migrates_idempotently_and_has_foreign_keys_and_uniques() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,notes TEXT,tags TEXT NOT NULL DEFAULT '[]',proxy TEXT,fingerprint TEXT NOT NULL,data_dir TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,last_opened_at TEXT);
    INSERT INTO profiles(id,name,fingerprint,data_dir,created_at,updated_at) VALUES ('old','old','{}','unused','t','t');").unwrap();
    for _ in 0..3 {
        profile_manager::migrate::run_migrations(&conn).unwrap();
    }
    assert_eq!(
        conn.query_row("SELECT count(*) FROM profiles", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    conn.execute_batch("INSERT INTO business_accounts VALUES('a','jinniu','A','id','old','t','t'); INSERT INTO business_profile_scopes VALUES('old','jinniu');").unwrap();
    assert!(conn
        .execute_batch("INSERT INTO business_accounts VALUES('b','jinniu','B','id',NULL,'t','t');")
        .is_err());
    assert!(conn
        .execute_batch(
            "INSERT INTO business_accounts VALUES('b','kuaishou-shop','B',NULL,'old','t','t');"
        )
        .is_err());
    assert!(conn
        .execute_batch("INSERT INTO business_profile_scopes VALUES('missing','jinniu');")
        .is_err());
    conn.execute_batch("DELETE FROM profiles WHERE id='old';")
        .unwrap();
    assert_eq!(
        conn.query_row("SELECT profile_id FROM business_accounts", [], |r| r
            .get::<_, Option<String>>(0))
            .unwrap(),
        None
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM business_profile_scopes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
