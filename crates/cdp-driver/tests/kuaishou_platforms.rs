//! Kuaishou platform connect/login wire tests on the shared offline CDP peer.
//!
//! No real browser or platform is contacted: the peer drives the real
//! chromiumoxide handler, scripts selector observations, and records the
//! outgoing method/sessionId/params. These prove routing, entry URLs, and the
//! race legs (control selector vs login-page URL), not live DOM behavior.
//!
//! Peer ceiling: the fixture reuses one loader id per target, while
//! chromiumoxide's navigation watcher requires a fresh loader per
//! navigation. Real browsers mint fresh loader ids, but against this peer
//! only the FIRST navigation per target completes. Every test below
//! navigates each target at most once (a second leg uses the other target).
mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cdp_driver::platforms::kuaishou::{
    self, AuthPhase, EnsureAuthOptions, EnsureAuthResult, KUAISHOU_CONFIG,
};
use cdp_driver::TaskCancel;
use common::{EvalReply, Peer};
use serde_json::json;

const BUDGET: Duration = Duration::from_secs(5);
const SHORT: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(5);

fn attached_visible() -> EvalReply {
    EvalReply::Value(json!({"attached": true, "visible": true}))
}

fn expressions(peer: &Peer) -> Vec<String> {
    peer.commands()
        .iter()
        .filter(|c| c["method"] == "Runtime.evaluate")
        .filter_map(|c| {
            c["params"]["expression"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

#[tokio::test]
async fn connect_race_control_selector_wins() {
    let (peer, session) = Peer::connect().await;
    peer.clear();
    // Navigate metadata first, then the control-panel observation.
    peer.evaluations(vec![
        EvalReply::Value(
            json!({"url": KUAISHOU_CONFIG.live_control_url, "title": "control"}),
        ),
        attached_visible(),
    ]);
    let logged_in = kuaishou::connect(&session, "a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert!(logged_in);
    let commands = peer.commands();
    let navigate = commands
        .iter()
        .find(|c| c["method"] == "Page.navigate")
        .expect("connect navigates to the control page");
    assert_eq!(navigate["sessionId"], "session-a");
    assert_eq!(
        navigate["params"]["url"],
        json!(KUAISHOU_CONFIG.live_control_url)
    );
    assert!(
        expressions(&peer).iter().any(|e| e.contains(
            KUAISHOU_CONFIG.verify.in_live_control_selector
        )),
        "control-marker race leg must be observed"
    );
    assert!(commands
        .iter()
        .all(|c| c["sessionId"] == "session-a"
            && c["method"] != "Target.activateTarget"));
}

#[tokio::test]
async fn connect_race_login_url_returns_false_without_selector_wait() {
    let (peer, session) = Peer::connect().await;
    peer.clear();
    // The peer reports whatever URL navigation requested, so entering on the
    // login page exercises the URL leg of the race with no scripting.
    let logged_in = kuaishou::connect_to(
        &session,
        "a",
        KUAISHOU_CONFIG.store_login_url,
        kuaishou::is_kuaishou_login_page,
        Some(KUAISHOU_CONFIG.verify.in_live_control_selector),
        TaskCancel::new(),
        BUDGET,
    )
    .await
    .unwrap();
    assert!(!logged_in);
    let commands = peer.commands();
    let navigate = commands
        .iter()
        .find(|c| c["method"] == "Page.navigate")
        .expect("connect navigates to the entry URL");
    assert_eq!(
        navigate["params"]["url"],
        json!(KUAISHOU_CONFIG.store_login_url)
    );
    assert!(
        !expressions(&peer).iter().any(|e| e.contains(
            KUAISHOU_CONFIG.verify.in_live_control_selector
        )),
        "login-URL verdict must skip the selector wait"
    );
}

#[tokio::test]
async fn login_skips_navigation_when_already_on_login_page() {
    let (peer, session) = Peer::connect().await;
    session
        .bind_page("a")
        .await
        .unwrap()
        .navigate(KUAISHOU_CONFIG.login_url, 2000)
        .await
        .unwrap();
    peer.clear();
    peer.evaluations(vec![
        EvalReply::Value(json!(KUAISHOU_CONFIG.login_url)),
        attached_visible(),
    ]);
    kuaishou::login(&session, "a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let commands = peer.commands();
    assert!(
        commands.iter().all(|c| c["method"] != "Page.navigate"),
        "already on the login page: wait for the scan, do not renavigate"
    );
    assert!(
        expressions(&peer).iter().any(|e| e
            .contains(KUAISHOU_CONFIG.verify.logged_in_selector)),
        "logged-in marker wait must be observed"
    );
}

#[tokio::test]
async fn ensure_auth_reuses_session_without_scan() {
    let (peer, session) = Peer::connect().await;
    peer.clear();
    peer.evaluations(vec![
        EvalReply::Value(
            json!({"url": KUAISHOU_CONFIG.live_control_url, "title": "control"}),
        ),
        attached_visible(),
    ]);
    let result = kuaishou::ensure_auth(
        &session,
        "a",
        EnsureAuthOptions::new(TaskCancel::new()),
    )
    .await;
    assert_eq!(
        result,
        EnsureAuthResult {
            ok: true,
            scanned: false,
            error: None,
        }
    );
    assert!(
        !expressions(&peer).iter().any(|e| e
            .contains(KUAISHOU_CONFIG.verify.logged_in_selector)),
        "cookie reuse must not wait for a scan"
    );
}

#[tokio::test]
async fn jinniu_connect_is_url_only_verdict() {
    let (peer, session) = Peer::connect().await;
    peer.clear();
    let startup = kuaishou::build_startup_url(Some("987"));
    assert!(startup.contains("homeType=new"));
    let reusable = kuaishou::connect_jinniu(&session, "a", &startup, TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert!(reusable);
    assert!(
        !expressions(&peer)
            .iter()
            .any(|e| e.contains("querySelector")),
        "jinniu has no DOM selector: verdict is URL-only"
    );
    peer.clear();
    // NOTE: the offline peer completes only the first navigation per target
    // (loader-id reuse; see header), so the login-page leg below uses the
    // untouched second target instead of re-navigating "a".
    let needs_login = kuaishou::connect_jinniu(
        &session,
        "b",
        "https://passport.kuaishou.com/login",
        TaskCancel::new(),
        BUDGET,
    )
    .await
    .unwrap();
    assert!(!needs_login);
}

#[tokio::test]
async fn connect_falls_back_to_url_verdict_on_selector_error() {
    let (_peer, session) = Peer::connect().await;
    // Unscripted: navigate metadata reports the control URL, the selector
    // observation is malformed (peer default branch), so the marker wait
    // fails and connect falls back to a fresh-lease URL verdict —
    // the jieger `catch` → URL-check leg. Still exactly one navigation.
    let logged_in = kuaishou::connect(&session, "a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert!(logged_in);
}

#[tokio::test]
async fn ensure_auth_scans_when_session_expired() {
    let (peer, session) = Peer::connect().await;
    peer.clear();
    let phases = Arc::new(Mutex::new(Vec::new()));
    let record = phases.clone();
    let options = EnsureAuthOptions {
        cancel: TaskCancel::new(),
        verify_timeout: BUDGET,
        login_timeout: BUDGET,
        on_phase: Some(Arc::new(move |phase| {
            record.lock().unwrap().push(phase);
        })),
    };
    // The entry lands on the login page (the peer reports the navigated URL),
    // so verify fails fast on the URL leg with no selector wait; login then
    // observes the scripted scan completion. One navigation total.
    peer.evaluations(vec![
        EvalReply::Value(
            json!({"url": KUAISHOU_CONFIG.store_login_url, "title": "login"}),
        ),
        EvalReply::Value(json!(KUAISHOU_CONFIG.store_login_url)),
        attached_visible(),
    ]);
    let result = kuaishou::ensure_auth_to(
        &session,
        "a",
        KUAISHOU_CONFIG.store_login_url,
        kuaishou::is_kuaishou_login_page,
        Some(KUAISHOU_CONFIG.verify.in_live_control_selector),
        KUAISHOU_CONFIG.store_login_url,
        KUAISHOU_CONFIG.verify.logged_in_selector,
        options,
    )
    .await;
    assert_eq!(
        result,
        EnsureAuthResult {
            ok: true,
            scanned: true,
            error: None,
        }
    );
    assert_eq!(
        *phases.lock().unwrap(),
        vec![AuthPhase::VerifyingSession, AuthPhase::WaitingForLogin]
    );
    let observed = expressions(&peer);
    assert!(
        !observed.iter().any(|e| e.contains(
            KUAISHOU_CONFIG.verify.in_live_control_selector
        )),
        "login-URL verdict must skip the control wait"
    );
    assert!(
        observed
            .iter()
            .any(|e| e.contains(KUAISHOU_CONFIG.verify.logged_in_selector)),
        "scan wait must observe the logged-in marker"
    );
}

#[tokio::test]
async fn url_wait_returns_once_login_markers_leave() {
    let (_peer, session) = Peer::connect().await;
    // Target `a` starts at https://test.invalid/a: no login marker, immediate Ok.
    kuaishou::wait_until_authenticated_url(
        &session,
        "a",
        kuaishou::is_kuaishou_login_page,
        TaskCancel::new(),
        BUDGET,
        POLL,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn url_wait_times_out_on_login_page() {
    let (_peer, session) = Peer::connect().await;
    session
        .bind_page("a")
        .await
        .unwrap()
        .navigate(KUAISHOU_CONFIG.login_url, 2000)
        .await
        .unwrap();
    let result = kuaishou::wait_until_authenticated_url(
        &session,
        "a",
        kuaishou::is_kuaishou_login_page,
        TaskCancel::new(),
        SHORT,
        POLL,
    )
    .await;
    assert!(matches!(result, Err(cdp_driver::TaskError::TimedOut)));
}
