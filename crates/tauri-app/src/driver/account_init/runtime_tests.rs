use super::*;
use chromiumoxide::cdp::browser_protocol::target::GetTargetsParams;
#[path = "../../../../cdp-driver/tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn identity_first_ticks_handoff_to_unfinished_initialization() {
    let (_temp, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
    let driver = Arc::new(driver);
    let profile = driver
        .create_profile(CreateProfileInput {
            name: "fair admission fixture".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let launched = crate::driver::business_tests::launch_without_cdp(&driver, &profile)
        .await
        .unwrap();
    let slot = driver
        .registry
        .prepared_slot(
            &profile.id,
            &format!("{}:{}", launched.started_at, launched.pid),
        )
        .await
        .unwrap();
    let (peer, session) = common::Peer::connect().await;
    for target in ["a", "b"] {
        session
            .bind_page(target)
            .await
            .unwrap()
            .navigate(page::SUBJECT_URL, 1000)
            .await
            .unwrap();
    }
    slot.install_test_session(Arc::new(session));
    let mut snapshot =
        KuaishouIdentitySnapshot::empty(&profile.id, KuaishouIdentityStatus::Detected);
    snapshot.platform_user_id = Some("12345".into());
    snapshot.checked_at = Some(chrono::Utc::now().to_rfc3339());
    let observation = KuaishouIdentityObservation {
        snapshot,
        session_id: Some(slot.id.clone()),
        avatar_url: None,
    };
    driver
        .init_db(None, move |pm| {
            pm.kuaishou_identity_save(
                observation,
                &BusinessProfileState {
                    account: None,
                    scope: None,
                },
            )
        })
        .await
        .unwrap()
        .unwrap();
    for attempt in 1..=3 {
        // Force identity-first ordering on every episode. No timers, sleeps or jitter
        // decide who wins; poll initialization exactly to its admission suspension.
        let identity = driver.identity.test_hold_detection(&profile.id).unwrap();
        let admission = driver.init_enter_wait(&profile.id);
        tokio::pin!(admission);
        std::future::poll_fn(|cx| {
            assert!(
                std::future::Future::poll(admission.as_mut(), cx).is_pending(),
                "initialization skipped identity-first admission instead of waiting for completion"
            );
            std::task::Poll::Ready(())
        })
        .await;
        drop(identity);
        assert!(
            driver.identity.test_hold_detection(&profile.id).is_none(),
            "identity overtook the pending initializer"
        );
        let active = tokio::time::timeout(Duration::from_secs(2), admission.as_mut())
            .await
            .unwrap()
            .expect("initialization admission lost");
        peer.evaluations((0..2).map(|_| common::EvalReply::Value(serde_json::json!({
            "url": page::SUBJECT_URL, "platformUserId": "12345", "nickname": null, "avatarUrl": null,
        }))).collect());
        // Fresh all-target identity verification remains AFTER admission. Exercise the
        // real launcher-thread claim, not merely an in-memory scheduled flag.
        let guard = driver.init_context(&profile.id).await.unwrap();
        let context = guard.context.clone();
        let lease = driver
            .init_db(Some(guard), move |pm| {
                pm.kuaishou_init_claim(&context, KuaishouInitStep::Subject, true)
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.attempts(), attempt);
        driver
            .init_db(None, move |pm| {
                pm.kuaishou_init_fail(&lease, Code::PersistenceUnverified)
            })
            .await
            .unwrap();
        drop(active);
    }
    driver.shutdown().await;
}

async fn pending<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn pending_initialization_deadline_and_cancellation_release_capacity() {
    for mode in ["deadline", "init-stop", "identity-stop"] {
        let (_temp, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
        let identity = driver.identity.test_hold_detection("p").unwrap();
        let active = driver.init_slot("p").unwrap();
        assert!(driver.init_slot("p").is_none()); // no second waiter for the same profile
        let admission = driver.init_admit(
            active,
            if mode == "deadline" {
                Duration::ZERO
            } else {
                Duration::from_secs(20)
            },
        );
        tokio::pin!(admission);
        if mode != "deadline" {
            pending(admission.as_mut()).await;
        }
        if mode == "init-stop" {
            driver.stop_kuaishou_init_monitor();
        }
        if mode == "identity-stop" {
            driver.identity.stop.cancel();
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(1), admission.as_mut())
                .await
                .unwrap()
                .is_none()
        );
        assert!(driver.account_init.active.lock().unwrap().is_empty());
        assert!(driver.identity.test_hold_detection("p").is_none());
        drop(identity);
        assert_eq!(
            driver.identity.test_hold_detection("p").is_some(),
            mode != "identity-stop",
            "only identity shutdown may disable subsequent detection"
        );
        driver.shutdown().await;
    }
}

#[tokio::test]
async fn dropped_waiter_before_or_after_handoff_releases_both_gates() {
    let (_temp, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
    for grant_before_drop in [false, true] {
        let mut identity = Some(driver.identity.test_hold_detection("p").unwrap());
        {
            let admission = driver.init_enter_wait("p");
            tokio::pin!(admission);
            pending(admission.as_mut()).await;
            if grant_before_drop {
                drop(identity.take());
            }
            // Drop without polling the newly granted permit: it must not strand ownership.
        }
        assert!(driver.account_init.active.lock().unwrap().is_empty());
        if !grant_before_drop {
            assert!(driver.identity.test_hold_detection("p").is_none());
        }
        drop(identity);
        assert!(driver.identity.test_hold_detection("p").is_some());
    }
    let slots: Vec<_> = ["a", "b", "c", "d"]
        .into_iter()
        .map(|id| driver.init_slot(id).unwrap())
        .collect();
    assert!(
        driver.init_slot("overflow").is_none(),
        "pending initialization must stay bounded to four"
    );
    drop(slots);
    assert!(driver.account_init.active.lock().unwrap().is_empty());
    driver.shutdown().await;
}

#[tokio::test]
async fn app_startup_enables_initialization_once_without_launching_or_claiming_idle_profiles() {
    let (_temp, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
    let driver = Arc::new(driver);
    assert!(!driver.account_init.started.load(Ordering::Acquire));
    driver.start_kuaishou_monitors();
    driver.start_kuaishou_monitors();
    assert!(driver.account_init.started.load(Ordering::Acquire));
    // An empty initialized-session registry must not cause browser creation or claims.
    assert!(driver.account_init.active.lock().unwrap().is_empty());
    assert!(driver.account_init.pages.lock().unwrap().is_empty());
    assert!(driver.account_init.runs.lock().unwrap().is_empty());
    driver.shutdown().await;
    assert!(driver.identity.stop.is_cancelled());
    assert!(driver.account_init.stop.is_cancelled());
    driver.start_kuaishou_monitors(); // one-shot cancellation/start flags cannot restart work
    assert!(driver.account_init.stop.is_cancelled());
}

#[tokio::test]
async fn present_but_unresponsive_owned_target_is_rebuilt_and_classified_as_crash() {
    let (_temp, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
    let driver = Arc::new(driver);
    let profile = driver
        .create_profile(CreateProfileInput {
            name: "crashed renderer fixture".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let launched = crate::driver::business_tests::launch_without_cdp(&driver, &profile)
        .await
        .unwrap();
    let slot = driver
        .registry
        .prepared_slot(
            &profile.id,
            &format!("{}:{}", launched.started_at, launched.pid),
        )
        .await
        .unwrap();
    let (peer, session) = common::Peer::connect().await;
    let session = Arc::new(session);
    slot.install_test_session(session.clone());
    let business = BusinessProfileState {
        account: None,
        scope: None,
    };
    let mut snapshot =
        KuaishouIdentitySnapshot::empty(&profile.id, KuaishouIdentityStatus::Detected);
    snapshot.platform_user_id = Some("12345".into());
    snapshot.checked_at = Some(chrono::Utc::now().to_rfc3339());
    let observation = KuaishouIdentityObservation {
        snapshot,
        session_id: Some(slot.id.clone()),
        avatar_url: None,
    };
    let expected = business.clone();
    driver
        .init_db(None, move |pm| {
            pm.kuaishou_identity_save(observation, &expected)?;
            Ok(())
        })
        .await
        .unwrap();
    let guard = Guard {
        context: KuaishouInitContext {
            platform_user_id: "12345".into(),
            profile_id: profile.id.clone(),
            session_id: slot.id.clone(),
            expected_business: business,
        },
        slot: slot.clone(),
    };
    let _reservation = driver
        .identity
        .reserve_initialization(&guard.context.profile_id)
        .unwrap();
    let inventory = async || {
        session
            .browser
            .execute(GetTargetsParams::default())
            .await
            .unwrap()
            .result
            .target_infos
            .len()
    };

    // Target present AND responsive: reused unchanged, no target created or adopted.
    driver.account_init.test_record_known_target(&slot.id, "a");
    let before = inventory().await;
    assert_eq!(
        driver
            .init_owned_target(&guard, KuaishouInitStep::Slice, &session)
            .await
            .unwrap(),
        "a"
    );
    assert_eq!(
        driver.account_init.test_owned_record(&slot.id),
        Some(Some("a".into()))
    );
    assert_eq!(
        inventory().await,
        before,
        "a live owned target must not be recreated"
    );

    // Present in inventory but its renderer never replies: proven-missing and rebuilt.
    peer.stall("Runtime.evaluate", None);
    let rebuilt = driver
        .init_owned_target(&guard, KuaishouInitStep::Slice, &session)
        .await
        .unwrap();
    assert_ne!(rebuilt, "a", "a dead renderer must never be reused");
    assert_eq!(
        driver.account_init.test_owned_record(&slot.id),
        Some(Some(rebuilt))
    );

    // Present but unresponsive AND rebuild cannot complete: the honest code is the crash,
    // never page-unsupported or context-rejected.
    peer.stall("Runtime.evaluate", None);
    peer.fail("Target.createTarget", None);
    assert!(matches!(
        driver
            .init_owned_target(&guard, KuaishouInitStep::Slice, &session)
            .await,
        Err(Code::PageCrashed)
    ));
    assert_eq!(driver.account_init.test_owned_record(&slot.id), Some(None));
    driver.shutdown().await;
}

#[test]
fn missing_target_reconciliation_requires_the_exact_known_record() {
    let mut pages = HashMap::new();
    pages.insert("session".into(), Some("owned".into()));
    assert!(!forget_proven_missing(&mut pages, "other-session", "owned"));
    assert!(!forget_proven_missing(&mut pages, "session", "user-page"));
    assert_eq!(pages.get("session"), Some(&Some("owned".into())));
    assert!(forget_proven_missing(&mut pages, "session", "owned"));
    assert!(!pages.contains_key("session")); // only now is a new owned creation eligible
    pages.insert("session".into(), None);
    assert!(!forget_proven_missing(&mut pages, "session", "owned"));
    assert_eq!(pages.get("session"), Some(&None)); // unknown completion is never promoted to missing
    assert_eq!(
        initial_page_url(KuaishouInitStep::Subject),
        page::SUBJECT_URL
    );
    assert_eq!(initial_page_url(KuaishouInitStep::Slice), page::SLICE_URL);
}

#[test]
fn shared_profile_reservation_does_not_relax_commit_context_validation() {
    for change in ["account", "session", "binding", "delete"] {
        let (_dir, pm, context) = super::super::lease::tests::fixture();
        assert!(super::super::validate(&pm, &context).is_ok());
        match change {
            "account" | "session" => {
                let mut observed = pm
                    .kuaishou_identity_observation(&context.profile_id)
                    .unwrap()
                    .unwrap();
                if change == "account" {
                    observed.snapshot.platform_user_id = Some("23456".into());
                } else {
                    observed.session_id = Some("replacement-session".into());
                }
                pm.kuaishou_identity_save(observed, &context.expected_business)
                    .unwrap()
                    .unwrap();
            }
            "binding" => {
                pm.business_accounts_save(SaveBusinessAccountInput {
                    id: None,
                    kind: BusinessAccountKind::KuaishouShop,
                    profile_id: context.profile_id.clone(),
                    display_name: "manual change".into(),
                    platform_user_id: Some("12345".into()),
                })
                .unwrap();
            }
            "delete" => pm.delete(&context.profile_id).unwrap(),
            _ => unreachable!(),
        }
        assert!(super::super::validate(&pm, &context).is_err(), "{change}");
    }
}

#[test]
fn retry_budget_is_per_account_generation_and_drop_releases_profile_gate() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(InitRuntime::new(temp.path().join("attachments")));
    runtime
        .runs
        .lock()
        .unwrap()
        .insert(("session-a".into(), "12345".into()), 3);
    assert!(!runtime
        .runs
        .lock()
        .unwrap()
        .contains_key(&("session-a".into(), "23456".into())));
    assert!(!runtime
        .runs
        .lock()
        .unwrap()
        .contains_key(&("session-b".into(), "12345".into())));
    runtime.active.lock().unwrap().insert("profile".into());
    let identity = Arc::new(crate::driver::identity::IdentityRuntime::new(
        temp.path().join("avatars"),
    ));
    let reservation = identity.reserve_initialization("profile").unwrap();
    let active = Active {
        runtime: runtime.clone(),
        id: "profile".into(),
        _identity: Some(reservation),
    };
    drop(active);
    assert!(runtime.active.lock().unwrap().is_empty());
}

#[tokio::test]
async fn wake_scan_returns_immediately_consumes_one_permit_and_honors_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = InitRuntime::new(temp.path().join("attachments"));
    let mut interval = tokio::time::interval(Duration::from_secs(3600));
    // The interval's first tick is already due, so this scan is timer-driven.
    assert!(runtime.wait_for_scan(&mut interval).await);
    // No permit: the next scan blocks until a wake (proved by the surrounding timeout).
    assert!(
        tokio::time::timeout(Duration::from_millis(50), runtime.wait_for_scan(&mut interval))
            .await
            .is_err()
    );
    // One wake, one permit: the next scan returns at once and a second waits again.
    runtime.wake();
    assert!(runtime.wait_for_scan(&mut interval).await);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), runtime.wait_for_scan(&mut interval))
            .await
            .is_err()
    );
    // Cancellation wins over both arms and stops the monitor.
    runtime.stop.cancel();
    assert!(!runtime.wait_for_scan(&mut interval).await);
}

#[tokio::test]
async fn monitor_wake_shortens_the_scan_window_without_launching_or_claiming_idle_profiles() {
    let (_temp, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
    let driver = Arc::new(driver);
    driver.start_kuaishou_init_monitor();
    // A second start cannot spawn a rival loop (once guard).
    driver.start_kuaishou_init_monitor();
    // Let the loop enter wait_for_scan (its first tick already fired; reset hides it).
    tokio::time::sleep(Duration::from_millis(50)).await;
    driver.account_init.test_reset_scanned();
    // A wake schedules the same scan the 5s tick would; the empty registry means no
    // launch, no claim and no browser is created for it.
    driver.account_init.wake();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !driver.account_init.test_scanned() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("a wake must schedule a scan well before the 5s tick");
    assert!(driver.account_init.active.lock().unwrap().is_empty());
    assert!(driver.account_init.pages.lock().unwrap().is_empty());
    assert!(driver.account_init.runs.lock().unwrap().is_empty());
    driver.shutdown().await;
    assert!(driver.account_init.stop.is_cancelled());
}
