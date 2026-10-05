//! 管线配置：提前量、氛围比例、置信度阈值、词库等。

use serde::{Deserialize, Serialize};

/// 生成配置。可从 JSON 载入，也可用 CLI 参数覆盖。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// 目标平台。
    #[serde(default = "default_platform")]
    pub platform: String,
    /// 默认提前量（秒）：send_at = spoken_at - lead_time。
    #[serde(default = "default_lead")]
    pub default_lead_time_sec: f64,
    /// 氛围弹幕相对保真弹幕的数量比例（0 表示不补）。
    #[serde(default = "default_filler_ratio")]
    pub filler_ratio: f64,
    /// read/cta/cue 置信度阈值，低于此值的识别结果丢弃。
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f64,
    /// 虚拟观众数量，用于给事件轮换分配 account_hint。
    #[serde(default = "default_viewers")]
    pub viewer_count: u32,
    /// 氛围弹幕词库。
    #[serde(default = "default_filler_pool")]
    pub filler_pool: Vec<String>,
    /// ASR 模型标识（仅溯源写入 meta）。
    #[serde(default)]
    pub asr_model: Option<String>,
    /// LLM 模型标识（仅溯源写入 meta）。
    #[serde(default)]
    pub llm_model: Option<String>,
    /// cta 默认延迟（秒）：首条 send = spoken + delay。
    #[serde(default = "default_cta_delay")]
    pub cta_delay_sec: f64,
    /// cta 默认刷屏条数。
    #[serde(default = "default_cta_burst")]
    pub cta_burst: usize,
    /// cta 刷屏条间 stagger（秒）。
    #[serde(default = "default_cta_stagger")]
    pub cta_stagger_sec: f64,
}

fn default_platform() -> String {
    "kuaishou".to_string()
}

fn default_lead() -> f64 {
    5.0
}

fn default_filler_ratio() -> f64 {
    0.5
}

fn default_confidence() -> f64 {
    0.6
}

fn default_viewers() -> u32 {
    8
}

fn default_cta_delay() -> f64 {
    2.5
}

fn default_cta_burst() -> usize {
    5
}

fn default_cta_stagger() -> f64 {
    0.5
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            platform: default_platform(),
            default_lead_time_sec: default_lead(),
            filler_ratio: default_filler_ratio(),
            confidence_threshold: default_confidence(),
            viewer_count: default_viewers(),
            filler_pool: default_filler_pool(),
            asr_model: None,
            llm_model: None,
            cta_delay_sec: default_cta_delay(),
            cta_burst: default_cta_burst(),
            cta_stagger_sec: default_cta_stagger(),
        }
    }
}

impl PipelineConfig {
    /// 从 JSON 文本载入，缺失字段用默认值。
    pub fn from_json(raw: &str) -> Result<Self, crate::error::PipelineError> {
        let cfg: PipelineConfig = serde_json::from_str(raw)?;
        Ok(cfg)
    }
}

/// 默认氛围弹幕词库（中性、口语、无导流违规）。
pub fn default_filler_pool() -> Vec<String> {
    [
        "主播讲得好细",
        "这个看着不错",
        "求个链接",
        "价格能便宜点吗",
        "有没有优惠",
        "刚来的，主播介绍下",
        "这个有别的颜色吗",
        "尺码怎么选",
        "支持主播",
        "质量怎么样呀",
        "包邮吗",
        "看着挺喜欢的",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
