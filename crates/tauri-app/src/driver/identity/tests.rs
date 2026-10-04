use super::*;
#[path = "../../../../cdp-driver/tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn initialization_reservation_skips_real_monitor_tick_and_busy_manual_read_never_saves_error() {
    use crate::driver::business_tests::{fixture, launch_without_cdp};
    use mcp_server::driver::BrowserDriver;
    let (_temp, driver) = fixture(multizen_core::ChromixSettings::default());
    let driver = Arc::new(driver);
    let profile = driver.create_profile(multizen_core::CreateProfileInput {
        name: "initialization contention fixture".into(), ..Default::default()
    }).await.unwrap();
    let launched = launch_without_cdp(&driver, &profile).await.unwrap();
    let slot = driver.registry.prepared_slot(&profile.id, &format!("{}:{}", launched.started_at, launched.pid)).await.unwrap();
    let (peer, session) = common::Peer::connect().await;
    for target in ["a", "b"] {
        session.bind_page(target).await.unwrap().navigate("https://s.kwaixiaodian.com/zone/home", 1000).await.unwrap();
    }
    let session = Arc::new(session);
    slot.install_test_session(session.clone());
    let mut snapshot = Snapshot::empty(&profile.id, Status::Detected);
    snapshot.platform_user_id = Some("12345".into());
    snapshot.checked_at = Some(chrono::Utc::now().to_rfc3339());
    let expected = BusinessProfileState { account: None, scope: None };
    driver.identity_request(|resp| IdentityCmd::Save {
        data: Observation { snapshot, session_id: Some(slot.id.clone()), avatar_url: None },
        expected, slot: slot.clone(), stop: driver.identity.stop.clone(), deadline: Instant::now() + COMMAND_WAIT, resp,
    }).await.unwrap().unwrap();
    let reservation = driver.identity.reserve_initialization(&profile.id).unwrap();
    let mut owned = session.task_page("a", slot.cancel.clone(), Duration::from_secs(1)).await.unwrap();
    peer.clear();
    let mut cursor = 0;
    // This is the same tick dispatcher called by the app monitor. A reserved candidate
    // never starts the TaskPage detector or produces an Error observation.
    let candidates = driver.registry.ids().await;
    assert!(candidates.contains(&profile.id));
    assert_eq!(driver.identity_monitor_tick(candidates, &mut cursor), 0);
    assert_eq!(driver.identity.permits.available_permits(), MAX_CONCURRENT);
    let other = driver.identity.enter("another-profile").unwrap();
    drop(other);
    let busy = driver.kuaishou_identity_detect(&profile.id).await;
    assert_eq!(busy.status, Status::Unknown);
    let persisted = driver.identity_request(|resp| IdentityCmd::Read { id: profile.id.clone(), resp }).await.unwrap().unwrap().2.unwrap();
    assert_eq!(persisted.snapshot.status, Status::Detected);
    assert_eq!(persisted.snapshot.platform_user_id.as_deref(), Some("12345"));
    assert!(peer.commands().iter().all(|command| command["method"] != "Runtime.evaluate"));
    let value = |id: &str| common::EvalReply::Value(serde_json::json!({
        "url": "https://s.kwaixiaodian.com/zone/home", "platformUserId": id,
        "nickname": null, "avatarUrl": null,
    }));
    peer.evaluations(vec![value("12345"), value("12345"), value("12345")]);
    driver.validate_kuaishou_task_identity(&session, &mut owned, "12345", slot.cancel.clone()).await.unwrap();
    let evaluations: Vec<_> = peer.commands().into_iter().filter(|command| command["method"] == "Runtime.evaluate").collect();
    assert_eq!(evaluations.len(), 3); // held target, other target, held target again: no self-reacquisition
    assert_eq!(evaluations[0]["sessionId"], evaluations[2]["sessionId"]);
    assert_ne!(evaluations[0]["sessionId"], evaluations[1]["sessionId"]);
    peer.evaluations(vec![value("12345"), value("23456")]);
    assert!(driver.validate_kuaishou_task_identity(&session, &mut owned, "12345", slot.cancel.clone()).await.is_err());
    peer.evaluations(vec![value("23456"), value("12345")]);
    assert!(driver.validate_kuaishou_task_identity(&session, &mut owned, "12345", slot.cancel.clone()).await.is_err());
    assert!(peer.commands().iter().all(|command| !matches!(command["method"].as_str(), Some("Page.navigate" | "Target.activateTarget" | "Target.createTarget"))));
    drop(owned);
    drop(reservation);
    assert!(driver.identity.enter(&profile.id).is_some());
    driver.close(&profile.id).await.unwrap();
    driver.shutdown().await;
}

#[test]
fn running_events_have_exact_camel_case_payloads() {
    use crate::driver::RunningStateChange;
    use serde_json::json;
    for (event, expected) in [
        (
            RunningStateChange::Launched {
                profile_id: "p".into(),
            },
            json!({"kind":"launched","profileId":"p"}),
        ),
        (
            RunningStateChange::Closing {
                profile_id: "p".into(),
            },
            json!({"kind":"closing","profileId":"p"}),
        ),
        (
            RunningStateChange::Closed {
                profile_id: "p".into(),
                reason: "user-close",
            },
            json!({"kind":"closed","profileId":"p","reason":"user-close"}),
        ),
    ] {
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value, expected);
        assert!(value.get("profile_id").is_none());
    }
}

// Poll exactly to the next suspension point; no timing/sleep assumptions in reply races.
async fn poll_pending<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn old_attach_failure_rollback_carries_original_slot() {
    use crate::driver::{
        business_tests::{fixture, launch_without_cdp},
        LauncherCmd,
    };
    use mcp_server::driver::BrowserDriver;
    let (_temp, mut driver) = fixture(multizen_core::ChromixSettings::default());
    let profile = driver
        .create_profile(multizen_core::CreateProfileInput {
            name: "P".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let launched = launch_without_cdp(&driver, &profile).await.unwrap();
    let marker = format!("{}:{}", launched.started_at, launched.pid);
    let original = driver
        .registry
        .prepared_slot(&profile.id, &marker)
        .await
        .unwrap();
    // Keep the real fake-launcher channel for cleanup; script only the driver's reply timing.
    let real_tx = driver.launcher_tx.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    driver.launcher_tx = tx;
    {
        let launch = driver.launch(&profile.id, false);
        tokio::pin!(launch);
        poll_pending(launch.as_mut()).await;
        let LauncherCmd::Launch { resp, .. } = rx.recv().await.unwrap() else {
            panic!("expected launch")
        };
        resp.send(Ok(launched)).unwrap();
        poll_pending(launch.as_mut()).await; // CDP attach is now awaiting its unreachable endpoint.
        driver.registry.remove(&profile.id).await;
        let replacement = driver
            .registry
            .prepare(
                &profile.id,
                "http://127.0.0.1:1",
                multizen_core::BrowserEngine::Chromix,
                "replacement",
                None,
            )
            .await;
        driver.running.lock().unwrap().insert(profile.id.clone());
        poll_pending(launch.as_mut()).await;
        let LauncherCmd::ClosePrepared { slot, resp, .. } = rx.recv().await.unwrap() else {
            panic!("stale failure used unconditional close")
        };
        assert!(Arc::ptr_eq(&slot, &original));
        assert!(!Arc::ptr_eq(&slot, &replacement));
        resp.send(Ok(false)).unwrap();
        assert!(launch.await.is_err());
        assert!(driver.is_running(&profile.id));
        assert!(driver.registry.is_current(&profile.id, &replacement).await);
    }
    driver.launcher_tx = real_tx;
    driver.close(&profile.id).await.unwrap();
    driver.shutdown().await;
}

#[tokio::test]
async fn launcher_rejects_old_generation_rollback_after_reopen() {
    use crate::driver::{
        business_tests::{fixture, launch_without_cdp},
        LauncherCmd,
    };
    use mcp_server::driver::BrowserDriver;
    let (_temp, driver) = fixture(multizen_core::ChromixSettings::default());
    let profile = driver
        .create_profile(multizen_core::CreateProfileInput {
            name: "P".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let first = launch_without_cdp(&driver, &profile).await.unwrap();
    let old = driver
        .registry
        .prepared_slot(&profile.id, &format!("{}:{}", first.started_at, first.pid))
        .await
        .unwrap();
    driver.close(&profile.id).await.unwrap();
    let second = launch_without_cdp(&driver, &profile).await.unwrap();
    let current = driver
        .registry
        .prepared_slot(
            &profile.id,
            &format!("{}:{}", second.started_at, second.pid),
        )
        .await
        .unwrap();
    let (resp, receive) = tokio::sync::oneshot::channel();
    driver
        .launcher_tx
        .send(LauncherCmd::ClosePrepared {
            profile_id: profile.id.clone(),
            slot: old,
            resp,
        })
        .await
        .unwrap();
    assert!(!receive.await.unwrap().unwrap());
    assert!(driver.registry.is_current(&profile.id, &current).await);
    let still_running = launch_without_cdp(&driver, &profile).await.unwrap();
    assert_eq!(still_running.pid, second.pid);
    assert_eq!(still_running.started_at, second.started_at);
    let (resp, receive) = tokio::sync::oneshot::channel();
    driver
        .launcher_tx
        .send(LauncherCmd::ClosePrepared {
            profile_id: profile.id.clone(),
            slot: current.clone(),
            resp,
        })
        .await
        .unwrap();
    assert!(receive.await.unwrap().unwrap());
    assert!(current.cancel.is_cancelled());
    assert!(driver
        .registry
        .prepared_slot(
            &profile.id,
            &format!("{}:{}", second.started_at, second.pid)
        )
        .await
        .is_err());
    driver.shutdown().await;
}

#[tokio::test]
async fn late_close_reply_does_not_clear_reopened_running_cache() {
    use crate::driver::{business_tests::fixture, LauncherCmd};
    use mcp_server::driver::BrowserDriver;
    let (_temp, mut driver) = fixture(multizen_core::ChromixSettings::default());
    let real_tx = driver.launcher_tx.clone();
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    driver.launcher_tx = tx;
    driver
        .registry
        .prepare(
            "p",
            "http://127.0.0.1:1",
            multizen_core::BrowserEngine::Chromix,
            "old",
            None,
        )
        .await;
    driver.running.lock().unwrap().insert("p".into());
    {
        let close = driver.close("p");
        tokio::pin!(close);
        poll_pending(close.as_mut()).await;
        let LauncherCmd::Close { resp, .. } = rx.recv().await.unwrap() else {
            panic!("expected close")
        };
        let replacement = driver
            .registry
            .prepare(
                "p",
                "http://127.0.0.1:1",
                multizen_core::BrowserEngine::Chromix,
                "new",
                None,
            )
            .await;
        resp.send(Ok(())).unwrap();
        close.await.unwrap();
        assert!(driver.is_running("p"));
        assert!(driver.registry.is_current("p", &replacement).await);
    }
    driver.launcher_tx = real_tx;
    driver.shutdown().await;
}

#[tokio::test]
async fn launcher_persistence_discards_close_relaunch_and_expired_results() {
    use crate::driver::business_tests::{fixture, launch_without_cdp};
    use mcp_server::driver::BrowserDriver;
    let (_temp, driver) = fixture(multizen_core::ChromixSettings::default());
    let p = driver
        .create_profile(multizen_core::CreateProfileInput {
            name: "P".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let launched = launch_without_cdp(&driver, &p).await.unwrap();
    let slot = driver
        .registry
        .prepare(
            &p.id,
            &launched.cdp_endpoint,
            multizen_core::BrowserEngine::Chromix,
            &format!("{}:{}", launched.started_at, launched.pid),
            None,
        )
        .await;
    let mut snapshot = Snapshot::empty(&p.id, Status::Detected);
    snapshot.platform_user_id = Some("12345".into());
    snapshot.checked_at = Some("2026-09-30T00:00:00Z".into());
    let data = Observation {
        snapshot,
        session_id: Some(slot.id.clone()),
        avatar_url: None,
    };
    let expected = BusinessProfileState {
        account: None,
        scope: None,
    };
    // Actual launcher-thread persistence path, using the shared protocol-only fake launcher.
    let result = driver
        .identity_request(|resp| IdentityCmd::Save {
            data: data.clone(),
            expected: expected.clone(),
            slot: slot.clone(),
            stop: driver.identity.stop.clone(),
            deadline: Instant::now() + COMMAND_WAIT,
            resp,
        })
        .await
        .unwrap();
    assert_eq!(result.unwrap().snapshot.status, Status::Detected);
    driver.close(&p.id).await.unwrap();
    let result = driver
        .identity_request(|resp| IdentityCmd::Save {
            data: data.clone(),
            expected: expected.clone(),
            slot: slot.clone(),
            stop: driver.identity.stop.clone(),
            deadline: Instant::now() + COMMAND_WAIT,
            resp,
        })
        .await
        .unwrap();
    assert!(result.is_none());
    let next = launch_without_cdp(&driver, &p).await.unwrap();
    let new_slot = driver
        .registry
        .prepare(
            &p.id,
            &next.cdp_endpoint,
            multizen_core::BrowserEngine::Chromix,
            &format!("{}:{}", next.started_at, next.pid),
            None,
        )
        .await;
    assert_ne!(slot.id, new_slot.id);
    let result = driver
        .identity_request(|resp| IdentityCmd::Save {
            data: data.clone(),
            expected: expected.clone(),
            slot: slot.clone(),
            stop: driver.identity.stop.clone(),
            deadline: Instant::now() + COMMAND_WAIT,
            resp,
        })
        .await
        .unwrap();
    assert!(result.is_none());
    let result = driver
        .identity_request(|resp| IdentityCmd::Save {
            data: data.clone(),
            expected: expected.clone(),
            slot: new_slot,
            stop: driver.identity.stop.clone(),
            deadline: Instant::now() - Duration::from_secs(1),
            resp,
        })
        .await
        .unwrap();
    assert!(result.is_none());
    driver.close(&p.id).await.unwrap();
    driver.delete_profile(&p.id).await.unwrap();
    assert!(driver.kuaishou_identity_list().await.unwrap().is_empty());
    driver.shutdown().await;
}

#[test]
fn tick_scheduling_drops_backlog_rotates_and_stops() {
    let runtime = Arc::new(IdentityRuntime::new(PathBuf::from("unused-test-cache")));
    let ids: Vec<_> = (0..8).map(|n| format!("p{n}")).collect();
    let mut cursor = 0;
    let first = runtime.schedule(ids.clone(), &mut cursor);
    assert_eq!(first.len(), 4);
    assert!(runtime.schedule(ids.clone(), &mut cursor).is_empty());
    let first_ids: Vec<_> = first.iter().map(|g| g.id.clone()).collect();
    drop(first);
    let next = runtime.schedule(ids.clone(), &mut cursor);
    assert_eq!(next.len(), 4);
    assert_eq!(runtime.active.lock().unwrap().len(), 4);
    drop(next);
    cursor = 4;
    let rotated = runtime.schedule(ids.clone(), &mut cursor);
    assert!(rotated.iter().all(|g| !first_ids.contains(&g.id)));
    drop(rotated);
    runtime.stop.cancel();
    assert!(runtime.schedule(ids, &mut cursor).is_empty());
}
#[tokio::test]
async fn monitor_is_once_only_and_does_not_keep_driver_alive() {
    let temp = tempfile::TempDir::new().unwrap();
    let driver = Arc::new(
        TauriBrowserDriver::start(
            temp.path().join("p.db"),
            temp.path().join("profiles"),
            temp.path().join("extensions"),
            Arc::new(ProfileRegistry::new()),
            multizen_core::BrowserEngine::Chromix,
            PathBuf::new(),
            None,
        )
        .unwrap(),
    );
    driver.start_kuaishou_monitors();
    driver.start_kuaishou_monitors();
    assert!(driver.identity.started.load(Ordering::Acquire));
    let weak = Arc::downgrade(&driver);
    let stop = driver.identity.stop.clone();
    let init_stop = driver.account_init.stop.clone();
    drop(driver);
    tokio::time::timeout(Duration::from_secs(1), async {
        while weak.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(stop.is_cancelled());
    assert!(init_stop.is_cancelled());
}
#[tokio::test]
async fn pending_and_granted_initialization_do_not_consume_identity_worker_capacity() {
    let runtime = Arc::new(IdentityRuntime::new(PathBuf::from("unused-test-cache")));
    let detector = runtime.enter("a").unwrap();
    let admission = runtime.reserve_initialization_wait("a");
    tokio::pin!(admission);
    poll_pending(admission.as_mut()).await;
    assert_eq!(runtime.permits.available_permits(), MAX_CONCURRENT - 1);
    drop(detector);
    assert!(runtime.enter("a").is_none(), "detector must not overtake the fair waiter");
    let initialized = tokio::time::timeout(Duration::from_secs(1), admission.as_mut()).await.unwrap().unwrap();
    assert_eq!(runtime.permits.available_permits(), MAX_CONCURRENT);
    let others: Vec<_> = ["b", "c", "d", "e"].into_iter().map(|id| runtime.enter(id).unwrap()).collect();
    assert!(runtime.enter("overflow").is_none());
    assert!(runtime.enter("a").is_none());
    drop(others);
    drop(initialized);
    assert!(runtime.enter("a").is_some());
}

#[test]
fn profile_admission_registry_uses_weak_ownership_and_prunes_idle_entries() {
    let runtime = Arc::new(IdentityRuntime::new(PathBuf::from("unused-test-cache")));
    let first = runtime.profile_lock("old");
    let same = runtime.profile_lock("old");
    assert!(Arc::ptr_eq(&first, &same));
    drop(first); drop(same);
    let _new = runtime.profile_lock("new");
    assert_eq!(runtime.profile_locks.lock().unwrap().len(), 1);
}

#[test]
fn shared_gate_is_bounded_non_reentrant_and_releases_on_drop() {
    let runtime = Arc::new(IdentityRuntime::new(PathBuf::from("unused-test-cache")));
    let a = runtime.enter("a").unwrap();
    assert!(runtime.enter("a").is_none());
    let _b = runtime.enter("b").unwrap();
    let _c = runtime.enter("c").unwrap();
    let _d = runtime.enter("d").unwrap();
    assert!(runtime.enter("e").is_none());
    drop(a);
    assert!(runtime.enter("a").is_some());
    runtime.stop.cancel();
    assert!(runtime.enter("z").is_none());
}
#[tokio::test]
async fn cached_detected_requires_same_session_and_closed_keeps_history() {
    let reg = ProfileRegistry::new();
    let slot = reg
        .prepare(
            "p",
            "http://127.0.0.1:1",
            multizen_core::BrowserEngine::Chromix,
            "first",
            None,
        )
        .await;
    let mut snapshot = Snapshot::empty("p", Status::Detected);
    snapshot.platform_user_id = Some("12345".into());
    let old = Observation {
        snapshot,
        session_id: Some(slot.id.clone()),
        avatar_url: None,
    };
    let business = BusinessProfileState {
        account: None,
        scope: None,
    };
    assert_eq!(
        project("p", Some(&old), &business, Some(&slot)).status,
        Status::Detected
    );
    let next = reg
        .prepare(
            "p",
            "http://127.0.0.1:1",
            multizen_core::BrowserEngine::Chromix,
            "second",
            None,
        )
        .await;
    assert_eq!(
        project("p", Some(&old), &business, Some(&next)).status,
        Status::Unknown
    );
    let closed = project("p", Some(&old), &business, None);
    assert_eq!(closed.status, Status::Closed);
    assert_eq!(closed.platform_user_id.as_deref(), Some("12345"));
    let jinniu = BusinessProfileState {
        account: None,
        scope: Some(multizen_core::BusinessProfileScope::Jinniu),
    };
    assert_eq!(
        project("p", Some(&old), &jinniu, Some(&next)).status,
        Status::Skipped
    );
}
#[tokio::test]
async fn manual_closed_profile_never_launches_and_avatar_is_db_gated() {
    let temp = tempfile::TempDir::new().unwrap();
    let driver = TauriBrowserDriver::start(
        temp.path().join("p.db"),
        temp.path().join("profiles"),
        temp.path().join("extensions"),
        Arc::new(ProfileRegistry::new()),
        multizen_core::BrowserEngine::Chromix,
        PathBuf::new(),
        None,
    )
    .unwrap();
    let p = driver
        .create_profile(multizen_core::CreateProfileInput {
            name: "P".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        driver.kuaishou_identity_detect(&p.id).await.status,
        Status::Closed
    );
    assert!(driver.registry.ids().await.is_empty());
    assert_eq!(
        driver.kuaishou_identity_list().await.unwrap()[0].status,
        Status::Closed
    );
    assert!(driver.kuaishou_identity_avatar("../secret").await.is_none());
    assert!(driver
        .kuaishou_identity_avatar(&format!("{}.gif", "a".repeat(64)))
        .await
        .is_none());
    driver.shutdown().await;
}
