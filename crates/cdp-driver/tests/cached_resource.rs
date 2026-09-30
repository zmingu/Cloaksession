//! Browser-free transport coverage for the loaded-image fallback. Real DOM/network
//! behavior is verified separately by identity_browser.rs, not by this scripted peer.
mod common;
#[allow(dead_code)]
#[path = "../../tauri-app/src/driver/identity/extract.rs"]
mod reader;
use cdp_driver::{TaskCancel, TaskError};
use common::{EvalReply, Peer};
use serde_json::{json, Value};
use std::time::Duration;

const IMAGE: &str = "https://p4.yximgs.com/avatar.png";
fn probe() -> Value {
    json!({
        "page": {"url":"https://s.kwaixiaodian.com/zone/home", "platformUserId":"12345", "nickname":"N", "avatarUrl":IMAGE},
        "documentEpoch":"123456.789", "currentSrc":IMAGE,
        "complete":true, "width":1, "height":1, "content":"iVBORw0KGgo="
    })
}

#[tokio::test]
async fn loaded_avatar_is_original_target_only_and_retains_task_lock() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    peer.clear();
    peer.evaluations(vec![EvalReply::Value(probe()), EvalReply::Value(probe())]);
    let mut image = reader::AvatarPage::open(&session, "a", "12345", IMAGE, TaskCancel::new())
        .await
        .unwrap();
    assert_eq!(image.content(), Some("iVBORw0KGgo="));
    assert!(matches!(
        session
            .task_page("a", TaskCancel::new(), Duration::from_millis(10))
            .await,
        Err(TaskError::TimedOut)
    ));
    image.revalidate().await.unwrap();
    drop(image);
    let _released = session
        .task_page("a", TaskCancel::new(), Duration::from_secs(1))
        .await
        .unwrap();
    let commands = peer.commands();
    assert_eq!(commands.len(), 2);
    assert!(commands
        .iter()
        .all(|c| c["method"] == "Runtime.evaluate" && c["sessionId"] == "session-a"));
    // No raw resource read, image reload or screenshot is dispatched by this path.
    let first = commands[0]["params"]["expression"].as_str().unwrap();
    assert!(first.contains("context.drawImage(image, 0, 0)"));
    assert!(!first.contains("fetch("));
    assert!(!first.contains("new Image"));
    assert!(!first.contains("crossOrigin ="));
}

#[tokio::test]
async fn taint_or_missing_pixels_is_nonfatal_and_invalid_resources_never_dispatch() {
    let (peer, session) = Peer::connect().await;
    for url in [
        "file:///avatar.png",
        "data:image/png;base64,eA==",
        "https://evil.test/avatar.png",
        "https://yximgs.com:444/a",
        "https://u@yximgs.com/a",
        "https://yximgs.com/a.svg",
        "https://yximgs.com/a.SVGZ",
    ] {
        assert!(
            reader::AvatarPage::open(&session, "a", "12345", url, TaskCancel::new())
                .await
                .is_err()
        );
    }
    assert!(peer.commands().is_empty());
    let mut missing = probe();
    missing["content"] = Value::Null;
    peer.evaluations(vec![
        EvalReply::Value(missing.clone()),
        EvalReply::Value(missing),
    ]);
    let mut image = reader::AvatarPage::open(&session, "a", "12345", IMAGE, TaskCancel::new())
        .await
        .unwrap();
    assert!(image.content().is_none());
    image.revalidate().await.unwrap();
}

#[tokio::test]
async fn changed_identity_url_document_or_loading_discards_the_image() {
    let (peer, session) = Peer::connect().await;
    let mut changes = Vec::new();
    for (field, value) in [
        ("platformUserId", json!("54321")),
        ("avatarUrl", json!("https://yximgs.com/b.png")),
        ("url", json!("https://s.kwaixiaodian.com.evil.test/")),
    ] {
        let mut p = probe();
        p["page"][field] = value;
        changes.push(p);
    }
    for (field, value) in [
        ("currentSrc", json!("https://yximgs.com/b.png")),
        ("documentEpoch", json!("123457")),
        ("complete", json!(false)),
        ("width", json!(2)),
        ("height", json!(2)),
    ] {
        let mut p = probe();
        p[field] = value;
        p["content"] = Value::Null;
        changes.push(p);
    }
    changes.push(Value::Null);
    for changed in changes {
        peer.evaluations(vec![EvalReply::Value(probe()), EvalReply::Value(changed)]);
        let mut image = reader::AvatarPage::open(&session, "a", "12345", IMAGE, TaskCancel::new())
            .await
            .unwrap();
        assert!(image.revalidate().await.is_err());
    }
}

#[tokio::test]
async fn invalid_payload_size_and_loaded_dimensions_are_rejected() {
    let (peer, session) = Peer::connect().await;
    for (field, value) in [
        (
            "content",
            json!("A".repeat(reader::MAX_AVATAR_BYTES.div_ceil(3) * 4 + 4)),
        ),
        ("width", json!(0)),
        ("height", json!(2049)),
        ("complete", json!(false)),
        ("documentEpoch", json!("NaN")),
    ] {
        let mut p = probe();
        p[field] = value;
        peer.evaluations(vec![EvalReply::Value(p)]);
        assert!(
            reader::AvatarPage::open(&session, "a", "12345", IMAGE, TaskCancel::new())
                .await
                .is_err()
        );
    }
    peer.evaluations(vec![EvalReply::JsError]);
    assert!(
        reader::AvatarPage::open(&session, "a", "12345", IMAGE, TaskCancel::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_and_deadline_do_not_fall_back_to_another_target() {
    let (peer, session) = Peer::connect().await;
    let cancel = TaskCancel::new();
    cancel.cancel();
    assert!(
        reader::AvatarPage::open(&session, "a", "12345", IMAGE, cancel)
            .await
            .is_err()
    );
    assert!(peer.commands().is_empty());
    let cancel = TaskCancel::new();
    peer.evaluations(vec![EvalReply::Value(probe())]);
    let mut image = reader::AvatarPage::open(&session, "a", "12345", IMAGE, cancel.clone())
        .await
        .unwrap();
    cancel.cancel();
    assert!(image.revalidate().await.is_err());
    assert_eq!(peer.commands().len(), 1);
    drop(image);
    peer.evaluations(vec![EvalReply::Stall]);
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        reader::AvatarPage::open(&session, "a", "12345", IMAGE, TaskCancel::new()),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(peer
        .commands()
        .iter()
        .all(|c| c["method"] == "Runtime.evaluate" && c["sessionId"] == "session-a"));
    drop(result);
    peer.clear();
    peer.evaluations(vec![EvalReply::Stall]);
    let cancel = TaskCancel::new();
    let (cancelled, _) = tokio::join!(
        reader::AvatarPage::open(&session, "a", "12345", IMAGE, cancel.clone()),
        async {
            peer.wait_for("Runtime.evaluate", 1).await;
            cancel.cancel();
        }
    );
    assert!(cancelled.is_err());
    assert_eq!(peer.commands().len(), 1);
}

#[tokio::test]
async fn detection_preserves_source_target_without_extending_wire_dto() {
    let (peer, session) = Peer::connect().await;
    for id in ["a", "b"] {
        session
            .bind_page(id)
            .await
            .unwrap()
            .navigate("https://s.kwaixiaodian.com/zone/home", 1000)
            .await
            .unwrap();
    }
    peer.clear();
    // Browser page enumeration order is unspecified. Inspect session IDs rather than assume A first.
    let mut without_image = probe()["page"].clone();
    without_image["avatarUrl"] = Value::Null;
    peer.evaluations(vec![
        EvalReply::Value(without_image),
        EvalReply::Value(probe()["page"].clone()),
    ]);
    let (detection, target) = reader::detect_with_avatar_target(&session, TaskCancel::new())
        .await
        .unwrap();
    let reader::Detection::Found(page) = detection else {
        panic!("expected found")
    };
    assert_eq!(page.avatar_url.as_deref(), Some(IMAGE));
    let reads: Vec<_> = peer
        .commands()
        .into_iter()
        .filter(|c| c["method"] == "Runtime.evaluate")
        .collect();
    assert_eq!(reads.len(), 2);
    assert_eq!(
        format!("session-{}", target.unwrap()),
        reads[1]["sessionId"]
    );
}
