use multizen_core::*;
use profile_manager::{migrate::run_migrations, KuaishouInitLease, ProfileManager};
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn manager(dir: &TempDir) -> ProfileManager {
    ProfileManager::new(&dir.path().join("test.db"), &dir.path().join("profiles")).unwrap()
}

fn observe(
    pm: &ProfileManager,
    profile: &str,
    account: &str,
    session: &str,
) -> KuaishouInitContext {
    let expected_business = pm.business_accounts_profile_state(profile).unwrap();
    let mut snapshot = KuaishouIdentitySnapshot::empty(profile, KuaishouIdentityStatus::Detected);
    snapshot.platform_user_id = Some(account.into());
    snapshot.nickname = Some("测试昵称".into());
    snapshot.avatar_key = Some(format!("{}.png", "c".repeat(64)));
    snapshot.checked_at = Some("2026-10-01T00:00:00Z".into());
    pm.kuaishou_identity_save(
        KuaishouIdentityObservation {
            snapshot,
            session_id: Some(session.into()),
            avatar_url: None,
        },
        &expected_business,
    )
    .unwrap()
    .unwrap();
    KuaishouInitContext {
        platform_user_id: account.into(),
        profile_id: profile.into(),
        session_id: session.into(),
        expected_business,
    }
}

fn account(pm: &ProfileManager, id: &str) -> KuaishouInitContext {
    let p = pm
        .create(CreateProfileInput {
            name: "临时测试环境".into(),
            ..Default::default()
        })
        .unwrap();
    observe(pm, &p.id, id, "test-generation")
}

fn fixture() -> (TempDir, ProfileManager, KuaishouInitContext) {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let context = account(&pm, "12345");
    (dir, pm, context)
}

fn image(hash: char) -> SubjectAttachment {
    let sha256 = hash.to_string().repeat(64);
    SubjectAttachment {
        key: format!("{sha256}.png"),
        sha256,
        mime_type: "image/png".into(),
        byte_len: 1024,
        width: 640,
        height: 400,
    }
}

fn candidate() -> SubjectCandidate {
    // Synthetic number: intentionally nonexistent region. Checks are structural, not legal authentication.
    let prefix = "99000120000229123";
    let weights = [7u32, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let sum: u32 = prefix
        .bytes()
        .zip(weights)
        .map(|(c, w)| u32::from(c - b'0') * w)
        .sum();
    SubjectCandidate {
        real_name: "合成样本".into(),
        id_card: format!("{prefix}{}", b"10X98765432"[(sum % 11) as usize] as char),
        source: SubjectSource::MainTab,
        evidence: SubjectVisibleEvidence {
            real_name: Some("合*本".into()),
            id_card: Some("990001************".into()),
        },
        attachments: vec![image('a'), image('b')],
    }
}

fn save(
    pm: &ProfileManager,
    context: &KuaishouInitContext,
) -> (KuaishouInitLease, KuaishouSubjectArchive) {
    let lease = pm
        .kuaishou_init_claim(context, KuaishouInitStep::Subject, false)
        .unwrap()
        .unwrap();
    let saved = pm
        .kuaishou_subject_save_candidate(&lease, 0, candidate())
        .unwrap();
    (lease, saved)
}

fn query(search: &str, offset: u32, limit: u32) -> KuaishouSubjectQuery {
    KuaishouSubjectQuery {
        search: search.into(),
        offset,
        limit,
    }
}

fn correction(saved: &KuaishouSubjectArchive) -> CorrectSubjectInput {
    CorrectSubjectInput {
        platform_user_id: saved.platform_user_id.clone(),
        expected_revision: saved.revision,
        real_name: saved.real_name.clone(),
        id_card: saved.id_card.clone(),
    }
}

#[test]
fn original_schema_repeat_migration_and_reopen_preserve_independent_documents() {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute_batch("CREATE TABLE profiles(id TEXT PRIMARY KEY,name TEXT NOT NULL,notes TEXT,tags TEXT NOT NULL DEFAULT '[]',proxy TEXT,fingerprint TEXT NOT NULL,data_dir TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,last_opened_at TEXT);").unwrap();
    run_migrations(&conn).unwrap();
    run_migrations(&conn).unwrap();
    let pm = manager(&dir);
    let context = account(&pm, "12345");
    let (lease, saved) = save(&pm, &context);
    pm.kuaishou_init_complete_subject(&lease, saved.revision, &saved.attachments)
        .unwrap();
    let document: String = conn
        .query_row("SELECT document FROM kuaishou_subject_archives", [], |r| {
            r.get(0)
        })
        .unwrap();
    run_migrations(&conn).unwrap();
    let after: String = conn
        .query_row("SELECT document FROM kuaishou_subject_archives", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(document, after);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM kuaishou_subject_archives", [], |r| {
            r.get::<_, u32>(0)
        })
        .unwrap(),
        1
    );
    drop(pm);
    let pm = manager(&dir);
    assert_eq!(
        pm.kuaishou_subject_detail("12345")
            .unwrap()
            .unwrap()
            .archive,
        saved
    );
    assert_eq!(pm.kuaishou_init_recover_interrupted().unwrap(), 0);
    assert_eq!(
        pm.kuaishou_init_steps("12345").unwrap()[0].state,
        KuaishouInitState::Done
    );
}

#[test]
fn profile_delete_retains_search_photos_steps_and_identity_without_manual_writes() {
    let (_dir, pm, context) = fixture();
    let (lease, saved) = save(&pm, &context);
    pm.kuaishou_init_complete_subject(&lease, saved.revision, &saved.attachments)
        .unwrap();
    assert!(pm.business_accounts_list().unwrap().is_empty());
    assert_eq!(
        pm.kuaishou_subject_list(&query("", 0, 20)).unwrap().items[0]
            .profiles
            .len(),
        1
    );
    pm.delete(&context.profile_id).unwrap();
    let detail = pm.kuaishou_subject_detail("12345").unwrap().unwrap();
    assert!(detail.profiles.is_empty());
    assert_eq!(detail.archive, saved);
    assert_eq!(detail.nickname.as_deref(), Some("测试昵称"));
    assert_eq!(detail.steps[0].state, KuaishouInitState::Done);
    assert!(pm
        .kuaishou_subject_attachment_referenced(&saved.attachments[0].key)
        .unwrap());
    assert!(!pm
        .kuaishou_subject_attachment_referenced("../anything.png")
        .unwrap());
    for term in ["合成", "12345", &saved.id_card] {
        let page = pm.kuaishou_subject_list(&query(term, 0, 20)).unwrap();
        assert_eq!(page.total, 1);
        let wire = serde_json::to_string(&page).unwrap();
        assert!(!wire.contains(&saved.id_card));
        assert!(!wire.contains("attachments"));
    }
    assert!(pm.business_accounts_list().unwrap().is_empty());
}

#[test]
fn search_is_literal_bounded_parameterized_and_has_stable_pagination() {
    let (_dir, pm, context) = fixture();
    let (_lease, _saved) = save(&pm, &context);
    let other = account(&pm, "23456");
    save(&pm, &other);
    for text in ["%", "_", "\\", "' OR 1=1 --"] {
        assert_eq!(
            pm.kuaishou_subject_list(&query(text, 0, 10)).unwrap().total,
            0
        );
    }
    let first = pm.kuaishou_subject_list(&query("", 0, 1)).unwrap();
    let second = pm.kuaishou_subject_list(&query("", 1, 1)).unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.items.len(), 1);
    assert_eq!(second.items.len(), 1);
    assert_ne!(
        first.items[0].platform_user_id,
        second.items[0].platform_user_id
    );
    assert!(pm
        .kuaishou_subject_list(&query("", u32::MAX, 1))
        .unwrap()
        .items
        .is_empty());
    assert!(pm.kuaishou_subject_list(&query("", 0, 0)).is_err());
    assert!(pm.kuaishou_subject_list(&query("", 0, 101)).is_err());
    assert!(pm
        .kuaishou_subject_list(&query(&"x".repeat(129), 0, 10))
        .is_err());
}

#[test]
fn revision_confirmation_and_ocr_recheck_fields_and_photo_version() {
    let (_dir, pm, context) = fixture();
    let (lease, saved) = save(&pm, &context);
    pm.kuaishou_init_complete_subject(&lease, saved.revision, &saved.attachments)
        .unwrap();
    let confirm = ConfirmSubjectInput {
        platform_user_id: "12345".into(),
        expected_revision: saved.revision,
    };
    assert!(pm.kuaishou_subject_confirm(confirm.clone(), &[]).is_err());
    let mut altered = saved.attachments.clone();
    altered[0].width += 1;
    assert!(pm
        .kuaishou_subject_confirm(confirm.clone(), &altered)
        .is_err());
    let confirmed = pm
        .kuaishou_subject_confirm(confirm.clone(), &saved.attachments)
        .unwrap();
    assert_eq!(confirmed.review_status, SubjectReviewStatus::Confirmed);
    assert_eq!(confirmed.revision, 2);
    assert!(confirmed.confirmed_at.is_some());
    assert!(pm
        .kuaishou_subject_confirm(confirm, &saved.attachments)
        .is_err());
    assert!(pm.kuaishou_subject_correct(correction(&saved)).is_err());
    let mut edit = correction(&confirmed);
    edit.id_card = "incorrect".into();
    let corrected = pm.kuaishou_subject_correct(edit).unwrap();
    assert_eq!(corrected.review_status, SubjectReviewStatus::PendingReview);
    assert!(corrected.confirmed_at.is_none());
    assert_eq!(corrected.evidence, saved.evidence);
    let error = pm
        .kuaishou_subject_confirm(
            ConfirmSubjectInput {
                platform_user_id: "12345".into(),
                expected_revision: corrected.revision,
            },
            &saved.attachments,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("自动校验未通过"));
    assert!(!error.contains("incorrect"));
    let mut edit = correction(&corrected);
    edit.id_card = saved.id_card.clone();
    let ocr = pm
        .kuaishou_subject_update_ocr(edit, &saved.attachments)
        .unwrap();
    assert_eq!(ocr.source, SubjectSource::Ocr);
    assert_eq!(ocr.review_status, SubjectReviewStatus::PendingReview);
    assert_eq!(ocr.evidence, saved.evidence);
    assert!(ocr.validation.can_confirm());
    assert_eq!(ocr.revision, 4);
    assert_eq!(
        pm.kuaishou_init_steps("12345").unwrap()[0].state,
        KuaishouInitState::Done
    );
}

#[test]
fn invalid_fields_or_images_cannot_set_subject_done_and_failed_write_has_no_reference() {
    let (dir, pm, context) = fixture();
    let lease = pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Subject, false)
        .unwrap()
        .unwrap();
    let mut bad = candidate();
    bad.attachments[0].key = "../../escape.png".into();
    assert!(pm.kuaishou_subject_save_candidate(&lease, 0, bad).is_err());
    assert!(pm.kuaishou_subject_detail("12345").unwrap().is_none());
    let mut bad = candidate();
    bad.id_card = "wrong".into();
    bad.attachments.clear();
    let saved = pm.kuaishou_subject_save_candidate(&lease, 0, bad).unwrap();
    assert!(pm
        .kuaishou_init_complete_subject(&lease, saved.revision, &[])
        .is_err());
    assert_eq!(
        pm.kuaishou_init_steps("12345").unwrap()[0].state,
        KuaishouInitState::Running
    );
    let conn = Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_subject BEFORE UPDATE ON kuaishou_subject_archives BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(pm
        .kuaishou_subject_save_candidate(&lease, saved.revision, candidate())
        .is_err());
    assert_eq!(
        pm.kuaishou_subject_detail("12345")
            .unwrap()
            .unwrap()
            .archive,
        saved
    );
    assert!(!pm
        .kuaishou_subject_attachment_referenced(&image('a').key)
        .unwrap());
}

#[test]
fn same_account_profiles_and_connections_share_lease_different_accounts_do_not() {
    let (dir, pm, context) = fixture();
    let another = account(&pm, "12345");
    let other_account = account(&pm, "23456");
    let second = manager(&dir);
    let lease = pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Subject, false)
        .unwrap()
        .unwrap();
    assert!(second
        .kuaishou_init_claim(&another, KuaishouInitStep::Subject, false)
        .unwrap()
        .is_none());
    assert!(second
        .kuaishou_init_claim(&another, KuaishouInitStep::Slice, false)
        .unwrap()
        .is_none());
    assert!(second
        .kuaishou_init_claim(&other_account, KuaishouInitStep::Slice, false)
        .unwrap()
        .is_some());
    let saved = pm
        .kuaishou_subject_save_candidate(&lease, 0, candidate())
        .unwrap();
    assert_eq!(
        pm.kuaishou_subject_detail("12345")
            .unwrap()
            .unwrap()
            .profiles
            .len(),
        2
    );
    observe(&pm, &another.profile_id, "34567", "test-generation");
    assert_eq!(
        pm.kuaishou_subject_detail("12345")
            .unwrap()
            .unwrap()
            .profiles
            .len(),
        1
    );
    pm.kuaishou_init_complete_subject(&lease, saved.revision, &saved.attachments)
        .unwrap();
    assert!(second
        .kuaishou_init_claim(&context, KuaishouInitStep::Subject, false)
        .unwrap()
        .is_none());
}

#[test]
fn stale_session_binding_or_deleted_profile_rejects_commits_but_owner_can_release() {
    let (_dir, pm, context) = fixture();
    let (lease, saved) = save(&pm, &context);
    observe(&pm, &context.profile_id, "12345", "changed-generation");
    assert!(pm
        .kuaishou_subject_save_candidate(&lease, saved.revision, candidate())
        .is_err());
    assert!(pm
        .kuaishou_init_complete_subject(&lease, saved.revision, &saved.attachments)
        .is_err());
    assert!(pm
        .kuaishou_init_fail(&lease, KuaishouInitErrorCode::ContextChanged)
        .unwrap());
    let new = observe(&pm, &context.profile_id, "12345", "changed-generation");
    let lease = pm
        .kuaishou_init_claim(&new, KuaishouInitStep::Subject, true)
        .unwrap()
        .unwrap();
    let manual = pm
        .business_accounts_save(SaveBusinessAccountInput {
            id: None,
            kind: BusinessAccountKind::KuaishouShop,
            profile_id: context.profile_id.clone(),
            display_name: "手工别名".into(),
            platform_user_id: Some("12345".into()),
        })
        .unwrap();
    assert!(pm
        .kuaishou_subject_save_candidate(&lease, saved.revision, candidate())
        .is_err());
    assert_eq!(
        pm.business_account_get(&manual.id).unwrap().unwrap(),
        manual
    );
    pm.delete(&context.profile_id).unwrap();
    assert!(pm
        .kuaishou_init_complete_subject(&lease, saved.revision, &saved.attachments)
        .is_err());
    assert!(pm
        .kuaishou_init_fail(&lease, KuaishouInitErrorCode::ContextChanged)
        .unwrap());
    assert_eq!(
        pm.kuaishou_subject_detail("12345")
            .unwrap()
            .unwrap()
            .archive,
        saved
    );
}

#[test]
fn failed_steps_need_manual_rearm_and_done_cannot_be_reset_or_faked() {
    let (_dir, pm, context) = fixture();
    let lease = pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Slice, false)
        .unwrap()
        .unwrap();
    let mut proof = SliceVerification {
        platform_user_id: "12345".into(),
        all_four_disabled: [true; 4],
        persisted_readback: false,
    };
    assert!(pm.kuaishou_init_complete_slice(&lease, &proof).is_err());
    proof.persisted_readback = true;
    proof.all_four_disabled[3] = false;
    assert!(pm.kuaishou_init_complete_slice(&lease, &proof).is_err());
    assert!(pm
        .kuaishou_init_fail(&lease, KuaishouInitErrorCode::PersistenceUnverified)
        .unwrap());
    assert!(!pm
        .kuaishou_init_fail(&lease, KuaishouInitErrorCode::TimedOut)
        .unwrap());
    // The monitor never re-claims a failed step; only the manual re-arm may.
    assert!(pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Slice, false)
        .unwrap()
        .is_none());
    let failed = pm
        .kuaishou_init_steps("12345")
        .unwrap()
        .into_iter()
        .find(|s| s.step == KuaishouInitStep::Slice)
        .unwrap();
    assert_eq!(failed.attempts, 1);
    assert_eq!(
        failed.last_error_code,
        Some(KuaishouInitErrorCode::PersistenceUnverified)
    );
    assert!(failed.next_retry_at.is_none());
    assert_eq!(pm.kuaishou_init_retry_failed("12345").unwrap(), 1);
    let next = pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Slice, true)
        .unwrap()
        .unwrap();
    assert_eq!(next.attempts(), 2);
    proof.all_four_disabled = [true; 4];
    assert!(pm.kuaishou_init_complete_slice(&lease, &proof).is_err());
    pm.kuaishou_init_complete_slice(&next, &proof).unwrap();
    assert_eq!(pm.kuaishou_init_retry_failed("12345").unwrap(), 0);
    assert_eq!(pm.kuaishou_init_recover_interrupted().unwrap(), 0);
    assert!(pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Slice, false)
        .unwrap()
        .is_none());
    assert!(!pm
        .kuaishou_init_fail(&next, KuaishouInitErrorCode::TimedOut)
        .unwrap());
    let done = pm
        .kuaishou_init_steps("12345")
        .unwrap()
        .into_iter()
        .find(|s| s.step == KuaishouInitStep::Slice)
        .unwrap();
    assert_eq!(done.state, KuaishouInitState::Done);
    assert_eq!(done.attempts, 2);
    assert!(done.completed_at.is_some());
    assert!(done.last_error_code.is_none());
}

#[test]
fn startup_recovery_preserves_attempts_and_revokes_old_tokens_without_completing() {
    let (dir, pm, context) = fixture();
    let lease = pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Subject, false)
        .unwrap()
        .unwrap();
    drop(pm);
    let pm = manager(&dir);
    assert_eq!(pm.kuaishou_init_recover_interrupted().unwrap(), 1);
    assert_eq!(pm.kuaishou_init_recover_interrupted().unwrap(), 0);
    let state = &pm.kuaishou_init_steps("12345").unwrap()[0];
    assert_eq!(state.state, KuaishouInitState::Failed);
    assert_eq!(state.attempts, 1);
    assert_eq!(
        state.last_error_code,
        Some(KuaishouInitErrorCode::InterruptedNeedsVerification)
    );
    assert!(state.completed_at.is_none());
    assert!(pm
        .kuaishou_subject_save_candidate(&lease, 0, candidate())
        .is_err());
    let next = pm
        .kuaishou_init_claim(&context, KuaishouInitStep::Subject, true)
        .unwrap()
        .unwrap();
    assert_eq!(next.attempts(), 2);
}

#[test]
fn persisted_sensitive_corruption_has_redacted_error_and_no_panic() {
    let (dir, pm, context) = fixture();
    let (_lease, saved) = save(&pm, &context);
    let conn = Connection::open(dir.path().join("test.db")).unwrap();
    conn.execute(
        "UPDATE kuaishou_subject_archives SET document=json_set(document,'$.source',?1)",
        params![saved.id_card],
    )
    .unwrap();
    let error = pm.kuaishou_subject_detail("12345").unwrap_err().to_string();
    assert!(error.contains("资料格式异常"));
    assert!(!error.contains(&saved.id_card));
}
