//! Identity reader on the shared local CDP peer; no actual browser/platform is visited.
mod common;
// Compile the production reader against the same real chromiumoxide fixture, without a reverse
// library dependency or copying the peer. The static script is returned, not executed, by the peer.
#[allow(dead_code)] // This binary exercises identity only; cached_resource covers avatar helpers.
#[path = "../../tauri-app/src/driver/identity/extract.rs"]
mod reader;
use cdp_driver::{session::BrowserSession, TaskCancel};
use common::{EvalReply, Peer};
use serde_json::json;
use std::time::Duration;

async fn shop_pages(session: &BrowserSession) {
    for id in ["a", "b"] {
        session
            .bind_page(id)
            .await
            .unwrap()
            .navigate("https://s.kwaixiaodian.com/zone/home", 1000)
            .await
            .unwrap();
    }
}
fn identity(id: Option<&str>) -> EvalReply {
    EvalReply::Value(
        json!({"url":"https://s.kwaixiaodian.com/zone/home","platformUserId":id,"nickname":"N","avatarUrl":null}),
    )
}
#[tokio::test]
async fn identity_reads_each_bound_target_without_active_or_navigation_side_effects() {
    let (peer, session) = Peer::connect().await;
    shop_pages(&session).await;
    session.activate_page("b").await.unwrap();
    peer.clear();
    peer.evaluations(vec![identity(Some("12345")), identity(Some("12345"))]);
    let reader::Detection::Found(found) =
        reader::detect(&session, TaskCancel::new()).await.unwrap()
    else {
        panic!("expected identity")
    };
    assert_eq!(found.platform_user_id.as_deref(), Some("12345"));
    let commands = peer.commands();
    let reads: Vec<_> = commands
        .iter()
        .filter(|c| c["method"] == "Runtime.evaluate")
        .collect();
    assert_eq!(reads.len(), 2);
    assert_ne!(reads[0]["sessionId"], reads[1]["sessionId"]);
    assert!(commands
        .iter()
        .all(|c| c["method"] != "Target.activateTarget"
            && c["method"] != "Page.navigate"
            && c["method"] != "Target.createTarget"));
    peer.evaluations(vec![identity(Some("12345")), identity(Some("54321"))]);
    assert!(matches!(
        reader::detect(&session, TaskCancel::new()).await.unwrap(),
        reader::Detection::Conflict
    ));
}
#[tokio::test]
async fn no_shop_no_id_and_js_error_are_distinct() {
    let (peer, session) = Peer::connect().await;
    assert!(matches!(
        reader::detect(&session, TaskCancel::new()).await.unwrap(),
        reader::Detection::NoPage
    ));
    shop_pages(&session).await;
    peer.evaluations(vec![identity(None), identity(None)]);
    assert!(matches!(
        reader::detect(&session, TaskCancel::new()).await.unwrap(),
        reader::Detection::NoId
    ));
    peer.evaluations(vec![identity(Some("12345")), EvalReply::JsError]);
    assert!(reader::detect(&session, TaskCancel::new()).await.is_err());
}
#[tokio::test]
async fn moved_origin_cancel_and_task_lock_wait_are_errors_not_success() {
    let (peer, session) = Peer::connect().await;
    shop_pages(&session).await;
    peer.evaluations(vec![EvalReply::Value(json!({"url":"https://s.kwaixiaodian.com.evil.test","platformUserId":"12345","nickname":null,"avatarUrl":null}))]);
    assert!(reader::detect(&session, TaskCancel::new()).await.is_err());
    let cancel = TaskCancel::new();
    cancel.cancel();
    assert!(reader::detect(&session, cancel).await.is_err());
    let _a = session
        .task_page("a", TaskCancel::new(), Duration::from_secs(1))
        .await
        .unwrap();
    let _b = session
        .task_page("b", TaskCancel::new(), Duration::from_secs(1))
        .await
        .unwrap();
    assert!(tokio::time::timeout(
        Duration::from_secs(2),
        reader::detect(&session, TaskCancel::new())
    )
    .await
    .unwrap()
    .is_err());
}
