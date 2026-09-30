//! 弹幕脚本契约的数据模型（输出侧）。
//!
//! 严格对齐 `tools/danmaku-pipeline/schema/danmaku-script.schema.json` v1.0。
//! 序列化产物需能通过该 JSON Schema 校验。

use serde::{Deserialize, Serialize};

/// 契约当前版本。
pub const CONTRACT_VERSION: &str = "1.0";

/// 顶层弹幕脚本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanmakuScript {
    /// 契约版本，固定 "1.0"。
    pub version: String,
    /// 生成参数与溯源信息。
    pub meta: Meta,
    /// 弹幕事件列表，按 `send_at` 升序。
    pub events: Vec<Event>,
}

/// 生成参数与溯源信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    /// 目标平台，当前落地 kuaishou。
    pub platform: String,
    /// 源录像文件名或路径，仅溯源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_video: Option<String>,
    /// 录像总时长（秒）。
    pub video_duration_sec: f64,
    /// generated_at 的时区。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// 脚本生成时间（ISO 8601）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// 生成工具及版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    /// 转写所用 ASR 模型标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asr_model: Option<String>,
    /// 识别/还原弹幕所用 LLM 标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    /// 默认提前量（秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_lead_time_sec: Option<f64>,
    /// 氛围弹幕相对保真弹幕的数量比例。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filler_ratio: Option<f64>,
}

/// 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    /// 主播真念过/回应过的弹幕，已还原原文。
    Read,
    /// 管线补充的氛围弹幕。
    Filler,
}

/// 人设提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Persona {
    Buyer,
    Fan,
    Newcomer,
    Neutral,
    Other,
}

/// 单条弹幕事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// 稳定唯一 ID，如 evt-0001。
    pub id: String,
    /// read / filler。
    #[serde(rename = "type")]
    pub event_type: EventType,
    /// 相对视频起点秒数，发送端权威触发时间。
    pub send_at: f64,
    /// 最终弹幕文本（1~100 字）。
    pub text: String,
    /// read 专用：主播念出该弹幕的时刻（秒）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoken_at: Option<f64>,
    /// read 专用：本条实际提前量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_time_sec: Option<f64>,
    /// read 专用：可信度 0~1。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// read 专用：该段 ASR 原始转写。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
    /// 账号分配提示：auto 或稳定虚拟观众 ID。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_hint: Option<String>,
    /// 人设提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<Persona>,
    /// 人工校对备注。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
