//! New-file ownership spans publication, OCR and the candidate DB commit. The
//! writer gate is retained through cleanup; another batch cannot borrow a key and
//! commit a reference between our negative reference check and removal.
use super::{attachments::AttachmentCache, InitCmd, LauncherCmd};
use multizen_core::SubjectAttachment;
use std::{sync::{Arc, Mutex}, time::{Duration, Instant}};
use tokio::sync::{mpsc, oneshot, OwnedMutexGuard};

pub(super) struct BatchState {
    cache: AttachmentCache,
    launcher: mpsc::Sender<LauncherCmd>,
    gate: Option<OwnedMutexGuard<()>>,
    created: Mutex<Vec<SubjectAttachment>>,
}
impl BatchState {
    pub(super) async fn begin(cache: &AttachmentCache, launcher: mpsc::Sender<LauncherCmd>, deadline: Instant) -> Result<Arc<Self>, &'static str> {
        if Instant::now() >= deadline { return Err("证件附件等待超时"); }
        let gate = tokio::time::timeout_at(deadline.into(), cache.writers.clone().lock_owned()).await.map_err(|_| "证件附件等待超时")?;
        Ok(Arc::new(Self { cache: cache.clone(), launcher, gate: Some(gate), created: Mutex::new(Vec::new()) }))
    }
    pub(super) fn record(&self, attachment: SubjectAttachment) {
        self.created.lock().unwrap().push(attachment);
    }
    pub(super) async fn store(self: &Arc<Self>, bytes: Vec<u8>, deadline: Instant) -> Result<SubjectAttachment, &'static str> {
        self.cache.store_tracked(bytes, deadline, Some(self.clone())).await
    }
}
impl Drop for BatchState {
    fn drop(&mut self) {
        let created = std::mem::take(self.created.get_mut().unwrap());
        let gate = Arc::new(self.gate.take().expect("attachment batch owns its writer gate"));
        if created.is_empty() { return; } // Existing artifacts were borrowed, never owned.
        let cache = self.cache.clone();
        let launcher = self.launcher.clone();
        tauri::async_runtime::spawn(async move {
            let _gate = gate.clone();
            for attachment in created {
                // DB uncertainty always preserves files. No directory scan, age-based
                // sweep, cleanup on Profile deletion, or removal of preexisting keys.
                if referenced(&launcher, attachment.key.clone()).await != Some(false) { continue; }
                if cache.remove_created(&attachment, Instant::now() + Duration::from_secs(10), gate.clone()).await.is_err() {
                    tracing::warn!("account-init new attachment cleanup skipped; file unavailable or changed");
                }
            }
        });
    }
}
async fn referenced(launcher: &mpsc::Sender<LauncherCmd>, key: String) -> Option<bool> {
    for attempt in 0..3 {
        let deadline = Instant::now() + Duration::from_secs(3);
        let (reply, receive) = oneshot::channel();
        let key = key.clone();
        let command = InitCmd { guard: None, deadline, operation: Box::new(move |pm, valid| {
            if !valid || reply.is_closed() { return; }
            let _ = reply.send(pm.kuaishou_subject_attachment_referenced(&key));
        }) };
        let result = tokio::time::timeout_at(deadline.into(), async {
            launcher.send(LauncherCmd::Init(command)).await.map_err(|_| ())?;
            receive.await.map_err(|_| ())?.map_err(|_| ())
        }).await;
        if let Ok(Ok(value)) = result { return Some(value); }
        if launcher.is_closed() { break; }
        if attempt < 2 { tokio::time::sleep(Duration::from_millis(100)).await; }
    }
    None
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
