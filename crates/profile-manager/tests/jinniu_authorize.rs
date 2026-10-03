use profile_manager::{JinniuAuthorizeItem, ProfileManager};
use tempfile::TempDir;

fn manager(dir: &TempDir) -> ProfileManager {
    ProfileManager::new(
        &dir.path().join("profiles.db"),
        &dir.path().join("profiles"),
    )
    .unwrap()
}

fn item(user_id: &str, user_name: &str) -> JinniuAuthorizeItem {
    JinniuAuthorizeItem {
        user_id: user_id.into(),
        user_name: user_name.into(),
        status: "已授权".into(),
        authorize_time: "2026-10-03".into(),
    }
}

#[test]
fn fresh_database_starts_empty_and_rejects_blank_key() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    assert!(pm.jinniu_authorize_get("j1").unwrap().is_empty());
    assert!(pm.jinniu_authorize_get("").is_err());
    assert!(pm
        .jinniu_authorize_upsert_batch("", &[item("u1", "n1")])
        .is_err());
}

#[test]
fn upsert_batch_inserts_updates_and_skips_blank_user_id() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    pm.jinniu_authorize_upsert_batch("j1", &[item("u1", "n1"), item("u2", "n2")])
        .unwrap();
    // Blank user_id rows are skipped; empty batch is a no-op success.
    pm.jinniu_authorize_upsert_batch(
        "j1",
        &[JinniuAuthorizeItem {
            user_id: "  ".into(),
            user_name: "skip".into(),
            status: String::new(),
            authorize_time: String::new(),
        }],
    )
    .unwrap();
    pm.jinniu_authorize_upsert_batch("j1", &[]).unwrap();
    let mut updated = item("u1", "n1-new");
    updated.status = "待确认".into();
    pm.jinniu_authorize_upsert_batch("j1", &[updated]).unwrap();

    let records = pm.jinniu_authorize_get("j1").unwrap();
    assert_eq!(records.len(), 2);
    let first = records.iter().find(|r| r.user_id == "u1").unwrap();
    assert_eq!(first.user_name, "n1-new");
    assert_eq!(first.status, "待确认");
    assert!(first.synced_at > 0);
}

#[test]
fn records_are_isolated_by_jinniu_id_and_survive_reopen() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    pm.jinniu_authorize_upsert_batch("j1", &[item("u1", "n1")])
        .unwrap();
    pm.jinniu_authorize_upsert_batch("j2", &[item("u9", "n9")])
        .unwrap();
    assert_eq!(pm.jinniu_authorize_get("j1").unwrap().len(), 1);
    assert_eq!(pm.jinniu_authorize_get("j2").unwrap().len(), 1);
    drop(pm);

    let pm = manager(&dir);
    let records = pm.jinniu_authorize_get("j1").unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].jinniu_id, "j1");
    assert_eq!(records[0].user_id, "u1");
}
