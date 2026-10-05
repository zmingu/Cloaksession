//! LLM 版弹幕识别器。
//!
//! 相比规则版，LLM 能理解语义：识别隐式回应（"对，这个是纯棉的" ← "是纯棉的吗"），
//! 还原口语化弹幕原文，给出置信度与人设。
//!
//! 设计要点（离线友好、可插拔）：
//! - `ChatClient` trait：抽象一次对话补全调用，传输层可换。
//! - `CannedClient`：从固定字符串返回响应，用于**离线回放/验证**解析与映射链路。
//! - `HttpChatClient`：真正的 HTTP 客户端，走 Cargo feature `llm-http` 门控
//!   （默认关闭，保证默认构建零网络依赖、离线可过）。
//! - `build_messages` / `parse_llm_json` / `map_to_detected` 均为纯函数，可离线验证。
//! - 识别失败（网络/解析错误）时 `detect` 返回空，调用方应回退到规则版。

use serde::Deserialize;

use super::{DanmakuDetector, DetectedCta, DetectedCue, DetectedItem, DetectedRead};
use crate::error::PipelineError;
use crate::model::{CueKind, Persona};
use crate::transcript::Transcript;

/// LLM 连接与生成配置。
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// 兼容 OpenAI Chat Completions 的 base_url，如 https://api.openai.com/v1。
    pub base_url: String,
    /// API key。
    pub api_key: String,
    /// 模型名。
    pub model: String,
    /// 采样温度。
    pub temperature: f32,
    /// 每次请求最多携带的转写分段数（长视频分块，避免超上下文）。
    pub chunk_size: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
            temperature: 0.2,
            chunk_size: 200,
        }
    }
}

impl LlmConfig {
    /// 从环境变量读取：DANMAKU_LLM_BASE_URL / DANMAKU_LLM_API_KEY / DANMAKU_LLM_MODEL。
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("DANMAKU_LLM_BASE_URL") {
            if !v.is_empty() {
                cfg.base_url = v;
            }
        }
        if let Ok(v) = std::env::var("DANMAKU_LLM_API_KEY") {
            cfg.api_key = v;
        }
        if let Ok(v) = std::env::var("DANMAKU_LLM_MODEL") {
            if !v.is_empty() {
                cfg.model = v;
            }
        }
        cfg
    }
}

/// 一次对话补全调用的抽象。
pub trait ChatClient {
    /// 给定 system 与 user 文本，返回模型输出的完整文本。
    fn complete(&self, system: &str, user: &str) -> Result<String, PipelineError>;
}

/// 允许把 `Box<dyn ChatClient>` 当作客户端使用（便于运行时选择传输实现）。
impl ChatClient for Box<dyn ChatClient> {
    fn complete(&self, system: &str, user: &str) -> Result<String, PipelineError> {
        (**self).complete(system, user)
    }
}

/// 离线回放客户端：忽略输入，始终返回预置响应。用于验证解析/映射链路。
#[derive(Debug, Clone)]
pub struct CannedClient {
    pub response: String,
}

impl ChatClient for CannedClient {
    fn complete(&self, _system: &str, _user: &str) -> Result<String, PipelineError> {
        Ok(self.response.clone())
    }
}

/// LLM 识别器，泛型于传输客户端。
pub struct LlmDetector<C: ChatClient> {
    client: C,
    chunk_size: usize,
}

impl<C: ChatClient> LlmDetector<C> {
    pub fn new(client: C, chunk_size: usize) -> Self {
        Self {
            client,
            chunk_size: chunk_size.max(1),
        }
    }
}

impl<C: ChatClient> DanmakuDetector for LlmDetector<C> {
    fn detect(&self, transcript: &Transcript) -> Vec<DetectedItem> {
        let mut out = Vec::new();
        // 长视频分块调用，块内分段索引是全局索引，便于映射回原段。
        for (chunk_start, chunk) in chunks_with_offset(&transcript.segments, self.chunk_size) {
            let (system, user) = build_messages(chunk, chunk_start);
            let raw = match self.client.complete(&system, &user) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[warn] LLM 调用失败，跳过该块并回退: {e}");
                    continue;
                }
            };
            let items = match parse_llm_json(&raw) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[warn] LLM 响应解析失败，跳过该块: {e}");
                    continue;
                }
            };
            out.extend(map_to_detected(&items, transcript));
        }
        out
    }
}

/// LLM 期望输出的单条结构。kind 三选一：read（念/回应弹幕）、cta（跟发刷屏）、cue（上车/卖点锚点）。
#[derive(Debug, Clone, Deserialize)]
pub struct LlmItem {
    /// 该条对应的转写分段全局索引（优先用于定位 spoken_at 与 source_text）。
    #[serde(default)]
    pub seg: Option<usize>,
    /// 备用：模型直接给的时刻（秒）。
    #[serde(default)]
    pub spoken_at: Option<f64>,
    /// read：还原的弹幕原文；cta：跟进文本（如 "1"）；cue：短标签（如【上车】/【卖点】）。
    pub text: String,
    /// 置信度 0~1。
    #[serde(default)]
    pub confidence: Option<f64>,
    /// 人设：buyer/fan/newcomer/neutral/other。
    #[serde(default)]
    pub persona: Option<String>,
    /// 条目种类：read/cta/cue，缺省 read（兼容旧回放）。
    #[serde(default)]
    pub kind: Option<String>,
    /// cta：本次刷屏条数。
    #[serde(default)]
    pub burst: Option<usize>,
    /// cta：喊出后延迟多少秒开始刷屏。
    #[serde(default)]
    pub delay_sec: Option<f64>,
    /// cue：onboard_call/pitch/other，缺省 other。
    #[serde(default)]
    pub cue_kind: Option<String>,
    /// cue：AI 分析载荷（如试用/退款/运费险/价格/产品），结构开放。
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// 构造 system 与 user 消息。`offset` 为本块首段的全局索引。
pub fn build_messages(segments: &[crate::transcript::Segment], offset: usize) -> (String, String) {
    let system = "你是直播运营助手。下面是一段直播录像的语音转写（主播说话），\\
        每行是【全局分段序号】【起始秒】文本。你的任务：找出三类时刻并只输出 JSON 数组。\\
        输出只能是 JSON 数组本身（可带```json围栏），数组外禁止任何文字，禁止总结、禁止换字段名。\\
        字段名只能用 kind/seg/text/confidence/persona/burst/delay_sec/cue_kind/data，\\
        禁止输出 type/start_time/end_time/summary 等任何其他字段。无命中必须输出 []。\\
        ①read：主播在‘念弹幕/回应观众提问’的时刻，text 还原观众当时最可能发出的弹幕原文\\
        （如主播说‘对，这个是纯棉的’可反推弹幕‘是纯棉的吗’，text 是观众口吻不是主播原话）。\\
        ②cta：主播喊观众一起跟发刷屏（如扣1/打1/刷1），text 必须原样照抄主播喊的跟发词\\
        （字幕说打什么弹幕就打什么，如“打一个有”就输出“有”，禁止纠错成“1”），并给 burst（条数）与 delay_sec（秒）。\\
        ③cue：主播讲到上车点/卖点保障话术的锚点，text 为短标签如【上车】/【卖点】，\\
        并给 cue_kind（onboard_call=上车/挂链，pitch=卖点/保障，other=其他）与 data（AI 分析载荷如试用/退款/运费险/价格/产品）。\\
        每个元素形如：\\
        {\\\"kind\\\":\\\"read|cta|cue\\\",\\\"seg\\\":分段序号(整数),\\\"text\\\":\\\"弹幕原文/跟进文本/短标签\\\",\\
        \\\"confidence\\\":0~1,\\\"persona\\\":\\\"buyer|fan|newcomer|neutral|other\\\",\\
        \\\"burst\\\":条数(cta用),\\\"delay_sec\\\":秒(cta用),\\\"cue_kind\\\":\\\"onboard_call|pitch|other\\\"(cue用),\\\"data\\\":{}}(cue用)。\\
        规则：只挑确实命中的分段；不确定就降低 confidence 或不输出该条；相亲/闲聊段无命中直接输出 []。"
        .to_string();

    let mut user = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let gidx = offset + i;
        user.push_str(&format!("[{}] {:.1}s {}\n", gidx, seg.start, seg.text.trim()));
    }
    (system, user)
}

/// 从 LLM 原始输出中解析 JSON 数组（容忍代码围栏与前后赘述）。
pub fn parse_llm_json(raw: &str) -> Result<Vec<LlmItem>, PipelineError> {
    let slice = extract_json_array(raw)
        .ok_or_else(|| PipelineError::Parse("LLM 响应中未找到 JSON 数组".into()))?;
    let items: Vec<LlmItem> = serde_json::from_str(slice)?;
    Ok(items)
}

/// 截取第一个 '[' 到与之匹配的 ']' 之间的子串。
fn extract_json_array(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let start = raw.find('[')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// 把 LLM 条目映射为 DetectedItem。优先用 seg 索引定位 spoken_at 与 source_text。
/// text 按字符截断 100 字，confidence 钳到 0~1。
pub fn map_to_detected(items: &[LlmItem], transcript: &Transcript) -> Vec<DetectedItem> {
    let mut out = Vec::new();
    for it in items {
        let text: String = it.text.trim().chars().take(100).collect();
        if text.chars().count() < 1 {
            continue;
        }
        let (spoken_at, source_text) = match it.seg.and_then(|s| transcript.segments.get(s)) {
            Some(seg) => (seg.start, seg.text.trim().to_string()),
            None => (it.spoken_at.unwrap_or(0.0), String::new()),
        };
        let confidence = it.confidence.unwrap_or(0.8).clamp(0.0, 1.0);
        let kind = it.kind.as_deref().unwrap_or("read").trim().to_ascii_lowercase();
        match kind.as_str() {
            "cta" => {
                out.push(DetectedItem::Cta(DetectedCta {
                    spoken_at,
                    text,
                    source_text,
                    confidence,
                    burst: it.burst.unwrap_or(5).max(1),
                    delay_sec: it.delay_sec.unwrap_or(2.5).max(0.0),
                    persona: parse_persona(it.persona.as_deref()),
                }));
            }
            "cue" => {
                out.push(DetectedItem::Cue(DetectedCue {
                    spoken_at,
                    cue_kind: parse_cue_kind(it.cue_kind.as_deref()),
                    label: text,
                    source_text,
                    confidence,
                    data: it.data.clone(),
                }));
            }
            _ => {
                // read 兜底：兼容旧回放（无 kind 字段）。
                if text.chars().count() < 2 {
                    continue;
                }
                out.push(DetectedItem::Read(DetectedRead {
                    spoken_at,
                    text,
                    source_text,
                    confidence,
                    persona: parse_persona(it.persona.as_deref()),
                }));
            }
        }
    }
    out
}

fn parse_cue_kind(s: Option<&str>) -> CueKind {
    match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
        Some("onboard_call") | Some("onboard") | Some("onboarding") => CueKind::OnboardCall,
        Some("pitch") => CueKind::Pitch,
        _ => CueKind::Other,
    }
}

fn parse_persona(s: Option<&str>) -> Option<Persona> {
    match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
        Some("buyer") => Some(Persona::Buyer),
        Some("fan") => Some(Persona::Fan),
        Some("newcomer") => Some(Persona::Newcomer),
        Some("neutral") => Some(Persona::Neutral),
        Some("other") => Some(Persona::Other),
        _ => None,
    }
}

/// 将分段按 chunk_size 切块，返回（本块首段全局索引, 本块切片）。
fn chunks_with_offset(
    segments: &[crate::transcript::Segment],
    chunk_size: usize,
) -> Vec<(usize, &[crate::transcript::Segment])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < segments.len() {
        let end = (i + chunk_size).min(segments.len());
        out.push((i, &segments[i..end]));
        i = end;
    }
    out
}

// ===== 真正的 HTTP 客户端：feature 门控，默认不编译，保证默认构建离线可过 =====
#[cfg(feature = "llm-http")]
pub use http_client::HttpChatClient;

#[cfg(feature = "llm-http")]
mod http_client {
    use super::{ChatClient, LlmConfig};
    use crate::error::PipelineError;

    /// 基于 reqwest 阻塞客户端的 OpenAI 兼容实现。
    pub struct HttpChatClient {
        config: LlmConfig,
        http: reqwest::blocking::Client,
    }

    impl HttpChatClient {
        pub fn new(config: LlmConfig) -> Result<Self, PipelineError> {
            let http = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .map_err(|e| PipelineError::Config(format!("构建 HTTP 客户端失败: {e}")))?;
            Ok(Self { config, http })
        }
    }

    impl ChatClient for HttpChatClient {
        fn complete(&self, system: &str, user: &str) -> Result<String, PipelineError> {
            if self.config.api_key.is_empty() {
                return Err(PipelineError::Config(
                    "缺少 API key（设置 DANMAKU_LLM_API_KEY）".into(),
                ));
            }
            let url = format!("{}/chat/completions", self.config.base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": self.config.model,
                "temperature": self.config.temperature,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user}
                ]
            });
            let resp = self
                .http
                .post(&url)
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .send()
                .map_err(|e| PipelineError::Parse(format!("LLM 请求失败: {e}")))?;
            if !resp.status().is_success() {
                return Err(PipelineError::Parse(format!(
                    "LLM 返回非 2xx: {}",
                    resp.status()
                )));
            }
            let val: serde_json::Value = resp
                .json()
                .map_err(|e| PipelineError::Parse(format!("解析 LLM JSON 失败: {e}")))?;
            let content = val["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| PipelineError::Parse("LLM 响应缺少 choices[0].message.content".into()))?;
            Ok(content.to_string())
        }
    }
}
