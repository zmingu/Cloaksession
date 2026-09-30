//! LLM 版弹幕识别器（预留）。
//!
//! 目标：把整段带时间戳的转写喂给 LLM，让它判定"哪些时刻主播在念/回应弹幕"，
//! 并还原（含隐式回应反推）弹幕原文、给出置信度和人设。相比规则版，语义理解更强，
//! 能处理"对，这个是纯棉的"这类没有显式提示语的隐式回应。
//!
//! 实现方式（待接入）：
//! - 通过 HTTP 调用兼容 OpenAI Chat Completions 的接口；
//! - system 提示约束输出为结构化 JSON（时刻/文本/置信度/persona）；
//! - 解析后映射为 `Vec<DetectedRead>`。
//!
//! 当前为占位：返回空结果，避免在未配置模型时阻塞主链路。

use super::{DanmakuDetector, DetectedRead};
use crate::transcript::Transcript;

/// LLM 识别器配置。
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// 兼容 OpenAI 的 base_url，如 https://api.openai.com/v1。
    pub base_url: String,
    /// API key。
    pub api_key: String,
    /// 模型名。
    pub model: String,
}

/// LLM 识别器（占位实现）。
#[derive(Debug)]
pub struct LlmDetector {
    #[allow(dead_code)]
    config: LlmConfig,
}

impl LlmDetector {
    pub fn new(config: LlmConfig) -> Self {
        Self { config }
    }
}

impl DanmakuDetector for LlmDetector {
    fn detect(&self, _transcript: &Transcript) -> Vec<DetectedRead> {
        // TODO: 接入 HTTP 调用与结构化解析。当前返回空，主链路应回退到规则版。
        eprintln!("[warn] LLM 识别器尚未接入，返回空结果；请使用规则版或稍后接入 LLM。");
        Vec::new()
    }
}
