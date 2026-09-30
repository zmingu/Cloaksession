//! 弹幕识别：从转写中判定主播"在念/回应弹幕"的时刻并还原原文。
//!
//! - `DanmakuDetector`：识别器 trait，可插拔。
//! - `rule`：规则版（离线、可测、零依赖），基于提示语匹配。
//! - `llm`：LLM 版预留（HTTP 调用），当前为占位。

pub mod llm;
pub mod rule;

use crate::model::Persona;
use crate::transcript::Transcript;

/// 一条被识别出的"主播在念/回应的弹幕"。
#[derive(Debug, Clone)]
pub struct DetectedRead {
    /// 主播念出该弹幕的时刻（秒）。
    pub spoken_at: f64,
    /// 还原出的弹幕文本。
    pub text: String,
    /// 该段 ASR 原始转写。
    pub source_text: String,
    /// 可信度 0~1。
    pub confidence: f64,
    /// 人设提示（可选）。
    pub persona: Option<Persona>,
}

/// 弹幕识别器接口。
pub trait DanmakuDetector {
    /// 从转写中识别所有"念/回应弹幕"的时刻。
    fn detect(&self, transcript: &Transcript) -> Vec<DetectedRead>;
}
