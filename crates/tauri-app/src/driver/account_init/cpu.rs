//! Admission retry for local image/OCR work. Busy guarantees no job was admitted;
//! it is not an invalid image. Never retry a timed-out, failed or cancelled worker.
use local_ocr::{OcrError, Result};
use std::{future::Future, time::{Duration, Instant}};

pub(super) async fn retry_busy<T, F, Fut>(deadline: Instant, mut work: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut delay = Duration::from_millis(10);
    loop {
        if Instant::now() >= deadline { return Err(OcrError::DeadlineExceeded); }
        match work().await {
            Err(OcrError::Busy) => {
                // Sleep owns no decoder permit and queues no CPU job. Dropping this
                // future stops retries; all attempts share the original deadline.
                tokio::time::timeout_at(deadline.into(), tokio::time::sleep(delay))
                    .await.map_err(|_| OcrError::DeadlineExceeded)?;
                delay = (delay * 2).min(Duration::from_millis(100));
            }
            result => return result,
        }
    }
}

pub(super) fn image_error(error: OcrError) -> &'static str {
    match error {
        OcrError::Busy => "证件图片处理繁忙，请稍后重试",
        OcrError::DeadlineExceeded => "证件图片处理超时",
        OcrError::WorkerFailed => "证件图片处理线程不可用",
        _ => "证件图片解码失败",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    #[tokio::test]
    async fn busy_retries_without_changing_deadline_or_retrying_invalid_images() {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut attempts = 0;
        let value = retry_busy(deadline, || {
            attempts += 1;
            std::future::ready(if attempts < 3 { Err(OcrError::Busy) } else { Ok(42) })
        }).await.unwrap();
        assert_eq!(value, 42);
        assert_eq!(attempts, 3);
        for error in [OcrError::InvalidImage, OcrError::DimensionsExceeded, OcrError::DeadlineExceeded, OcrError::WorkerFailed] {
            let mut attempts = 0;
            let result = retry_busy::<(), _, _>(deadline, || {
                attempts += 1;
                std::future::ready(Err(error.clone()))
            }).await;
            assert_eq!(result, Err(error));
            assert_eq!(attempts, 1);
        }
    }

    #[tokio::test]
    async fn busy_is_bounded_and_expired_calls_never_dispatch() {
        let mut attempts = 0;
        let result = retry_busy::<(), _, _>(Instant::now(), || {
            attempts += 1;
            std::future::ready(Err(OcrError::Busy))
        }).await;
        assert_eq!(result, Err(OcrError::DeadlineExceeded));
        assert_eq!(attempts, 0);
        let result = retry_busy::<(), _, _>(Instant::now() + Duration::from_millis(25), || {
            attempts += 1;
            std::future::ready(Err(OcrError::Busy))
        }).await;
        assert_eq!(result, Err(OcrError::DeadlineExceeded));
        assert!(attempts <= 2);
    }

    #[tokio::test]
    async fn abandoned_busy_waiter_stops_dispatching() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(tokio::sync::Notify::new());
        let count = attempts.clone(); let signal = started.clone();
        let task = tokio::spawn(async move {
            retry_busy::<(), _, _>(Instant::now() + Duration::from_secs(2), || {
                count.fetch_add(1, Ordering::SeqCst); signal.notify_one();
                std::future::ready(Err(OcrError::Busy))
            }).await
        });
        started.notified().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let count = attempts.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(attempts.load(Ordering::SeqCst), count);
    }
}
