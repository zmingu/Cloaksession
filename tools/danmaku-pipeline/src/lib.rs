//! danmaku-pipeline：直播录像 → 分析主播念/回应弹幕 → 生成符合契约的定时弹幕脚本。
//!
//! 主链路（离线可跑）：
//!   转写(whisper.json/SRT) → 规则识别 read → 组装(算 send_at + 补 filler) → DanmakuScript
//!
//! 契约见 `tools/danmaku-pipeline/schema/danmaku-script.schema.json`。

pub mod asr;
pub mod builder;
pub mod config;
pub mod detect;
pub mod error;
pub mod filler;
pub mod model;
pub mod transcript;

use crate::builder::ScriptBuilder;
use crate::config::PipelineConfig;
use crate::detect::rule::RuleDetector;
use crate::detect::DanmakuDetector;
use crate::error::PipelineError;
use crate::model::DanmakuScript;
use crate::transcript::Transcript;

/// 从转写内容一步生成弹幕脚本（默认用规则版识别器）。
///
/// - `transcript_path`：仅用于按扩展名判断 json/srt。
/// - `raw`：转写文件内容。
/// - `config`：管线配置。
pub fn build_from_transcript(
    transcript_path: &str,
    raw: &str,
    config: PipelineConfig,
) -> Result<DanmakuScript, PipelineError> {
    let transcript = Transcript::from_file_content(transcript_path, raw)?;
    Ok(build_with_detector(&transcript, &RuleDetector, config))
}

/// 用指定识别器从已解析的转写生成脚本。
pub fn build_with_detector<D: DanmakuDetector>(
    transcript: &Transcript,
    detector: &D,
    config: PipelineConfig,
) -> DanmakuScript {
    let duration = transcript.total_duration();
    let reads = detector.detect(transcript);
    ScriptBuilder::new(config).build(&reads, duration)
}
