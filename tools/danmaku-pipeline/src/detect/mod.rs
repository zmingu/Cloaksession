//! 弹幕识别：从转写中判定主播"在念/回应弹幕"的时刻并还原原文。
//!
//! - `DanmakuDetector`：识别器 trait，可插拔。
//! - `rule`：规则版（离线、零依赖），基于提示语匹配，作兜底。
//! - `llm`：LLM 版，语义识别隐式回应、还原口语弹幕；HTTP 走 `llm-http` feature，
//!   核心解析/映射为纯函数可离线回放验证。
//! - `FallbackDetector`：auto 模式，LLM 为主、规则兜底。

pub mod llm;
pub mod rule;

use crate::model::{CueKind, Persona};
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

/// 一条被识别出的"主播喊跟发刷屏"（如扣1/打1）。
#[derive(Debug, Clone)]
pub struct DetectedCta {
    /// 主播喊出跟发的时刻（秒）。
    pub spoken_at: f64,
    /// 跟进文本（如 "1"）。
    pub text: String,
    /// 该段 ASR 原始转写。
    pub source_text: String,
    /// 可信度 0~1。
    pub confidence: f64,
    /// 本次刷屏条数。
    pub burst: usize,
    /// 喊出后延迟多少秒开始刷屏。
    pub delay_sec: f64,
    /// 人设提示（可选）。
    pub persona: Option<Persona>,
}

/// 一条被识别出的"上车/卖点时间锚点"，下游不发送只消费。
#[derive(Debug, Clone)]
pub struct DetectedCue {
    /// 主播讲到该点的时刻（秒，精确透传）。
    pub spoken_at: f64,
    /// 锚点种类。
    pub cue_kind: CueKind,
    /// 短标签（如【上车】/【卖点】）。
    pub label: String,
    /// 该段 ASR 原始转写。
    pub source_text: String,
    /// 可信度 0~1。
    pub confidence: f64,
    /// AI 分析载荷（如试用/退款/运费险/价格/产品）。
    pub data: Option<serde_json::Value>,
}

/// 识别器产出：AI 只做语义判断回 seg 索引，具体分块与时间锚定由识别器完成。
#[derive(Debug, Clone)]
pub enum DetectedItem {
    Read(DetectedRead),
    Cta(DetectedCta),
    Cue(DetectedCue),
}

impl DetectedItem {
    /// 时间锚点（秒），供组装器排序与避让。
    pub fn spoken_at(&self) -> f64 {
        match self {
            DetectedItem::Read(r) => r.spoken_at,
            DetectedItem::Cta(c) => c.spoken_at,
            DetectedItem::Cue(c) => c.spoken_at,
        }
    }

    /// 可信度，供组装器统一过滤。
    pub fn confidence(&self) -> f64 {
        match self {
            DetectedItem::Read(r) => r.confidence,
            DetectedItem::Cta(c) => c.confidence,
            DetectedItem::Cue(c) => c.confidence,
        }
    }
}

/// 弹幕识别器接口。
pub trait DanmakuDetector {
    /// 从转写中识别所有"念/回应弹幕 + 跟发刷屏 + 上车/卖点锚点"。
    fn detect(&self, transcript: &Transcript) -> Vec<DetectedItem>;
}

/// 降级检测器：先跑 primary，结果为空则回退到 fallback。
/// 用于 auto 模式：LLM 为主、规则兜底。
pub struct FallbackDetector<'a> {
    pub primary: &'a dyn DanmakuDetector,
    pub fallback: &'a dyn DanmakuDetector,
}

impl DanmakuDetector for FallbackDetector<'_> {
    fn detect(&self, transcript: &Transcript) -> Vec<DetectedItem> {
        let primary = self.primary.detect(transcript);
        if primary.is_empty() {
            eprintln!("[info] 主识别器无结果，回退到兜底识别器");
            self.fallback.detect(transcript)
        } else {
            primary
        }
    }
}
