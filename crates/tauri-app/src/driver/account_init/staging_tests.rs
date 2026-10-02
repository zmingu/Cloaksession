use super::*;
use super::super::lease::tests::fixture;
use multizen_core::*;

const IMAGE: &[u8] = include_bytes!("../../../../local-ocr/tests/fixtures/synthetic-zh.png");
fn deadline() -> Instant { Instant::now() + Duration::from_secs(20) }
async fn next(receiver: &mut mpsc::Receiver<LauncherCmd>) -> InitCmd {
    match tokio::time::timeout(Duration::from_secs(2), receiver.recv()).await.unwrap().unwrap() {
        LauncherCmd::Init(command) => command,
        _ => panic!("unexpected launcher command"),
    }
}
async fn cleaned(cache: &AttachmentCache) {
    let _gate = tokio::time::timeout(Duration::from_secs(15), cache.writers.clone().lock_owned()).await.unwrap();
}

#[tokio::test]
async fn only_new_unreferenced_files_are_removed_and_successor_waits_for_cleanup() {
    let (dir, pm, _) = fixture();
    let cache = AttachmentCache::new(dir.path().join("attachments"));
    let (launcher, mut receiver) = mpsc::channel(4);
    let batch = BatchState::begin(&cache, launcher, deadline()).await.unwrap();
    let meta = batch.store(IMAGE.to_vec(), deadline()).await.unwrap();
    assert!(cache.read(&meta.key, deadline()).await.is_ok());
    assert_eq!(std::fs::read_dir(dir.path().join("attachments")).unwrap().count(), 1); // no temporary residue
    drop(batch);
    let command = next(&mut receiver).await;
    assert!(cache.writers.try_lock().is_err()); // No successor may borrow the key before deletion.
    (command.operation)(&pm, true);
    cleaned(&cache).await;
    assert!(cache.read(&meta.key, deadline()).await.is_err());
}

#[tokio::test]
async fn existing_unreferenced_artifacts_are_never_adopted_or_deleted() {
    let (dir, _pm, _) = fixture();
    let cache = AttachmentCache::new(dir.path().join("attachments"));
    let meta = cache.store(IMAGE.to_vec(), deadline()).await.unwrap();
    let (launcher, mut receiver) = mpsc::channel(4);
    let batch = BatchState::begin(&cache, launcher, deadline()).await.unwrap();
    assert_eq!(batch.store(IMAGE.to_vec(), deadline()).await.unwrap(), meta);
    drop(batch);
    cleaned(&cache).await;
    assert!(receiver.try_recv().is_err());
    assert!(cache.read(&meta.key, deadline()).await.is_ok());
}

#[tokio::test]
async fn committed_reference_survives_lost_reply_and_profile_deletion() {
    let (dir, pm, context) = fixture();
    let cache = AttachmentCache::new(dir.path().join("attachments"));
    let (launcher, mut receiver) = mpsc::channel(4);
    let batch = BatchState::begin(&cache, launcher, deadline()).await.unwrap();
    let meta = batch.store(IMAGE.to_vec(), deadline()).await.unwrap();
    let lease = pm.kuaishou_init_claim(&context, KuaishouInitStep::Subject).unwrap().unwrap();
    // Commit without delivering a success response to the attachment owner.
    pm.kuaishou_subject_save_candidate(&lease, 0, SubjectCandidate {
        real_name: "合成样本".into(), id_card: "incomplete".into(), source: SubjectSource::Ocr,
        evidence: SubjectVisibleEvidence::default(), attachments: vec![meta.clone()],
    }).unwrap();
    pm.delete(&context.profile_id).unwrap();
    drop(batch);
    let command = next(&mut receiver).await;
    (command.operation)(&pm, true);
    cleaned(&cache).await;
    assert!(cache.read(&meta.key, deadline()).await.is_ok());
    assert!(pm.kuaishou_subject_attachment_referenced(&meta.key).unwrap());
}

#[tokio::test]
async fn changed_files_and_unknown_database_state_are_preserved() {
    for changed in [false, true] {
        let (dir, pm, _) = fixture();
        let root = dir.path().join("attachments");
        let cache = AttachmentCache::new(root.clone());
        let (launcher, mut receiver) = mpsc::channel(4);
        let batch = BatchState::begin(&cache, launcher, deadline()).await.unwrap();
        let meta = batch.store(IMAGE.to_vec(), deadline()).await.unwrap();
        if changed { std::fs::write(root.join(&meta.key), b"replacement must be retained").unwrap(); }
        if !changed { receiver.close(); }
        drop(batch);
        if changed {
            let command = next(&mut receiver).await;
            (command.operation)(&pm, true);
        }
        cleaned(&cache).await;
        assert!(root.join(&meta.key).is_file());
    }
}

#[tokio::test]
async fn abandoned_caller_cannot_cleanup_before_publication_owner_finishes() {
    let (dir, pm, _) = fixture();
    let cache = AttachmentCache::new(dir.path().join("attachments"));
    let (launcher, mut receiver) = mpsc::channel(4);
    let batch = BatchState::begin(&cache, launcher, deadline()).await.unwrap();
    let meta = batch.store(IMAGE.to_vec(), deadline()).await.unwrap();
    // Blocking publication retains this same Arc even if its async waiter is cancelled.
    let publication_owner = batch.clone();
    drop(batch);
    assert!(cache.writers.try_lock().is_err());
    assert!(receiver.try_recv().is_err());
    drop(publication_owner);
    let command = next(&mut receiver).await;
    (command.operation)(&pm, true);
    cleaned(&cache).await;
    assert!(cache.read(&meta.key, deadline()).await.is_err());
}
