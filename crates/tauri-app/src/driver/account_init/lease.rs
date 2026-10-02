//! A claim is owned even while its oneshot reply is queued. Dropping the reply or
//! worker releases only this token, never a newer attempt or a completed step.
use super::{InitCmd, LauncherCmd};
use multizen_core::KuaishouInitErrorCode;
use profile_manager::KuaishouInitLease;
use std::{ops::Deref, time::{Duration, Instant}};
use tokio::sync::{mpsc, oneshot};
#[cfg(test)]
#[path = "lease_tests.rs"]
pub(super) mod tests;

pub(super) struct RunningLease {
    lease: KuaishouInitLease,
    launcher: mpsc::Sender<LauncherCmd>,
    failure: KuaishouInitErrorCode,
    retry_seconds: u32,
    released: bool,
}
impl RunningLease {
    pub(super) fn new(lease: KuaishouInitLease, launcher: mpsc::Sender<LauncherCmd>) -> Self {
        Self { lease, launcher, failure: KuaishouInitErrorCode::InterruptedNeedsVerification, retry_seconds: 40, released: false }
    }
    pub(super) async fn fail(&mut self, code: KuaishouInitErrorCode, retry_seconds: u32) {
        self.failure = code;
        self.retry_seconds = retry_seconds;
        self.released = release(&self.launcher, &self.lease, code, retry_seconds).await;
    }
}
impl Deref for RunningLease {
    type Target = KuaishouInitLease;
    fn deref(&self) -> &Self::Target { &self.lease }
}
impl Drop for RunningLease {
    fn drop(&mut self) {
        if self.released { return; }
        let launcher = self.launcher.clone();
        let lease = self.lease.clone();
        let code = self.failure;
        let retry_seconds = self.retry_seconds;
        tauri::async_runtime::spawn(async move {
            if !release(&launcher, &lease, code, retry_seconds).await {
                // Closed/unavailable storage is handled by startup recovery, never takeover.
                tracing::warn!("account-init lease cleanup unavailable; startup recovery required");
            }
        });
    }
}

async fn release(launcher: &mpsc::Sender<LauncherCmd>, lease: &KuaishouInitLease, code: KuaishouInitErrorCode, retry_seconds: u32) -> bool {
    for attempt in 0..3 {
        let deadline = Instant::now() + Duration::from_secs(3);
        let (reply, receive) = oneshot::channel();
        let lease = lease.clone();
        let command = InitCmd { guard: None, deadline, operation: Box::new(move |pm, _within_deadline| {
            // Unlike a new claim/write, token-only release must still run if its caller's
            // deadline/reply was lost. CAS makes a repeated or late release harmless.
            let result = pm.kuaishou_init_fail(&lease, code, retry_seconds);
            let _ = reply.send(result);
        }) };
        let result = tokio::time::timeout_at(deadline.into(), async {
            launcher.send(LauncherCmd::Init(command)).await.map_err(|_| ())?;
            receive.await.map_err(|_| ())?.map_err(|_| ())
        }).await;
        if matches!(result, Ok(Ok(_))) { return true; }
        if launcher.is_closed() { return false; }
        if attempt < 2 { tokio::time::sleep(Duration::from_millis(100)).await; }
    }
    false
}
