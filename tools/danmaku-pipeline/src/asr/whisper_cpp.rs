//! 本地 whisper.cpp 绑定（预留，尚未接入）。
//!
//! 计划：通过 `whisper-rs`（whisper.cpp 的 Rust 绑定）加载本地 GGUF/GGML 模型，
//! 对 `extract_audio` 产出的 16k 单声道 wav 做转写，产出词级时间戳。
//! 用 Cargo feature（如 `whisper-cpp`）门控，避免默认引入 C 依赖与模型体积。
//!
//! 当前为占位：transcribe 返回未实现错误，主链路请用 `imported::ImportedAsr`。

use std::path::Path;

use crate::error::PipelineError;
use crate::transcript::Transcript;

use super::Asr;

/// 本地 whisper.cpp 转写器（占位）。
#[derive(Debug, Default)]
pub struct WhisperCppAsr;

impl Asr for WhisperCppAsr {
    fn transcribe(&self, _media_path: &Path) -> Result<Transcript, PipelineError> {
        Err(PipelineError::NotImplemented(
            "本地 whisper.cpp 绑定尚未接入；请用外部 ASR 产物经 ImportedAsr 导入".into(),
        ))
    }
}
