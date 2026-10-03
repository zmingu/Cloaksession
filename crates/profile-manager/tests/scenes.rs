use profile_manager::scenes::{
    RecordInteractionInput, SceneLineAction, TriggerMode,
};
use profile_manager::ProfileManager;
use tempfile::TempDir;

fn open() -> (TempDir, ProfileManager) {
    let dir = TempDir::new().unwrap();
    let pm = ProfileManager::new(&dir.path().join("t.db"), &dir.path().join("profiles")).unwrap();
    (dir, pm)
}

#[test]
fn migration_creates_scene_tables_and_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    let root = dir.path().join("profiles");
    // Create then reopen the same file: migrations must be idempotent and
    // the scene tables must exist on both passes.
    for _ in 0..2 {
        let pm = ProfileManager::new(&db, &root).unwrap();
        let scene = pm
            .scene_create("t", TriggerMode::RelativeTime, None)
            .unwrap();
        pm.scene_add_line(scene.id, "hi", 1, SceneLineAction::Danmaku)
            .unwrap();
        pm.record_interaction(&RecordInteractionInput {
            account_id: "a".into(),
            scene_id: Some(scene.id),
            action: SceneLineAction::Like,
            message: None,
            live_room_url: None,
            ok: true,
            error: None,
            duration_ms: None,
        })
        .unwrap();
        pm.scene_delete(scene.id).unwrap();
    }
}

#[test]
fn scene_crud_roundtrip() {
    let (_dir, pm) = open();
    let scene = pm
        .scene_create("早场暖场", TriggerMode::RelativeTime, Some("g1".into()))
        .unwrap();
    assert_eq!(scene.name, "早场暖场");
    assert_eq!(scene.trigger_mode, TriggerMode::RelativeTime);
    assert_eq!(scene.group_id.as_deref(), Some("g1"));
    assert!(scene.lines.is_empty());

    let l1 = pm
        .scene_add_line(scene.id, "欢迎来到直播间", 5, SceneLineAction::Danmaku)
        .unwrap();
    assert_eq!(l1.ord, 0);
    let l2 = pm
        .scene_add_line(scene.id, "", 10, SceneLineAction::Like)
        .unwrap();
    assert_eq!(l2.ord, 1);

    let got = pm.scene_get(scene.id).unwrap().unwrap();
    assert_eq!(got.lines.len(), 2);
    assert_eq!(got.lines[0].message, "欢迎来到直播间");

    // list carries lines without N+1-per-scene failure
    let all = pm.scene_list().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].lines.len(), 2);

    // update line message + offset
    let edited = pm
        .scene_update_line(l1.id, Some("欢迎宝宝们".into()), Some(7), None)
        .unwrap();
    assert_eq!(edited.message, "欢迎宝宝们");
    assert_eq!(edited.time_offset_sec, 7);

    // rename scene, change mode, clear group
    let renamed = pm
        .scene_update(
            scene.id,
            Some("晚场".into()),
            Some(TriggerMode::LocalTime),
            Some(None),
        )
        .unwrap();
    assert_eq!(renamed.name, "晚场");
    assert_eq!(renamed.trigger_mode, TriggerMode::LocalTime);
    assert!(renamed.group_id.is_none());

    // local-time offset guard
    assert!(pm
        .scene_add_line(scene.id, "x", 90_000, SceneLineAction::Danmaku)
        .is_err());

    // reorder
    let reordered = pm.scene_reorder_lines(scene.id, &[l2.id, l1.id]).unwrap();
    assert_eq!(reordered[0].id, l2.id);
    assert_eq!(reordered[0].ord, 0);
    assert_eq!(reordered[1].ord, 1);

    // delete one line, then the scene (lines cascade)
    pm.scene_delete_line(l2.id).unwrap();
    assert_eq!(pm.scene_get(scene.id).unwrap().unwrap().lines.len(), 1);
    pm.scene_delete(scene.id).unwrap();
    assert!(pm.scene_get(scene.id).unwrap().is_none());
    assert!(pm.scene_list().unwrap().is_empty());
    // absent delete succeeds
    pm.scene_delete(scene.id).unwrap();
}

#[test]
fn scene_validation_rejects_bad_input() {
    let (_dir, pm) = open();
    assert!(pm.scene_create("   ", TriggerMode::RelativeTime, None).is_err());
    assert!(pm
        .scene_create(&"x".repeat(101), TriggerMode::RelativeTime, None)
        .is_err());
    assert!(pm
        .scene_update(999, Some("nope".into()), None, None)
        .is_err());
    let scene = pm.scene_create("s", TriggerMode::RelativeTime, None).unwrap();
    assert!(pm.scene_add_line(999, "m", 1, SceneLineAction::Danmaku).is_err());
    // danmaku requires a message; like/follow do not
    assert!(pm
        .scene_add_line(scene.id, "   ", 1, SceneLineAction::Danmaku)
        .is_err());
    assert!(pm
        .scene_add_line(scene.id, "", 1, SceneLineAction::Follow)
        .is_ok());
    assert!(pm.scene_add_line(scene.id, "m", -1, SceneLineAction::Danmaku).is_err());
    // reorder must match exactly
    let lines = pm.scene_get(scene.id).unwrap().unwrap().lines;
    assert!(pm.scene_reorder_lines(scene.id, &[]).is_err());
    assert!(pm
        .scene_reorder_lines(scene.id, &[lines[0].id, lines[0].id])
        .is_err());
    assert!(pm.scene_reorder_lines(scene.id, &[lines[0].id + 10_000]).is_err());
    assert!(pm.scene_reorder_lines(999, &[]).is_err());
    // switching a like line to danmaku without a message fails
    let like = &lines[0];
    assert!(pm
        .scene_update_line(like.id, None, None, Some(SceneLineAction::Danmaku))
        .is_err());
}

#[test]
fn interactions_record_and_list() {
    let (_dir, pm) = open();
    let scene = pm.scene_create("s", TriggerMode::RelativeTime, None).unwrap();
    let r = pm
        .record_interaction(&RecordInteractionInput {
            account_id: "acc-1".into(),
            scene_id: Some(scene.id),
            action: SceneLineAction::Danmaku,
            message: Some("hi".into()),
            live_room_url: Some("https://live.kuaishou.com/u/1".into()),
            ok: true,
            error: None,
            duration_ms: Some(120),
        })
        .unwrap();
    assert!(r.ok);
    pm.record_interaction(&RecordInteractionInput {
        account_id: "acc-1".into(),
        scene_id: Some(scene.id),
        action: SceneLineAction::Follow,
        message: None,
        live_room_url: None,
        ok: false,
        error: Some("boom".into()),
        duration_ms: None,
    })
    .unwrap();
    let all = pm.list_interactions(Some(scene.id), None, 100, 0).unwrap();
    assert_eq!(all.len(), 2);
    // newest first
    assert_eq!(all[0].action, SceneLineAction::Follow);
    assert!(!all[0].ok);
    let by_account = pm
        .list_interactions(None, Some("acc-1"), 100, 0)
        .unwrap();
    assert_eq!(by_account.len(), 2);
    assert!(pm
        .list_interactions(None, Some("nobody"), 100, 0)
        .unwrap()
        .is_empty());
    // scene delete keeps interaction history with NULL scene
    pm.scene_delete(scene.id).unwrap();
    let kept = pm.list_interactions(None, Some("acc-1"), 100, 0).unwrap();
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|i| i.scene_id.is_none()));
    // empty account id rejected
    assert!(pm
        .record_interaction(&RecordInteractionInput {
            account_id: "  ".into(),
            scene_id: None,
            action: SceneLineAction::Like,
            message: None,
            live_room_url: None,
            ok: true,
            error: None,
            duration_ms: None,
        })
        .is_err());
}
