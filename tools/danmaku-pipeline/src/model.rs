//! 弹幕脚本契约的数据模型（输出侧）。
//!
//! 严格对齐 `tools/danmaku-pipeline/schema/danmaku-script.schema.json` v1.1。
//! 序列化产物需能通过该 JSON Schema 校验。

use serde::{Deserialize, Serialize};

/// 契约当前版本。
pub const CONTRACT_VERSION: &str = "1.1";

/// 顶层弹幕脚本。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanmakuScript {
    /// 契约版本，固定 "1.1"。
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
    /// 主播喊跟发刷屏（如扣1），需多号同发。
    Cta,
    /// 上车/卖点时间锚点，下游不发送只消费 cue_kind + data。
    Cue,
}

/// 上车/卖点锚点种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    /// 上车点（可挂链上车）。
    OnboardCall,
    /// 卖点/保障话术锚点。
    Pitch,
    /// 其他 AI 分析锚点。
    Other,
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
    /// read / filler / cta / cue。
    #[serde(rename = "type")]
    pub event_type: EventType,
    /// 相对视频起点秒数，发送端权威触发时间。
    pub send_at: f64,
    /// 最终弹幕文本（1~100 字）。cue 的 text 为短标签（如【上车】/【卖点】），下游不发送。
    pub text: String,
    /// read/cta/cue 专用：主播念出/喊出/讲到该点的时刻（秒）。filler 为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spoken_at: Option<f64>,
    /// read 专用：本条实际提前量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_time_sec: Option<f64>,
    /// read/cta/cue 专用：可信度 0~1。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// read/cta/cue 专用：该段 ASR 原始转写。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
    /// 账号分配提示：auto 或稳定虚拟观众 ID。cue 为 None（不发送）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_hint: Option<String>,
    /// 人设提示。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<Persona>,
    /// 人工校对备注。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// cta 专用：喊跟发后延迟多少秒开始刷屏。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_sec: Option<f64>,
    /// cta 专用：同一次刷屏的分组 ID，同组多条由不同账号同发。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_group: Option<String>,
    /// cue 专用：锚点种类。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cue_kind: Option<CueKind>,
    /// cue 专用：AI 分析载荷，结构开放。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
