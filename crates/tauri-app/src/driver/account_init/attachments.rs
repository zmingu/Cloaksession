//! Application-owned immutable subject images. Never accepts a URL or renderer path.
use base64::{engine::general_purpose::STANDARD, Engine};
use multizen_core::{valid_subject_attachment_key, SubjectAttachment};
use std::{path::PathBuf, sync::Arc, time::Instant};
use tokio::io::AsyncReadExt;

#[derive(Clone)]
pub(super) struct AttachmentCache {
    root: PathBuf,
    pub(super) writers: Arc<tokio::sync::Mutex<()>>,
}
struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); }
}
fn safe(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() { return false; }
    #[cfg(windows)] { use std::os::windows::fs::MetadataExt; if meta.file_attributes() & 0x400 != 0 { return false; } }
    true
}
fn metadata(info: local_ocr::ImageInfo) -> SubjectAttachment {
    SubjectAttachment { key: info.key, sha256: info.sha256, mime_type: info.mime.into(), byte_len: info.byte_len as u64, width: info.width, height: info.height }
}
impl AttachmentCache {
    pub fn new(root: PathBuf) -> Self { Self { root, writers: Arc::new(tokio::sync::Mutex::new(())) } }
    async fn root(&self) -> Result<PathBuf, &'static str> {
        let m = tokio::fs::symlink_metadata(&self.root).await.map_err(|_| "证件目录不可用")?;
        if !m.is_dir() || !safe(&m) { return Err("证件目录不安全"); }
        tokio::fs::canonicalize(&self.root).await.map_err(|_| "证件目录不可用")
    }
    pub async fn read(&self, key: &str, deadline: Instant) -> Result<(SubjectAttachment, Vec<u8>), &'static str> {
        if Instant::now() >= deadline { return Err("证件附件读取超时"); }
        tokio::time::timeout_at(deadline.into(), self.read_file(key, deadline))
            .await.map_err(|_| "证件附件读取超时")?
    }
    async fn read_file(&self, key: &str, deadline: Instant) -> Result<(SubjectAttachment, Vec<u8>), &'static str> {
        if !valid_subject_attachment_key(key) { return Err("证件附件key无效"); }
        let root = self.root().await?;
        let path = root.join(key);
        let m = tokio::fs::symlink_metadata(&path).await.map_err(|_| "证件附件缺失")?;
        if !m.is_file() || !safe(&m) || m.len() > local_ocr::MAX_IMAGE_BYTES as u64 { return Err("证件附件不安全"); }
        if tokio::fs::canonicalize(&path).await.map_err(|_| "证件附件不可用")?.parent() != Some(root.as_path()) { return Err("证件附件越界"); }
        let mut options = tokio::fs::OpenOptions::new(); options.read(true);
        #[cfg(windows)] { options.custom_flags(0x00200000); }
        #[cfg(target_os="linux")] { options.custom_flags(0x20000); }
        #[cfg(target_os="macos")] { options.custom_flags(0x100); }
        let file = options.open(&path).await.map_err(|_| "证件附件不可用")?;
        let m = file.metadata().await.map_err(|_| "证件附件不可用")?;
        if !m.is_file() || !safe(&m) || m.len() > local_ocr::MAX_IMAGE_BYTES as u64 { return Err("证件附件不安全"); }
        let mut bytes = Vec::new();
        file.take(local_ocr::MAX_IMAGE_BYTES as u64 + 1).read_to_end(&mut bytes).await.map_err(|_| "证件附件读取失败")?;
        let meta = metadata(super::cpu::retry_busy(deadline, || local_ocr::inspect_image_bounded(bytes.clone(), deadline))
            .await.map_err(super::cpu::image_error)?);
        if meta.key != key { return Err("证件附件哈希不符"); }
        Ok((meta, bytes))
    }
    pub async fn verify(&self, expected: &[SubjectAttachment], deadline: Instant) -> Result<Vec<SubjectAttachment>, &'static str> {
        if !multizen_core::valid_subject_attachments(expected) { return Err("证件附件不完整"); }
        let mut verified = Vec::new();
        for item in expected {
            let (actual, _) = self.read(&item.key, deadline).await?;
            if actual != *item { return Err("证件附件版本变化"); }
            verified.push(actual);
        }
        Ok(verified)
    }
    #[cfg(test)]
    pub async fn store(&self, bytes: Vec<u8>, deadline: Instant) -> Result<SubjectAttachment, &'static str> {
        self.store_tracked(bytes, deadline, None).await
    }
    pub(super) async fn store_tracked(&self, bytes: Vec<u8>, deadline: Instant, owner: Option<Arc<super::staging::BatchState>>) -> Result<SubjectAttachment, &'static str> {
        let meta = metadata(super::cpu::retry_busy(deadline, || local_ocr::inspect_image_bounded(bytes.clone(), deadline))
            .await.map_err(super::cpu::image_error)?);
        tokio::fs::create_dir_all(&self.root).await.map_err(|_| "证件目录创建失败")?;
        let root = self.root().await?;
        match tokio::fs::symlink_metadata(root.join(&meta.key)).await {
            Ok(_) => {
                // An existing file is borrowed, not a publication candidate. Propagate
                // its actual read/admission error instead of labelling any failure corrupt.
                let (found, _) = self.read(&meta.key, deadline).await?;
                return Ok(found);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => return Err("证件附件不可用"),
        }
        let published = meta.clone();
        // Publication and ownership registration cannot be interrupted between hard-link
        // creation and recording it. A dropped waiter leaves the worker holding the batch
        // (and its writer gate), so cleanup cannot race an in-flight filesystem operation.
        let worker = tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let temp = root.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|_| "证件临时写入失败")?;
            let temporary = TemporaryFile(temp.clone());
            let result = (|| {
                file.write_all(&bytes).map_err(|_| "证件写入失败")?;
                file.sync_all().map_err(|_| "证件写入失败")?;
                if std::fs::hard_link(&temp, root.join(&published.key)).is_ok() {
                    if let Some(owner) = &owner { owner.record(published); }
                }
                Ok::<_, &'static str>(())
            })();
            drop(file);
            drop(temporary);
            result
        });
        tokio::time::timeout_at(deadline.into(), worker).await.map_err(|_| "证件保存超时")?
            .map_err(|_| "证件写入失败")??;
        let (actual, _) = self.read(&meta.key, deadline).await?;
        if actual != meta { return Err("证件保存后校验失败"); }
        Ok(actual)
    }
    /// Only used for keys recorded by this batch's successful no-clobber publication.
    /// The caller holds the writer gate and has obtained a fresh negative DB reference check.
    pub(super) async fn remove_created(&self, expected: &SubjectAttachment, deadline: Instant, gate: Arc<tokio::sync::OwnedMutexGuard<()>>) -> Result<(), &'static str> {
        let (actual, _) = self.read(&expected.key, deadline).await?;
        if actual != *expected { return Err("证件附件已变化，保留文件"); }
        let root = self.root().await?;
        let path = root.join(&expected.key);
        owned_deletion(deadline, gate, move || std::fs::remove_file(path)).await
    }
    pub fn data_url(meta: &SubjectAttachment, bytes: &[u8]) -> String { format!("data:{};base64,{}", meta.mime_type, STANDARD.encode(bytes)) }
}

/// Deadline bounds admission, not the lifetime of an already dispatched OS unlink.
/// The actual worker owns a gate reference even if its async waiter is dropped.
async fn owned_deletion(
    deadline: Instant,
    gate: Arc<tokio::sync::OwnedMutexGuard<()>>,
    remove: impl FnOnce() -> std::io::Result<()> + Send + 'static,
) -> Result<(), &'static str> {
    if Instant::now() >= deadline { return Err("证件清理超时"); }
    tokio::task::spawn_blocking(move || {
        let _gate = gate;
        if Instant::now() >= deadline { return Err("证件清理超时"); }
        remove().map_err(|_| "证件清理失败")
    }).await.map_err(|_| "证件清理线程不可用")?
}

#[cfg(test)]
mod tests {
    use super::*;
    const IMAGE: &[u8] = include_bytes!("../../../../local-ocr/tests/fixtures/synthetic-zh.png");
    #[tokio::test]
    async fn expired_or_dropped_delete_waiter_keeps_successor_blocked_until_os_work_finishes() {
        use std::time::Duration;
        for abort in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("new-owned-file");
            std::fs::write(&path, b"owned fixture").unwrap();
            let writers = Arc::new(tokio::sync::Mutex::new(()));
            let gate = Arc::new(writers.clone().lock_owned().await);
            let started = Arc::new(tokio::sync::Notify::new());
            let signal = started.clone();
            let (finish, barrier) = std::sync::mpsc::channel();
            let target = path.clone();
            let task = tokio::spawn(async move {
                tokio::time::timeout(if abort { Duration::from_secs(10) } else { Duration::from_millis(20) }, owned_deletion(
                    Instant::now() + Duration::from_secs(10), gate,
                    move || {
                        signal.notify_one();
                        barrier.recv().unwrap(); // OS operation admitted, but not completed.
                        std::fs::remove_file(target)
                    },
                )).await
            });
            tokio::time::timeout(Duration::from_secs(2), started.notified()).await.unwrap();
            if abort { task.abort(); assert!(task.await.unwrap_err().is_cancelled()); }
            else { assert!(task.await.unwrap().is_err()); }
            assert!(writers.try_lock().is_err());
            assert!(path.exists());
            finish.send(()).unwrap();
            let _successor = tokio::time::timeout(Duration::from_secs(2), writers.clone().lock_owned()).await.unwrap();
            assert!(!path.exists()); // Successor can only proceed after the real unlink.
        }
    }
    #[tokio::test]
    async fn expired_deletion_is_not_admitted() {
        let writers = Arc::new(tokio::sync::Mutex::new(()));
        let gate = Arc::new(writers.clone().lock_owned().await);
        assert!(owned_deletion(Instant::now(), gate, || panic!("expired deletion dispatched")).await.is_err());
        assert!(writers.try_lock().is_ok());
    }
    #[tokio::test]
    async fn parallel_valid_image_reads_retry_busy_admission_without_bypassing_validation() {
        let temp = tempfile::tempdir().unwrap();
        let cache = AttachmentCache::new(temp.path().join("attachments"));
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        let image = cache.store(IMAGE.to_vec(), deadline).await.unwrap();
        let mut jobs = tokio::task::JoinSet::new();
        for _ in 0..local_ocr::MAX_CONCURRENT_JOBS * 4 {
            let cache = cache.clone(); let expected = image.clone();
            jobs.spawn(async move {
                assert_eq!(cache.verify(&[expected.clone()], deadline).await.unwrap(), vec![expected]);
            });
        }
        while let Some(result) = jobs.join_next().await { result.unwrap(); }
        let mut mismatched = image; mismatched.width += 1;
        assert!(cache.verify(&[mismatched], deadline).await.is_err());
    }
    #[tokio::test]
    async fn actual_decode_hash_tamper_delete_and_confinement() {
        let temp = tempfile::tempdir().unwrap();
        let cache = AttachmentCache::new(temp.path().join("attachments"));
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        assert!(cache.store(b"not an image".to_vec(), deadline).await.is_err());
        let a = cache.store(IMAGE.to_vec(), deadline).await.unwrap();
        assert!(a.is_valid());
        assert_eq!(cache.verify(&[a.clone()], deadline).await.unwrap(), vec![a.clone()]);
        assert!(cache.read(&a.key, Instant::now()).await.is_err());
        let mut stale = a.clone(); stale.width += 1;
        assert!(cache.verify(&[stale], deadline).await.is_err());
        assert!(cache.verify(&[], deadline).await.is_err());
        assert!(cache.read("../secret.png", deadline).await.is_err());
        let profiles = temp.path().join("profiles"); tokio::fs::create_dir(&profiles).await.unwrap();
        tokio::fs::remove_dir_all(profiles).await.unwrap();
        assert!(cache.read(&a.key, deadline).await.is_ok());
        tokio::fs::write(cache.root.join(&a.key), b"corrupt").await.unwrap();
        assert!(cache.verify(&[a.clone()], deadline).await.is_err());
        assert!(cache.store(IMAGE.to_vec(), deadline).await.is_err());
        tokio::fs::remove_file(cache.root.join(&a.key)).await.unwrap();
        assert!(cache.read(&a.key, deadline).await.is_err());
    }
}
