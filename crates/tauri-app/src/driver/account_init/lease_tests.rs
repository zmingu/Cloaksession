use super::*;
use multizen_core::*;
use profile_manager::ProfileManager;

pub(in crate::driver::account_init) fn fixture() -> (tempfile::TempDir, ProfileManager, KuaishouInitContext) {
    let dir = tempfile::tempdir().unwrap();
    let pm = ProfileManager::new(&dir.path().join("test.db"), &dir.path().join("profiles")).unwrap();
    let profile = pm.create(CreateProfileInput { name: "cleanup fixture".into(), ..Default::default() }).unwrap();
    let business = pm.business_accounts_profile_state(&profile.id).unwrap();
    let mut snapshot = KuaishouIdentitySnapshot::empty(&profile.id, KuaishouIdentityStatus::Detected);
    snapshot.platform_user_id = Some("12345".into());
    snapshot.checked_at = Some("2026-10-01T00:00:00Z".into());
    pm.kuaishou_identity_save(KuaishouIdentityObservation { snapshot, session_id: Some("session".into()), avatar_url: None }, &business).unwrap();
    let context = KuaishouInitContext { platform_user_id: "12345".into(), profile_id: profile.id, session_id: "session".into(), expected_business: business };
    (dir, pm, context)
}
async fn next(receiver: &mut mpsc::Receiver<LauncherCmd>) -> InitCmd {
    match tokio::time::timeout(Duration::from_secs(2), receiver.recv()).await.unwrap().unwrap() {
        LauncherCmd::Init(command) => command,
        _ => panic!("unexpected launcher command"),
    }
}

#[tokio::test]
async fn lost_claim_reply_before_and_after_delivery_releases_original_token() {
    for delivered in [false, true] {
        let (_dir, pm, context) = fixture();
        let lease = pm.kuaishou_init_claim(&context, KuaishouInitStep::Subject, true).unwrap().unwrap();
        let (launcher, mut receiver) = mpsc::channel(2);
        let (reply, receive) = oneshot::channel();
        let owned = RunningLease::new(lease, launcher);
        if delivered {
            assert!(reply.send(owned).is_ok());
            drop(receive); // Claim committed and reply queued, but caller never received it.
        } else {
            drop(receive);
            drop(reply.send(owned)); // Caller left before the claim reply was sent.
        }
        let command = next(&mut receiver).await;
        (command.operation)(&pm, false); // Late cleanup is allowed, unlike a new claim.
        let steps = pm.kuaishou_init_steps("12345").unwrap();
        let subject = steps.iter().find(|s| s.step == KuaishouInitStep::Subject).unwrap();
        assert_eq!(subject.state, KuaishouInitState::Failed);
        assert_eq!(subject.last_error_code, Some(KuaishouInitErrorCode::InterruptedNeedsVerification));
    }
}

#[tokio::test]
async fn dropped_release_waiter_does_not_cancel_token_cleanup_or_revoke_successor() {
    let (_dir, pm, context) = fixture();
    let lease = pm.kuaishou_init_claim(&context, KuaishouInitStep::Slice, true).unwrap().unwrap();
    let (launcher, mut receiver) = mpsc::channel(2);
    let old = lease.clone();
    let task = tokio::spawn(async move { release(&launcher, &old, KuaishouInitErrorCode::TimedOut).await });
    let command = next(&mut receiver).await;
    task.abort();
    let _ = task.await;
    (command.operation)(&pm, false);
    let successor = pm.kuaishou_init_claim(&context, KuaishouInitStep::Slice, true).unwrap().unwrap();
    assert!(!pm.kuaishou_init_fail(&lease, KuaishouInitErrorCode::InterruptedNeedsVerification).unwrap());
    pm.kuaishou_init_complete_slice(&successor, &SliceVerification {
        platform_user_id: "12345".into(), all_four_disabled: [true; 4], persisted_readback: true,
    }).unwrap();
    assert!(!pm.kuaishou_init_fail(&successor, KuaishouInitErrorCode::TimedOut).unwrap());
}

#[tokio::test]
async fn release_retries_a_lost_command_without_opening_another_database() {
    let (_dir, pm, context) = fixture();
    let lease = pm.kuaishou_init_claim(&context, KuaishouInitStep::Subject, true).unwrap().unwrap();
    let (launcher, mut receiver) = mpsc::channel(2);
    let task = tokio::spawn(async move { release(&launcher, &lease, KuaishouInitErrorCode::OcrFailed).await });
    drop(next(&mut receiver).await); // Lost operation/reply; retry must retain the same token.
    let retry = next(&mut receiver).await;
    (retry.operation)(&pm, true);
    assert!(task.await.unwrap());
    let subject = pm.kuaishou_init_steps("12345").unwrap().into_iter().find(|s| s.step == KuaishouInitStep::Subject).unwrap();
    assert_eq!(subject.state, KuaishouInitState::Failed);
    assert_eq!(subject.last_error_code, Some(KuaishouInitErrorCode::OcrFailed));
    assert_eq!(subject.attempts, 1);
}
