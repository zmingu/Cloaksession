//! Fixed-target tests using the shared offline wire peer.
mod common;
use common::Peer;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn two_bindings_stay_on_target_after_active_switch() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    peer.clear();
    let a = session.bind_page("a").await.unwrap();
    let b = session.bind_page("b").await.unwrap();
    assert_eq!(a.target_id(), "a");
    assert_eq!(b.target_id(), "b");
    assert!(
        peer.commands().is_empty(),
        "binding must not activate or send commands"
    );
    let (av, bv) = tokio::join!(a.evaluate("identity"), b.evaluate("identity"));
    assert_eq!(av.unwrap(), "a");
    assert_eq!(bv.unwrap(), "b");
    session.activate_page("a").await.unwrap();
    assert_eq!(b.evaluate("identity").await.unwrap(), "b");
    session.activate_page("b").await.unwrap();
    peer.clear();
    let nav = a
        .navigate("https://test.invalid/navigated-a", 2000)
        .await
        .unwrap();
    assert_eq!(nav.title, "a");
    assert_eq!(nav.url, "https://test.invalid/navigated-a");
    assert_eq!(a.screenshot().await.unwrap(), "QQ==");
    a.click("#present").await.unwrap();
    a.type_text("#present", "中x").await.unwrap();
    assert_eq!(
        a.extract().await.unwrap(),
        json!({"url":nav.url,"title":"a","text":"text-a"})
    );
    let commands = peer.commands();
    assert!(!commands.is_empty());
    assert!(
        commands.iter().all(|c| c["sessionId"] == "session-a"),
        "{commands:?}"
    );
    assert!(!commands
        .iter()
        .any(|c| c["method"] == "Target.activateTarget" || c["method"] == "Page.bringToFront"));
    assert!(commands
        .iter()
        .any(|c| c["method"] == "Input.dispatchKeyEvent" && c["params"]["text"] == "中"));
    assert_eq!(session.evaluate("identity").await.unwrap(), "b");
}

#[tokio::test]
async fn missing_and_closed_targets_never_fall_back() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    peer.clear();
    assert!(session.bind_page("missing").await.is_err());
    assert!(session.bind_page("").await.is_err());
    assert!(peer.commands().is_empty());
    let a = session.bind_page("a").await.unwrap();
    session.close_page("a").await.unwrap();
    assert!(session.bind_page("a").await.is_err());
    peer.clear();
    tokio::time::timeout(Duration::from_secs(2), async {
        assert!(a.evaluate("identity").await.is_err());
        assert!(a.screenshot().await.is_err());
        assert!(a.click("#present").await.is_err());
        assert!(a.type_text("#present", "").await.is_err());
        assert!(a.extract().await.is_err());
        assert!(a
            .navigate("https://test.invalid/closed", 500)
            .await
            .is_err());
    })
    .await
    .expect("closed page errors must be bounded");
    assert!(peer
        .commands()
        .iter()
        .all(|c| c["sessionId"] != "session-b"));
    assert_eq!(session.evaluate("identity").await.unwrap(), "b");
}

#[tokio::test]
async fn bound_errors_propagate_and_legacy_input_policy_is_preserved() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("a").await.unwrap();
    let a = session.bind_page("a").await.unwrap();
    assert!(a
        .evaluate("throw_error")
        .await
        .unwrap_err()
        .to_string()
        .contains("mock JS exception"));
    peer.fail("Runtime.evaluate", None);
    assert!(a
        .evaluate("identity")
        .await
        .unwrap_err()
        .to_string()
        .contains("mock CDP failure"));
    peer.fail("Page.captureScreenshot", None);
    assert!(a.screenshot().await.is_err());
    assert!(a.click("#missing").await.is_err());
    peer.clear();
    assert!(a.type_text("#missing", "x").await.is_err());
    assert!(!peer
        .commands()
        .iter()
        .any(|c| c["method"] == "Input.dispatchKeyEvent"));
    peer.fail("Runtime.evaluate", None);
    assert!(a.type_text("#present", "x").await.is_err());
    for kind in ["mouseMoved", "mousePressed", "mouseReleased"] {
        peer.fail("Input.dispatchMouseEvent", Some(kind));
        assert!(a
            .click("#present")
            .await
            .unwrap_err()
            .to_string()
            .contains("mock CDP failure"));
        peer.fail("Input.dispatchMouseEvent", Some(kind));
        session.click("#present").await.unwrap();
    }
    for kind in ["keyDown", "keyUp"] {
        peer.fail("Input.dispatchKeyEvent", Some(kind));
        assert!(a.type_text("#present", "x").await.is_err());
        peer.fail("Input.dispatchKeyEvent", Some(kind));
        session.type_text("#present", "x").await.unwrap();
    }
    // The old missing-focus behavior remains intentionally best-effort.
    session.type_text("#missing", "x").await.unwrap();
}

#[tokio::test]
async fn task_page_creation_preserves_legacy_selection() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("a").await.unwrap();
    peer.clear();
    let new = tokio::time::timeout(
        Duration::from_secs(3),
        session.new_bound_page("about:blank"),
    )
    .await
    .expect("new task page deadline")
    .unwrap();
    assert_eq!(new.target_id(), "new-1");
    assert_eq!(new.evaluate("identity").await.unwrap(), "new-1");
    assert_eq!(session.evaluate("identity").await.unwrap(), "a");
    let commands = peer.commands();
    assert!(commands
        .iter()
        .any(|c| c["method"] == "Target.createTarget" && c["params"]["background"] == true));
    assert!(!commands
        .iter()
        .any(|c| c["method"] == "Target.activateTarget"));
    let legacy = session.new_page("about:blank").await.unwrap();
    assert_eq!(session.evaluate("identity").await.unwrap(), legacy);
    assert_eq!(new.evaluate("identity").await.unwrap(), "new-1");
    let nav = session
        .navigate("https://test.invalid/legacy", 500)
        .await
        .unwrap();
    assert_eq!(nav.title, legacy);
    assert_eq!(session.extract().await.unwrap()["url"], nav.url);
    assert_eq!(session.screenshot().await.unwrap(), "Qg==");
    assert!(peer
        .commands()
        .iter()
        .any(|c| c["method"] == "Target.activateTarget"));
}

#[tokio::test]
async fn task_page_creation_failure_does_not_change_active_or_retry() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("a").await.unwrap();
    peer.clear();
    peer.fail("Target.createTarget", None);
    let error = session
        .new_bound_page("about:blank")
        .await
        .err()
        .expect("creation must fail");
    assert!(error.to_string().contains("mock CDP failure"));
    let commands = peer.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0]["method"], "Target.createTarget");
    assert_eq!(session.evaluate("identity").await.unwrap(), "a");
}

#[tokio::test]
async fn bound_navigation_deadline_does_not_redirect_to_active_page() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    let a = session.bind_page("a").await.unwrap();
    peer.clear();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        a.navigate("https://test.invalid/stall", 50),
    )
    .await
    .expect("outer deadline")
    .err()
    .expect("navigation should time out");
    assert!(error.to_string().contains("navigate target a timed out"));
    let commands = peer.commands();
    assert!(commands
        .iter()
        .any(|c| c["method"] == "Page.navigate" && c["sessionId"] == "session-a"));
    assert!(commands.iter().all(|c| c["sessionId"] == "session-a"));
    assert_eq!(session.evaluate("identity").await.unwrap(), "b");
}
