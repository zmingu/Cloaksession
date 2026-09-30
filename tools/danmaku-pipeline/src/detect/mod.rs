//! 弹幕识别：从转写中判定主播"在念/回应弹幕"的时刻并还原原文。
//!
//! - `DanmakuDetector`：识别器 trait，可插拔。
//! - `rule`：规则版（离线、零依赖），基于提示语匹配，作兜底。
//! - `llm`：LLM 版，语义识别隐式回应、还原口语弹幕；HTTP 走 `llm-http` feature，
//!   核心解析/映射为纯函数可离线回放验证。
//! - `FallbackDetector`：auto 模式，LLM 为主、规则兜底。

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

/// 降级检测器：先跑 primary，结果为空则回退到 fallback。
/// 用于 auto 模式：LLM 为主、规则兜底。
pub struct FallbackDetector<'a> {
    pub primary: &'a dyn DanmakuDetector,
    pub fallback: &'a dyn DanmakuDetector,
}

impl DanmakuDetector for FallbackDetector<'_> {
    fn detect(&self, transcript: &Transcript) -> Vec<DetectedRead> {
        let primary = self.primary.detect(transcript);
        if primary.is_empty() {
            eprintln!("[info] 主识别器无结果，回退到兜底识别器");
            self.fallback.detect(transcript)
        } else {
            primary
        }
    }
}
