use profile_manager::live_events::{append_event, count_events, recent_events, LiveEventRow};
use profile_manager::migrate::run_migrations;
use rusqlite::Connection;
use std::path::PathBuf;

fn open_mem() -> Connection {
    Connection::open_in_memory().unwrap()
}

fn row(id: &str, kind: &str, account: &str, time: &str) -> LiveEventRow {
    LiveEventRow {
        msg_id: id.into(),
        event_type: kind.into(),
        user_id: Some("u1".into()),
        nickname: Some("nick".into()),
        content: "hello".into(),
        time: time.into(),
        account_id: account.into(),
    }
}

#[test]
fn migration_creates_live_events_table_and_index() {
    let conn = open_mem();
    run_migrations(&conn).unwrap();
    let table: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='live_events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(table, 1);
    let idx: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='idx_live_events_account_time'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx, 1);
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(live_events)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    for expected in [
        "msg_id",
        "type",
        "user_id",
        "nickname",
        "content",
        "time",
        "account_id",
    ] {
        assert!(cols.iter().any(|c| c == expected), "missing: {expected}");
    }
    // Idempotent: a second run must not error.
    run_migrations(&conn).unwrap();
}

#[test]
fn crud_append_recent_count_and_redelivery_ignore() {
    let dir = tempfile::tempdir().unwrap();
    let db: PathBuf = dir.path().join("profiles.db");
    append_event(&db, &row("m1", "Comment", "a", "2026-10-03T00:00:01Z")).unwrap();
    append_event(&db, &row("m2", "RoomLike", "a", "2026-10-03T00:00:02Z")).unwrap();
    // Same msg_id re-delivered: ignored, no duplicate, no error.
    append_event(&db, &row("m1", "Comment", "a", "2026-10-03T00:00:01Z")).unwrap();
    // Other account is isolated.
    append_event(&db, &row("m3", "Comment", "b", "2026-10-03T00:00:03Z")).unwrap();

    assert_eq!(count_events(&db, "a").unwrap(), 2);
    assert_eq!(count_events(&db, "b").unwrap(), 1);

    let recent = recent_events(&db, "a", 10).unwrap();
    assert_eq!(recent.len(), 2);
    // Newest first.
    assert_eq!(recent[0].msg_id, "m2");
    assert_eq!(recent[0].event_type, "RoomLike");
    assert_eq!(recent[1].msg_id, "m1");

    let limited = recent_events(&db, "a", 1).unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].msg_id, "m2");

    assert!(recent_events(&db, "nobody", 10).unwrap().is_empty());
}

#[test]
fn rejects_empty_msg_id_and_unknown_type() {
    let dir = tempfile::tempdir().unwrap();
    let db: PathBuf = dir.path().join("profiles.db");
    let mut bad = row("", "Comment", "a", "2026-10-03T00:00:01Z");
    assert!(append_event(&db, &bad).is_err());
    bad = row("m9", "Danmaku", "a", "2026-10-03T00:00:01Z");
    assert!(append_event(&db, &bad).is_err());
    assert_eq!(count_events(&db, "a").unwrap(), 0);
}
