//! ASR 抽象：音频/视频 → 带时间戳转写。
//!
//! - `Asr`：转写器 trait。
//! - `imported`：从外部 ASR 产物（whisper.json/SRT）导入，当前主用路径。
//! - `whisper_cpp`：本地 whisper.cpp 绑定预留（feature 门控，尚未接入）。
//! - `extract_audio`：ffmpeg 抽音频辅助（供本地转写用）。

pub mod imported;
pub mod whisper_cpp;

use std::path::Path;
use std::process::Command;

use crate::error::PipelineError;
use crate::transcript::Transcript;

/// 转写器接口。
pub trait Asr {
    /// 将音频/视频转写为带时间戳的 Transcript。
    fn transcribe(&self, media_path: &Path) -> Result<Transcript, PipelineError>;
}

/// 用 ffmpeg 从视频抽取 16k 单声道 wav（本地转写前置步骤）。
///
/// 需要系统已安装 ffmpeg。返回输出路径。
pub fn extract_audio(input: &Path, out_wav: &Path) -> Result<(), PipelineError> {
    let status = Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(input)
        .args(["-ac", "1", "-ar", "16000", "-vn"])
        .arg(out_wav)
        .status()
        .map_err(|e| PipelineError::Config(format!("调用 ffmpeg 失败（是否已安装？）: {e}")))?;
    if !status.success() {
        return Err(PipelineError::Parse(format!(
            "ffmpeg 退出码非零: {status}"
        )));
    }
    Ok(())
}
