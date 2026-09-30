//! Fixed process-lifetime workers: WinRT must not repeatedly tear down the last
//! MTA between jobs (the real Chinese OCR smoke regressed with per-call teardown).
use crate::{OcrError, Result, MAX_CONCURRENT_JOBS};
use std::sync::{
    mpsc::{self, SyncSender},
    Arc, Mutex, OnceLock,
};

type Job = Box<dyn FnOnce() + Send + 'static>;

fn sender() -> Result<&'static SyncSender<Job>> {
    static WORKERS: OnceLock<Result<SyncSender<Job>>> = OnceLock::new();
    WORKERS
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<Job>(MAX_CONCURRENT_JOBS);
            let receiver = Arc::new(Mutex::new(receiver));
            for index in 0..MAX_CONCURRENT_JOBS {
                let receiver = receiver.clone();
                std::thread::Builder::new()
                    .name(format!("local-ocr-{index}"))
                    .spawn(move || loop {
                        // Drop the receiver lock BEFORE executing a job, allowing both
                        // workers to run without concurrent access to the queue itself.
                        let next = match receiver.lock() {
                            Ok(queue) => queue.recv(),
                            Err(_) => return,
                        };
                        let Ok(job) = next else {
                            return;
                        };
                        // A panicked job drops its oneshot and permit; report WorkerFailed
                        // to its caller without killing this persistent MTA worker.
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                    })
                    .map_err(|_| OcrError::WorkerFailed)?;
            }
            Ok(sender)
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn submit(job: impl FnOnce() + Send + 'static) -> Result<()> {
    sender()?
        .try_send(Box::new(job))
        .map_err(|error| match error {
            mpsc::TrySendError::Full(_) => OcrError::Busy,
            mpsc::TrySendError::Disconnected(_) => OcrError::WorkerFailed,
        })
}
