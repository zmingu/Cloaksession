//! Application-owned immutable subject images. Never accepts a URL or renderer path.
use base64::{engine::general_purpose::STANDARD, Engine};
use multizen_core::{valid_subject_attachment_key, SubjectAttachment};
use std::{path::PathBuf, time::Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) struct AttachmentCache { root: PathBuf }
fn safe(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() { return false; }
    #[cfg(windows)] { use std::os::windows::fs::MetadataExt; if meta.file_attributes() & 0x400 != 0 { return false; } }
    true
}
fn metadata(info: local_ocr::ImageInfo) -> SubjectAttachment {
    SubjectAttachment { key: info.key, sha256: info.sha256, mime_type: info.mime.into(), byte_len: info.byte_len as u64, width: info.width, height: info.height }
}
impl AttachmentCache {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    async fn root(&self) -> Result<PathBuf, &'static str> {
        let m = tokio::fs::symlink_metadata(&self.root).await.map_err(|_| "证件目录不可用")?;
        if !m.is_dir() || !safe(&m) { return Err("证件目录不安全"); }
        tokio::fs::canonicalize(&self.root).await.map_err(|_| "证件目录不可用")
    }
    pub async fn read(&self, key: &str, deadline: Instant) -> Result<(SubjectAttachment, Vec<u8>), &'static str> {
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
        let meta = metadata(local_ocr::inspect_image_bounded(bytes.clone(), deadline).await.map_err(|_| "证件附件解码失败")?);
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
    pub async fn store(&self, bytes: Vec<u8>, deadline: Instant) -> Result<SubjectAttachment, &'static str> {
        let meta = metadata(local_ocr::inspect_image_bounded(bytes.clone(), deadline).await.map_err(|_| "证件图片解码失败")?);
        tokio::fs::create_dir_all(&self.root).await.map_err(|_| "证件目录创建失败")?;
        let root = self.root().await?;
        if let Ok((found, _)) = self.read(&meta.key, deadline).await { return Ok(found); }
        // Never overwrite a preexisting corrupted file or link, even with the same hash name.
        if tokio::fs::symlink_metadata(root.join(&meta.key)).await.is_ok() { return Err("已有证件附件损坏"); }
        let temp = root.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = async {
            let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&temp).await.map_err(|_| "证件临时写入失败")?;
            file.write_all(&bytes).await.map_err(|_| "证件写入失败")?;
            file.sync_all().await.map_err(|_| "证件写入失败")?;
            drop(file);
            // Hard-link publication is atomic and no-clobber on Windows and Unix.
            // The temporary inode is fully written before becoming visible by its content key.
            if tokio::fs::hard_link(&temp, root.join(&meta.key)).await.is_err() {
                let (actual, _) = self.read(&meta.key, deadline).await?;
                if actual != meta { return Err("证件原子保存失败"); }
            }
            let (actual, _) = self.read(&meta.key, deadline).await?;
            if actual != meta { return Err("证件保存后校验失败"); }
            Ok(actual)
        }.await;
        let _ = tokio::fs::remove_file(temp).await;
        result
    }
    pub fn data_url(meta: &SubjectAttachment, bytes: &[u8]) -> String { format!("data:{};base64,{}", meta.mime_type, STANDARD.encode(bytes)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    const IMAGE: &[u8] = include_bytes!("../../../../local-ocr/tests/fixtures/synthetic-zh.png");
    #[tokio::test]
    async fn actual_decode_hash_tamper_delete_and_confinement() {
        let temp = tempfile::tempdir().unwrap();
        let cache = AttachmentCache::new(temp.path().join("attachments"));
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        assert!(cache.store(b"not an image".to_vec(), deadline).await.is_err());
        let a = cache.store(IMAGE.to_vec(), deadline).await.unwrap();
        assert!(a.is_valid());
        assert_eq!(cache.verify(&[a.clone()], deadline).await.unwrap(), vec![a.clone()]);
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
