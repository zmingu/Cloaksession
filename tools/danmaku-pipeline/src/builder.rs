//! 脚本组装：把识别出的 read + 生成的 filler 汇成符合契约的 DanmakuScript。
//!
//! - read 事件：send_at = max(0, spoken_at - lead_time)，写入溯源字段；
//! - 低于置信度阈值的 read 丢弃；
//! - filler 事件：仅时刻 + 文本；
//! - 全部按 send_at 升序，重排 id，并轮换分配 account_hint。

use crate::config::PipelineConfig;
use crate::detect::DetectedRead;
use crate::filler::{generate_fillers, FillerItem};
use crate::model::{DanmakuScript, Event, EventType, Meta, Persona, CONTRACT_VERSION};

/// 脚本组装器。
pub struct ScriptBuilder {
    config: PipelineConfig,
}

impl ScriptBuilder {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }

    /// 组装脚本。`duration` 为录像总时长。
    pub fn build(&self, reads: &[DetectedRead], duration: f64) -> DanmakuScript {
        let cfg = &self.config;

        // 1) read → 事件（先过滤置信度）。
        let mut events: Vec<Event> = Vec::new();
        let mut kept_reads: Vec<&DetectedRead> = Vec::new();
        for r in reads {
            if r.confidence < cfg.confidence_threshold {
                continue;
            }
            kept_reads.push(r);
            let send_at = (r.spoken_at - cfg.default_lead_time_sec).max(0.0);
            events.push(Event {
                id: String::new(), // 排序后统一赋值
                event_type: EventType::Read,
                send_at: round2(send_at),
                text: r.text.clone(),
                spoken_at: Some(round2(r.spoken_at)),
                lead_time_sec: Some(cfg.default_lead_time_sec),
                confidence: Some(round2(r.confidence)),
                source_text: Some(r.source_text.clone()),
                account_hint: None, // 排序后统一赋值
                persona: r.persona,
                note: None,
            });
        }

        // 2) filler → 事件（基于保留下来的 read 做避让与配比）。
        let kept_owned: Vec<DetectedRead> = kept_reads.iter().map(|r| (*r).clone()).collect();
        let fillers: Vec<FillerItem> = generate_fillers(&kept_owned, duration, cfg);
        for f in fillers {
            events.push(Event {
                id: String::new(),
                event_type: EventType::Filler,
                send_at: f.send_at,
                text: f.text,
                spoken_at: None,
                lead_time_sec: None,
                confidence: None,
                source_text: None,
                account_hint: None,
                persona: Some(Persona::Fan),
                note: None,
            });
        }

        // 3) 按 send_at 升序。
        events.sort_by(|a, b| a.send_at.partial_cmp(&b.send_at).unwrap_or(std::cmp::Ordering::Equal));

        // 4) 重排 id，轮换分配 account_hint。
        for (i, ev) in events.iter_mut().enumerate() {
            ev.id = format!("evt-{:04}", i + 1);
            ev.account_hint = Some(assign_account(i, cfg.viewer_count));
        }

        let read_count = events.iter().filter(|e| e.event_type == EventType::Read).count();
        let filler_count = events.len() - read_count;
        let actual_ratio = if read_count > 0 {
            Some(round2(filler_count as f64 / read_count as f64))
        } else {
            Some(cfg.filler_ratio)
        };

        DanmakuScript {
            version: CONTRACT_VERSION.to_string(),
            meta: Meta {
                platform: cfg.platform.clone(),
                source_video: None,
                video_duration_sec: round2(duration),
                timezone: Some("Asia/Shanghai".to_string()),
                generated_at: Some(now_rfc3339()),
                generator: Some(format!("danmaku-pipeline {}", env!("CARGO_PKG_VERSION"))),
                asr_model: cfg.asr_model.clone(),
                llm_model: cfg.llm_model.clone(),
                default_lead_time_sec: Some(cfg.default_lead_time_sec),
                filler_ratio: actual_ratio,
            },
            events,
        }
    }
}

/// 轮换分配虚拟观众 ID；viewer_count=0 时用 auto。
fn assign_account(index: usize, viewer_count: u32) -> String {
    if viewer_count == 0 {
        return "auto".to_string();
    }
    let n = (index as u32) % viewer_count + 1;
    format!("viewer-{n}")
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}
