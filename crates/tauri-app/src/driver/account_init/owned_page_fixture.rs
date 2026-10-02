//! Opt-in actual Chromix target-lifecycle regression. No platform content is read.
//! HTTPS navigation is rejected by an owned, non-forwarding proxy; only target
//! creation/inventory/close are under test. Identity/action acceptance is separate.
use super::*;
use chromiumoxide::cdp::browser_protocol::target::GetTargetsParams;
use std::collections::BTreeSet;
use tokio::io::AsyncWriteExt;

struct RejectTraffic(tokio::task::JoinHandle<()>);
impl Drop for RejectTraffic {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn ids(session: &BrowserSession) -> BTreeSet<String> {
    stage(
        "owned-inventory-control",
        session.browser.execute(GetTargetsParams::default()),
    )
    .await
    .result
    .target_infos
    .into_iter()
    .map(|info| info.target_id.as_ref().to_owned())
    .collect()
}

#[tokio::test]
#[ignore = "Chromix-only owned-target regression; explicit parent opt-in"]
async fn chromix_closed_owned_target_recovery() {
    assert!(
        tokio::time::timeout(Duration::from_secs(90), run_owned_recovery())
            .await
            .is_ok(),
        "owned recovery fixture deadline"
    );
}
async fn run_owned_recovery() {
    let mut browser = OwnedChromix::start();
    let sink = browser._sink.try_clone().unwrap();
    sink.set_nonblocking(true).unwrap();
    let sink = tokio::net::TcpListener::from_std(sink).unwrap();
    let _traffic = RejectTraffic(tokio::spawn(async move {
        while let Ok((mut stream, _)) = sink.accept().await {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
        }
    }));
    let endpoint = browser.endpoint().await;
    let session = Arc::new(
        stage(
            "owned-fixture-connect",
            BrowserSession::connect(&endpoint, BrowserEngine::Chromix),
        )
        .await,
    );
    let user = stage("owned-user-page", session.new_bound_page("about:blank")).await;
    let user_id = user.target_id().to_owned();
    let (_data, driver) = fixture(ChromixSettings::default());
    let driver = Arc::new(driver);
    let profile = stage(
        "owned-fixture-profile",
        driver.create_profile(CreateProfileInput {
            name: "owned target fixture".into(),
            ..Default::default()
        }),
    )
    .await;
    let launched = stage(
        "owned-fixture-launcher",
        launch_without_cdp(&driver, &profile),
    )
    .await;
    let slot = stage(
        "owned-fixture-slot",
        driver.registry.prepared_slot(
            &profile.id,
            &format!("{}:{}", launched.started_at, launched.pid),
        ),
    )
    .await;
    slot.install_test_session(session.clone());
    let expected = BusinessProfileState {
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
    let business = expected.clone();
    stage(
        "owned-fixture-observation",
        driver.init_db(None, move |pm| {
            pm.kuaishou_identity_save(observation, &business)?;
            Ok(())
        }),
    )
    .await;
    let guard = Guard {
        context: KuaishouInitContext {
            platform_user_id: "12345".into(),
            profile_id: profile.id,
            session_id: slot.id.clone(),
            expected_business: expected,
        },
        slot: slot.clone(),
    };
    let _reservation = driver
        .identity
        .reserve_initialization(&guard.context.profile_id)
        .unwrap();

    let first = stage(
        "owned-first-create",
        driver.init_owned_target(&guard, KuaishouInitStep::Slice, &session),
    )
    .await;
    assert!(
        first != user_id,
        "must create an owned page, not adopt a user page"
    );
    let before = ids(&session).await;
    assert!(
        stage(
            "owned-existing-reuse",
            driver.init_owned_target(&guard, KuaishouInitStep::Slice, &session)
        )
        .await
            == first
    );
    assert!(ids(&session).await == before);
    stage("owned-close-control", session.close_page(&first)).await;
    stage("owned-close-absence-proof", async {
        // A close reply is acknowledgement, not destruction. Each bounded poll reads
        // browser-level inventory; never infer absence from elapsed time or page cache.
        let mut ticks = tokio::time::interval(Duration::from_millis(25));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            let inventory = session
                .browser
                .execute(GetTargetsParams::default())
                .await
                .map_err(|_| ())?;
            if !inventory
                .result
                .target_infos
                .iter()
                .any(|info| info.target_id.as_ref() == first.as_str())
            {
                return Ok::<(), ()>(());
            }
        }
    })
    .await;
    let second = stage(
        "owned-proven-missing-recovery",
        driver.init_owned_target(&guard, KuaishouInitStep::Slice, &session),
    )
    .await;
    assert!(second != first);
    assert!(second != user_id);
    let inventory = ids(&session).await;
    assert!(
        inventory.contains(&user_id) && inventory.contains(&second) && !inventory.contains(&first)
    );

    driver.account_init.test_record_unknown_creation(&slot.id);
    assert!(matches!(
        driver
            .init_owned_target(&guard, KuaishouInitStep::Slice, &session)
            .await,
        Err(Code::InterruptedNeedsVerification)
    ));
    assert!(
        ids(&session).await == inventory,
        "unknown completion must not create/adopt any target"
    );
    assert!(driver.account_init.test_owned_record(&slot.id) == Some(None));

    // Restore this test's previously proven owned record to exercise inventory failure.
    driver
        .account_init
        .test_record_known_target(&slot.id, &second);
    browser.child.kill().unwrap();
    browser.child.wait().unwrap();
    assert!(driver
        .init_owned_target(&guard, KuaishouInitStep::Slice, &session)
        .await
        .is_err());
    assert!(
        driver.account_init.test_owned_record(&slot.id) == Some(Some(second)),
        "failed inventory is not evidence of absence"
    );
    driver.shutdown().await;
}
