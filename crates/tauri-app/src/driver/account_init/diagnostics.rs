//! Allowlisted diagnostics only. Never format a driver error, target/profile/account
//! identifier, URL, evaluated expression, page text or document value into a log.
use cdp_driver::TaskError;
use multizen_core::{KuaishouInitErrorCode, KuaishouInitStep, MultizenError};
use std::time::Instant;

pub(super) struct Probe {
    step: &'static str,
    phase: &'static str,
    start: Instant,
    finished: bool,
}
impl Probe {
    pub(super) fn new(step: KuaishouInitStep, phase: &'static str) -> Self {
        let step = match step {
            KuaishouInitStep::Subject => "subject",
            KuaishouInitStep::Slice => "slice",
        };
        tracing::info!(
            step,
            phase,
            error = "pending",
            elapsed_ms = 0u64,
            "account-init phase"
        );
        Self {
            step,
            phase,
            start: Instant::now(),
            finished: false,
        }
    }
    pub(super) fn slice_action(action: &str) -> Option<Self> {
        let phase = match action {
            "open-ready" => "slice-entry-probe",
            "open" => "slice-open",
            "slice" => "slice-read",
            "off" => "slice-off",
            "close" => "slice-close",
            _ => return None,
        };
        Some(Self::new(KuaishouInitStep::Slice, phase))
    }
    pub(super) fn finish(&mut self, error: &'static str) {
        if self.finished {
            return;
        }
        self.finished = true;
        tracing::info!(
            step = self.step,
            phase = self.phase,
            error,
            elapsed_ms = self.start.elapsed().as_millis() as u64,
            "account-init phase"
        );
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.finish("waiter-dropped");
    }
}
pub(super) fn finish(probe: &mut Option<Probe>, error: &'static str) {
    if let Some(probe) = probe {
        probe.finish(error);
    }
}
pub(super) fn task_error(error: &TaskError) -> &'static str {
    match error {
        TaskError::Cancelled => "task-cancelled",
        TaskError::TimedOut => "task-timed-out",
        TaskError::Interrupted => "task-interrupted",
        TaskError::InvalidOptions(_) => "invalid-options",
        TaskError::Driver(MultizenError::Cdp(message))
            if message.contains("account-init-page-unsupported") =>
        {
            "adapter-rejected"
        }
        TaskError::Driver(_) => "driver-error",
    }
}
pub(super) fn step_error(code: KuaishouInitErrorCode) -> &'static str {
    match code {
        KuaishouInitErrorCode::InterruptedNeedsVerification => "interrupted-needs-verification",
        KuaishouInitErrorCode::ContextChanged => "context-rejected",
        KuaishouInitErrorCode::TimedOut => "step-timed-out",
        KuaishouInitErrorCode::PageUnsupported => "page-unsupported",
        KuaishouInitErrorCode::PageCrashed => "page-crashed",
        KuaishouInitErrorCode::AttachmentUnavailable => "attachment-unavailable",
        KuaishouInitErrorCode::OcrUnavailable => "ocr-unavailable",
        KuaishouInitErrorCode::OcrFailed => "ocr-failed",
        KuaishouInitErrorCode::ValidationFailed => "validation-failed",
        KuaishouInitErrorCode::PersistenceUnverified => "persistence-unverified",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_categories_never_return_raw_driver_messages() {
        let raw = "https://s.kwaixiaodian.com/private?secret=fixture account-init-page-unsupported";
        assert_eq!(
            task_error(&TaskError::Driver(MultizenError::Cdp(raw.into()))),
            "adapter-rejected"
        );
        assert_eq!(
            task_error(&TaskError::Driver(MultizenError::Cdp(
                "sensitive fixture".into()
            ))),
            "driver-error"
        );
        assert_eq!(
            task_error(&TaskError::InvalidOptions("sensitive fixture")),
            "invalid-options"
        );
        assert_eq!(task_error(&TaskError::TimedOut), "task-timed-out");
    }
    #[test]
    fn crashed_renderer_code_is_distinct_from_structure_and_context() {
        assert_eq!(
            step_error(KuaishouInitErrorCode::PageCrashed),
            "page-crashed"
        );
        assert_ne!(
            step_error(KuaishouInitErrorCode::PageCrashed),
            step_error(KuaishouInitErrorCode::PageUnsupported)
        );
        assert_ne!(
            step_error(KuaishouInitErrorCode::PageCrashed),
            step_error(KuaishouInitErrorCode::ContextChanged)
        );
    }
}
