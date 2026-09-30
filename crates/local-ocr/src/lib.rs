//! Memory-only image inspection and bounded, local Windows Chinese OCR.
//! No file, network, database, model installation, or logging operations.

mod candidate;
mod picture;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows_ocr;
mod worker;

pub use candidate::{extract_candidates, OcrCandidates};
pub use picture::{
    inspect_image, ImageInfo, MAX_IMAGE_BYTES, MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS,
};

use std::{
    fmt,
    future::Future,
    sync::{Arc, OnceLock},
    time::Instant,
};
use tokio::sync::Semaphore;

pub const OCR_LANGUAGE: &str = "zh-Hans-CN";
/// Process-wide maximum for admitted decoding/OCR jobs, including timed-out workers.
pub const MAX_CONCURRENT_JOBS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OcrError {
    #[error("本平台不支持 Windows 本地 OCR")]
    Unsupported,
    #[error("本机中文 OCR 语言资源不可用；不会自动安装或转云端")]
    LanguageUnavailable,
    #[error("本地 OCR 运行时不可用")]
    RuntimeUnavailable,
    #[error("图片为空或无法完整解码")]
    InvalidImage,
    #[error("仅支持静态 PNG、JPEG、WebP 图片")]
    UnsupportedImage,
    #[error("不接受动画图片")]
    AnimatedImage,
    #[error("图片字节数超限")]
    ImageTooLarge,
    #[error("图片尺寸、像素数或解码内存超限")]
    DimensionsExceeded,
    #[error("本地 OCR 未识别到文字")]
    EmptyRecognition,
    #[error("本地 OCR 调用失败")]
    RecognitionFailed,
    #[error("本地图片/OCR 处理繁忙，请稍后重试")]
    Busy,
    #[error("本地图片/OCR 处理已超时")]
    DeadlineExceeded,
    #[error("本地图片/OCR 工作线程失败")]
    WorkerFailed,
}

pub type Result<T> = std::result::Result<T, OcrError>;

#[derive(Clone)]
pub struct OcrOutput {
    pub image: ImageInfo,
    pub language: &'static str,
    pub lines: Vec<String>,
    pub candidates: OcrCandidates,
}

// OCR text is sensitive. Deliberately redact it from normal error/debug diagnostics.
impl fmt::Debug for OcrOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OcrOutput")
            .field("language", &self.language)
            .field("line_count", &self.lines.len())
            .field("candidates", &self.candidates)
            .finish_non_exhaustive()
    }
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(OcrError::DeadlineExceeded)
    } else {
        Ok(())
    }
}

fn job_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_JOBS)))
}

async fn bounded<T: Send + 'static>(
    deadline: Instant,
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    check_deadline(deadline)?;
    // No unbounded queue of images. Permit moves INSIDE the worker so aborting or
    // timing out the awaiting task cannot release it while work is still running.
    let permit = job_slots()
        .clone()
        .try_acquire_owned()
        .map_err(|_| OcrError::Busy)?;
    let (reply, receiver) = tokio::sync::oneshot::channel();
    worker::submit(move || {
        // Permit stays with the actual work, not the awaiting future.
        // A dropped caller does not need its queued job to start.
        if reply.is_closed() {
            return;
        }
        let result = (|| {
            check_deadline(deadline)?;
            let result = work()?;
            check_deadline(deadline)?;
            Ok(result)
        })();
        drop(permit);
        let _ = reply.send(result);
    })?;
    await_deadline(deadline, async {
        receiver.await.map_err(|_| OcrError::WorkerFailed)?
    })
    .await
}

async fn await_deadline<T>(
    deadline: Instant,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), future)
        .await
        .map_err(|_| OcrError::DeadlineExceeded)?
}

/// Bounded CPU decoder for a Tokio/Tauri async command, NOT the launcher/DB loop.
/// The caller must enforce MAX_IMAGE_BYTES while receiving data, before allocation.
pub async fn inspect_image_bounded(bytes: Vec<u8>, deadline: Instant) -> Result<ImageInfo> {
    picture::check_bytes(&bytes)?;
    bounded(deadline, move || inspect_image(&bytes)).await
}

/// Run actual zh-Hans-CN Windows.Media.Ocr on owned bytes. Never confirms identity.
/// Needs a Tokio runtime with time enabled. Non-Windows always returns Unsupported.
/// The OS OCR request is cancelled on its deadline; CPU decode cannot be preempted.
/// Caller timeout/cancellation never permits unlimited replacement worker threads.
pub async fn recognize(bytes: Vec<u8>, deadline: Instant) -> Result<OcrOutput> {
    #[cfg(windows)]
    {
        picture::check_bytes(&bytes)?;
        bounded(deadline, move || windows_ocr::recognize(&bytes, deadline)).await
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, deadline);
        Err(OcrError::Unsupported)
    }
}

/// Read-only language/engine probe with the same admission bound and deadline.
/// No automatic language installation or alternate recognition engine.
pub async fn check_availability(deadline: Instant) -> Result<()> {
    #[cfg(windows)]
    {
        bounded(deadline, windows_ocr::check_availability).await
    }
    #[cfg(not(windows))]
    {
        let _ = deadline;
        Err(OcrError::Unsupported)
    }
}
