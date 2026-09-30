//! Offline real-handler wire tests, NOT a JavaScript engine or DOM simulation.
//! Selector observations/errors are scripted CDP payloads; matching/polling and
//! routing are real Rust behavior. No user browser or platform is contacted.
mod common;

use std::sync::Arc;
use std::time::Duration;

use cdp_driver::{SelectorState, TaskCancel, TaskError};
use common::{EvalReply, Peer};
use futures::poll;
use serde_json::json;
use tokio::time::timeout;

const BUDGET: Duration = Duration::from_secs(5);
const SHORT: Duration = Duration::from_millis(200);
const POLL: Duration = Duration::from_millis(1);

fn sample(attached: bool, visible: bool) -> EvalReply {
    EvalReply::Value(json!({"attached": attached, "visible": visible}))
}

#[tokio::test]
async fn bindings_share_one_queue_release_and_drop_unlock() {
    let (peer, session) = Peer::connect().await;
    let first = session.bind_page("a").await.unwrap();
    let second = session.bind_page("a").await.unwrap();
    let mut lease = first.into_task(TaskCancel::new(), BUDGET).await.unwrap();
    let mut queued = Box::pin(second.into_task(TaskCancel::new(), BUDGET));
    assert!(
        poll!(queued.as_mut()).is_pending(),
        "separate bindings must share a lock"
    );
    assert_eq!(lease.evaluate("identity", BUDGET).await.unwrap(), "a");
    assert!(poll!(queued.as_mut()).is_pending());
    lease.release();
    let lease = timeout(BUDGET, queued).await.unwrap().unwrap();
    assert_eq!(lease.target_id(), "a");
    let mut third = Box::pin(session.task_page("a", TaskCancel::new(), BUDGET));
    assert!(poll!(third.as_mut()).is_pending());
    drop(lease);
    timeout(BUDGET, third).await.unwrap().unwrap().release();
    assert!(peer
        .commands()
        .iter()
        .all(|c| c["sessionId"] == "session-a"));
}

#[tokio::test]
async fn different_targets_and_sessions_run_independently_with_real_wire_routing() {
    let (peer, session) = Peer::connect().await;
    let (_other_peer, other_session) = Peer::connect().await;
    let mut a = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let mut b = session
        .task_page("b", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let mut other_a = other_session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let (av, bv, other) = tokio::join!(
        a.evaluate("identity", BUDGET),
        b.evaluate("identity", BUDGET),
        other_a.evaluate("identity", BUDGET)
    );
    assert_eq!(av.unwrap(), "a");
    assert_eq!(bv.unwrap(), "b");
    assert_eq!(other.unwrap(), "a");
    assert!(peer
        .commands()
        .iter()
        .any(|c| c["sessionId"] == "session-a"));
    assert!(peer
        .commands()
        .iter()
        .any(|c| c["sessionId"] == "session-b"));
}

#[tokio::test]
async fn queued_cancellation_and_timeout_do_not_acquire_or_steal_lock() {
    let (peer, session) = Peer::connect().await;
    let owner = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let cancel = TaskCancel::new();
    let mut cancelled = Box::pin(session.task_page("a", cancel.clone(), BUDGET));
    assert!(poll!(cancelled.as_mut()).is_pending());
    cancel.cancel();
    assert!(matches!(cancelled.await, Err(TaskError::Cancelled)));
    assert!(matches!(
        session.task_page("a", TaskCancel::new(), SHORT).await,
        Err(TaskError::TimedOut)
    ));
    let mut live = Box::pin(session.task_page("a", TaskCancel::new(), BUDGET));
    assert!(
        poll!(live.as_mut()).is_pending(),
        "owner must still hold lock"
    );
    owner.release();
    timeout(BUDGET, live).await.unwrap().unwrap().release();
    assert!(peer.commands().is_empty());
}

#[tokio::test]
async fn cancellation_wins_when_lock_and_cancel_are_both_ready() {
    let (_peer, session) = Peer::connect().await;
    let owner = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let cancel = TaskCancel::new();
    let mut queued = Box::pin(session.task_page("a", cancel.clone(), BUDGET));
    assert!(poll!(queued.as_mut()).is_pending());
    owner.release();
    cancel.cancel();
    assert!(matches!(queued.await, Err(TaskError::Cancelled)));
    session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap()
        .release();
}

#[tokio::test]
async fn pre_cancelled_and_zero_deadline_do_not_dispatch_any_action() {
    let (peer, session) = Peer::connect().await;
    let cancel = TaskCancel::new();
    cancel.cancel();
    assert!(matches!(
        session.task_page("a", cancel, Duration::ZERO).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        session
            .task_page("a", TaskCancel::new(), Duration::ZERO)
            .await,
        Err(TaskError::TimedOut)
    ));
    let cancel = TaskCancel::new();
    let mut page = session
        .task_page("a", cancel.clone(), BUDGET)
        .await
        .unwrap();
    cancel.cancel();
    assert!(matches!(
        page.navigate("https://test.invalid/unused", BUDGET).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        page.evaluate("identity", BUDGET).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        page.screenshot(BUDGET).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        page.click("#present", BUDGET).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        page.type_text("#present", "x", BUDGET).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        page.extract(BUDGET).await,
        Err(TaskError::Cancelled)
    ));
    assert!(matches!(
        page.wait_for_selector("#x", SelectorState::Attached, BUDGET, POLL)
            .await,
        Err(TaskError::Cancelled)
    ));
    page.release();
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert!(matches!(
        page.evaluate("identity", Duration::ZERO).await,
        Err(TaskError::TimedOut)
    ));
    assert!(matches!(
        page.evaluate("identity", BUDGET).await,
        Err(TaskError::TimedOut)
    ));
    assert!(peer.commands().is_empty());
}

#[tokio::test]
async fn all_controlled_operations_preserve_target_and_legacy_selection() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    peer.clear();
    let nav = page
        .navigate("https://test.invalid/task-a", BUDGET)
        .await
        .unwrap();
    assert_eq!(nav.title, "a");
    assert_eq!(page.evaluate("identity", BUDGET).await.unwrap(), "a");
    assert_eq!(page.screenshot(BUDGET).await.unwrap(), "QQ==");
    page.click("#present", BUDGET).await.unwrap();
    page.type_text("#present", "中x", BUDGET).await.unwrap();
    assert_eq!(
        page.extract(BUDGET).await.unwrap(),
        json!({"url":nav.url,"title":"a","text":"text-a"})
    );
    assert!(peer
        .commands()
        .iter()
        .all(|c| c["sessionId"] == "session-a"));
    assert!(!peer
        .commands()
        .iter()
        .any(|c| c["method"] == "Target.activateTarget" || c["method"] == "Page.bringToFront"));
    // Legacy is intentionally NOT covered by the cooperative lock.
    assert_eq!(session.evaluate("identity").await.unwrap(), "b");
    assert_eq!(
        session
            .bind_page("a")
            .await
            .unwrap()
            .evaluate("identity")
            .await
            .unwrap(),
        "a"
    );
}

#[tokio::test]
async fn selector_states_poll_scripted_observations_on_bound_target() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let selector = "[data-value=\"中\\\n\u{1b}\"]";
    for (state, replies) in [
        (
            SelectorState::Attached,
            vec![sample(false, false), sample(true, false)],
        ),
        (
            SelectorState::Detached,
            vec![sample(true, true), sample(false, false)],
        ),
        (
            SelectorState::Visible,
            vec![sample(true, false), sample(true, true)],
        ),
        (
            SelectorState::Hidden,
            vec![sample(true, true), sample(true, false)],
        ),
    ] {
        peer.clear();
        peer.evaluations(replies);
        page.wait_for_selector(selector, state, BUDGET, POLL)
            .await
            .unwrap();
        let commands = peer.commands();
        assert_eq!(commands.len(), 2);
        assert!(commands
            .iter()
            .all(|c| c["sessionId"] == "session-a" && c["method"] == "Runtime.evaluate"));
        assert!(commands[0]["params"]["expression"]
            .as_str()
            .unwrap()
            .contains(&serde_json::to_string(selector).unwrap()));
    }
    peer.evaluations(vec![sample(false, false)]);
    page.wait_for_selector("#absent", SelectorState::Hidden, BUDGET, POLL)
        .await
        .unwrap();
}

#[tokio::test]
async fn selector_js_cdp_payload_and_option_errors_fail_without_polling_again() {
    let (peer, session) = Peer::connect().await;
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert!(matches!(
        page.wait_for_selector("#x", SelectorState::Attached, BUDGET, Duration::ZERO)
            .await,
        Err(TaskError::InvalidOptions(_))
    ));
    assert!(peer.commands().is_empty());
    // A browser would throw for '['. This peer injects exceptionDetails; it does
    // not parse or execute that selector. The test proves error propagation.
    peer.evaluations(vec![EvalReply::JsError]);
    assert!(matches!(
        page.wait_for_selector("[", SelectorState::Visible, BUDGET, POLL)
            .await,
        Err(TaskError::Driver(_))
    ));
    assert_eq!(peer.commands().len(), 1);
    peer.clear();
    peer.fail("Runtime.evaluate", None);
    assert!(matches!(
        page.wait_for_selector("#x", SelectorState::Attached, BUDGET, POLL)
            .await,
        Err(TaskError::Driver(_))
    ));
    assert_eq!(peer.commands().len(), 1);
    peer.clear();
    peer.evaluations(vec![EvalReply::Value(json!({"attached":"bad"}))]);
    assert!(matches!(
        page.wait_for_selector("#x", SelectorState::Attached, BUDGET, POLL)
            .await,
        Err(TaskError::Driver(_))
    ));
    assert_eq!(peer.commands().len(), 1);
}

#[tokio::test]
async fn selector_deadline_covers_sleep_and_slow_evaluate_and_poison_is_sticky() {
    let (peer, session) = Peer::connect().await;
    for reply in [sample(false, false), EvalReply::Stall] {
        let mut page = session
            .task_page("a", TaskCancel::new(), BUDGET)
            .await
            .unwrap();
        peer.clear();
        peer.evaluations(vec![reply]);
        let result = timeout(
            BUDGET,
            page.wait_for_selector("#x", SelectorState::Attached, SHORT, BUDGET),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(TaskError::TimedOut)));
        assert!(matches!(
            page.evaluate("identity", BUDGET).await,
            Err(TaskError::TimedOut)
        ));
        assert_eq!(peer.commands().len(), 1);
    }
}

#[tokio::test]
async fn cancellation_interrupts_selector_sleep_and_pending_evaluation() {
    let (peer, session) = Peer::connect().await;
    for reply in [sample(false, false), EvalReply::Stall] {
        let cancel = TaskCancel::new();
        let mut page = session
            .task_page("a", cancel.clone(), BUDGET)
            .await
            .unwrap();
        peer.clear();
        peer.evaluations(vec![reply]);
        let mut action = Box::pin(page.wait_for_selector(
            "#x",
            SelectorState::Attached,
            Duration::from_secs(60),
            Duration::from_secs(60),
        ));
        tokio::select! {
            result = &mut action => panic!("wait finished before cancellation: {result:?}"),
            _ = peer.wait_for("Runtime.evaluate", 1) => {}
        }
        // B's reply is ordered after A's reply on this WebSocket. Polling A after
        // this barrier consumes its observation and reaches the 60s sleep (or
        // stays on the stalled evaluate). No scheduler-dependent delay assertion.
        session
            .bind_page("b")
            .await
            .unwrap()
            .evaluate("identity")
            .await
            .unwrap();
        assert!(poll!(action.as_mut()).is_pending());
        cancel.cancel();
        assert!(matches!(
            timeout(BUDGET, action).await.unwrap(),
            Err(TaskError::Cancelled)
        ));
        assert!(matches!(
            page.click("#present", BUDGET).await,
            Err(TaskError::Cancelled)
        ));
        assert_eq!(
            peer.commands()
                .iter()
                .filter(|c| c["sessionId"] == "session-a")
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn direct_evaluate_cancel_retains_lock_until_release_and_dropped_queue_is_safe() {
    let (peer, session) = Peer::connect().await;
    let cancel = TaskCancel::new();
    let mut page = session
        .task_page("a", cancel.clone(), BUDGET)
        .await
        .unwrap();
    let mut abandoned = Box::pin(session.task_page("a", TaskCancel::new(), BUDGET));
    assert!(poll!(abandoned.as_mut()).is_pending());
    drop(abandoned);
    peer.evaluations(vec![EvalReply::Stall]);
    let mut action = Box::pin(page.evaluate("slow", Duration::from_secs(60)));
    tokio::select! {
        result = &mut action => panic!("stalled evaluate finished: {result:?}"),
        _ = peer.wait_for("Runtime.evaluate", 1) => {}
    }
    cancel.cancel();
    assert!(matches!(
        timeout(BUDGET, action).await.unwrap(),
        Err(TaskError::Cancelled)
    ));
    let mut next = Box::pin(session.task_page("a", TaskCancel::new(), BUDGET));
    assert!(
        poll!(next.as_mut()).is_pending(),
        "cancellation does not unlock a retained lease"
    );
    page.release();
    timeout(BUDGET, next).await.unwrap().unwrap().release();
    assert_eq!(peer.commands().len(), 1);
}

#[tokio::test]
async fn slow_evaluate_and_navigation_use_typed_whole_operation_deadlines() {
    let (peer, session) = Peer::connect().await;
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    peer.evaluations(vec![EvalReply::Stall]);
    assert!(matches!(
        timeout(BUDGET, page.evaluate("slow", SHORT)).await.unwrap(),
        Err(TaskError::TimedOut)
    ));
    assert!(matches!(
        page.screenshot(BUDGET).await,
        Err(TaskError::TimedOut)
    ));
    assert_eq!(peer.commands().len(), 1);
    page.release();
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert!(matches!(
        timeout(BUDGET, page.navigate("https://test.invalid/stall", SHORT))
            .await
            .unwrap(),
        Err(TaskError::TimedOut)
    ));
    assert!(matches!(
        page.extract(BUDGET).await,
        Err(TaskError::TimedOut)
    ));
}

#[tokio::test]
async fn cancelling_in_flight_input_does_not_dispatch_release_or_later_keys() {
    let (peer, session) = Peer::connect().await;
    for (method, kind, typing) in [
        ("Input.dispatchKeyEvent", "keyDown", true),
        ("Input.dispatchMouseEvent", "mousePressed", false),
    ] {
        let cancel = TaskCancel::new();
        let mut page = session
            .task_page("a", cancel.clone(), BUDGET)
            .await
            .unwrap();
        peer.clear();
        peer.stall(method, Some(kind));
        let action = async {
            if typing {
                page.type_text("#present", "ab", BUDGET).await
            } else {
                page.click("#present", BUDGET).await
            }
        };
        let (result, ()) = timeout(BUDGET, async {
            tokio::join!(action, async {
                // For mouse, wait for press rather than an earlier movement.
                loop {
                    let commands = peer.commands();
                    if commands
                        .iter()
                        .any(|c| c["method"] == method && c["params"]["type"] == kind)
                    {
                        break;
                    }
                    let next = commands.iter().filter(|c| c["method"] == method).count() + 1;
                    peer.wait_for(method, next).await;
                }
                cancel.cancel();
            })
        })
        .await
        .unwrap();
        assert!(matches!(result, Err(TaskError::Cancelled)));
        assert!(matches!(
            page.evaluate("identity", BUDGET).await,
            Err(TaskError::Cancelled)
        ));
        // Round-trip barrier on another target, rather than a timing assertion.
        session
            .bind_page("b")
            .await
            .unwrap()
            .evaluate("identity")
            .await
            .unwrap();
        let commands = peer.commands();
        let input: Vec<_> = commands.iter().filter(|c| c["method"] == method).collect();
        assert_eq!(input.last().unwrap()["params"]["type"], kind);
        if typing {
            assert_eq!(input.len(), 1);
        }
    }
}

#[tokio::test]
async fn aborting_owner_releases_lease_and_dropping_action_poison_prevents_reuse() {
    let (peer, session) = Peer::connect().await;
    let session = Arc::new(session);
    let worker_session = session.clone();
    peer.evaluations(vec![EvalReply::Stall]);
    let worker = tokio::spawn(async move {
        let mut page = worker_session
            .task_page("a", TaskCancel::new(), BUDGET)
            .await
            .unwrap();
        page.evaluate("slow", Duration::from_secs(60)).await
    });
    peer.wait_for("Runtime.evaluate", 1).await;
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    peer.clear();
    peer.evaluations(vec![EvalReply::Stall]);
    {
        let action = page.evaluate("slow-again", BUDGET);
        tokio::pin!(action);
        tokio::select! {
            result = &mut action => panic!("stalled action finished: {result:?}"),
            _ = peer.wait_for("Runtime.evaluate", 1) => {}
        }
    }
    assert!(matches!(
        page.evaluate("identity", BUDGET).await,
        Err(TaskError::Interrupted)
    ));
    assert_eq!(peer.commands().len(), 1);
    page.release();
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    assert_eq!(page.evaluate("identity", BUDGET).await.unwrap(), "a");
}

#[tokio::test]
async fn queued_target_is_revalidated_after_closure_and_failures_never_fallback() {
    let (peer, session) = Peer::connect().await;
    session.activate_page("b").await.unwrap();
    let mut page = session
        .task_page("a", TaskCancel::new(), BUDGET)
        .await
        .unwrap();
    let bound = session.bind_page("a").await.unwrap();
    let mut queued = Box::pin(bound.into_task(TaskCancel::new(), BUDGET));
    assert!(poll!(queued.as_mut()).is_pending());
    peer.fail("Runtime.evaluate", None);
    assert!(matches!(
        page.evaluate("identity", BUDGET).await,
        Err(TaskError::Driver(_))
    ));
    session.close_page("a").await.unwrap();
    peer.clear();
    assert!(matches!(
        page.evaluate("identity", BUDGET).await,
        Err(TaskError::Driver(_))
    ));
    page.release();
    assert!(matches!(
        timeout(BUDGET, queued).await.unwrap(),
        Err(TaskError::Driver(_))
    ));
    assert!(matches!(
        session
            .task_page("missing", TaskCancel::new(), BUDGET)
            .await,
        Err(TaskError::Driver(_))
    ));
    assert!(peer
        .commands()
        .iter()
        .all(|c| c["sessionId"] != "session-b"));
    assert!(!peer
        .commands()
        .iter()
        .any(|c| c["method"] == "Target.createTarget" || c["method"] == "Target.activateTarget"));
    assert_eq!(session.evaluate("identity").await.unwrap(), "b");
}
