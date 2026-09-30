//! 从外部 ASR 产物导入转写（当前主用路径）。
//!
//! 支持 whisper 系（OpenAI / faster-whisper / whisperX）的 verbose_json，
//! 以及 .srt 字幕。这样无需在本 crate 内跑推理即可打通主链路。

use std::path::Path;

use crate::error::PipelineError;
use crate::transcript::Transcript;

use super::Asr;

/// 导入式转写器：直接读取外部 ASR 产物文件。
#[derive(Debug, Default)]
pub struct ImportedAsr;

impl Asr for ImportedAsr {
    fn transcribe(&self, media_path: &Path) -> Result<Transcript, PipelineError> {
        let path = media_path.to_string_lossy().to_string();
        let raw = std::fs::read_to_string(media_path)?;
        Transcript::from_file_content(&path, &raw)
    }
}
