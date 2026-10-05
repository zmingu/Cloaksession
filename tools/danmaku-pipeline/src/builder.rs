//! 脚本组装：把识别出的 read/cta/cue + 生成的 filler 汇成符合契约的 DanmakuScript。
//!
//! - read 事件：send_at = max(0, spoken_at - lead_time)，写入溯源字段；
//! - cta 事件：展开 burst 条，send = spoken + delay + i*stagger，同 burst_group、不同 viewer；
//! - cue 事件：send = spoken 精确透传，account_hint=None，cue_kind + data 透传（下游不发送）；
//! - 三类都按置信度阈值过滤；text 超 100 字截断；
//! - filler 事件：仅时刻 + 文本；
//! - 全部按 send_at 升序，重排 id，并轮换分配 account_hint（cue 跳过账号）。

use crate::config::PipelineConfig;
use crate::detect::{DetectedCta, DetectedCue, DetectedItem, DetectedRead};
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
    pub fn build(&self, items: &[DetectedItem], duration: f64) -> DanmakuScript {
        let cfg = &self.config;

        // 0) 置信度过滤（三类统一）。
        let kept: Vec<&DetectedItem> = items
            .iter()
            .filter(|it| it.confidence() >= cfg.confidence_threshold)
            .collect();

        let mut events: Vec<Event> = Vec::new();
        let mut cta_seq: usize = 0;

        // 1) read / cta / cue → 事件。
        for it in &kept {
            match it {
                DetectedItem::Read(r) => {
                    events.push(read_event(r, cfg));
                }
                DetectedItem::Cta(c) => {
                    cta_seq += 1;
                    let group = format!("cta-{cta_seq:04}");
                    events.extend(cta_events(c, &group, cfg, duration));
                }
                DetectedItem::Cue(c) => {
                    events.push(cue_event(c));
                }
            }
        }

        // 2) filler → 事件（基于保留下来的全部识别条目做避让与配比）。
        let kept_owned: Vec<DetectedItem> = kept.iter().map(|r| (*r).clone()).collect();
        let fillers: Vec<FillerItem> = generate_fillers(&kept_owned, duration, cfg);
        for f in fillers {
            events.push(Event {
                id: String::new(),
                event_type: EventType::Filler,
                send_at: f.send_at,
                text: truncate100(&f.text),
                spoken_at: None,
                lead_time_sec: None,
                confidence: None,
                source_text: None,
                account_hint: None, // 排序后统一赋值
                persona: Some(Persona::Fan),
                note: None,
                delay_sec: None,
                burst_group: None,
                cue_kind: None,
                data: None,
            });
        }

        // 3) 按 send_at 升序（排序校验：send_at 单调非降）。
        events.sort_by(|a, b| a.send_at.partial_cmp(&b.send_at).unwrap_or(std::cmp::Ordering::Equal));
        debug_assert!(events.windows(2).all(|w| w[0].send_at <= w[1].send_at + f64::EPSILON));

        // 4) 重排 id，轮换分配 account_hint（cue 跳过账号，保持 None）。
        let mut account_idx: usize = 0;
        for (i, ev) in events.iter_mut().enumerate() {
            ev.id = format!("evt-{:04}", i + 1);
            if ev.event_type == EventType::Cue {
                ev.account_hint = None;
            } else {
                ev.account_hint = Some(assign_account(account_idx, cfg.viewer_count));
                account_idx += 1;
            }
        }

        let read_count = events.iter().filter(|e| e.event_type == EventType::Read).count();
        let cta_count = events.iter().filter(|e| e.event_type == EventType::Cta).count();
        let filler_count = events.iter().filter(|e| e.event_type == EventType::Filler).count();
        let denom = read_count + cta_count;
        let actual_ratio = if denom > 0 {
            Some(round2(filler_count as f64 / denom as f64))
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

fn read_event(r: &DetectedRead, cfg: &PipelineConfig) -> Event {
    let send_at = (r.spoken_at - cfg.default_lead_time_sec).max(0.0);
    Event {
        id: String::new(), // 排序后统一赋值
        event_type: EventType::Read,
        send_at: round2(send_at),
        text: truncate100(&r.text),
        spoken_at: Some(round2(r.spoken_at)),
        lead_time_sec: Some(cfg.default_lead_time_sec),
        confidence: Some(round2(r.confidence)),
        source_text: Some(r.source_text.clone()),
        account_hint: None, // 排序后统一赋值
        persona: r.persona,
        note: None,
        delay_sec: None,
        burst_group: None,
        cue_kind: None,
        data: None,
    }
}

/// cta 展开：burst 条，send = spoken + delay + i*stagger，同 burst_group、不同 viewer（账号排序后轮换），clamp 到 [0, duration]。
fn cta_events(c: &DetectedCta, group: &str, cfg: &PipelineConfig, duration: f64) -> Vec<Event> {
    let burst = c.burst.max(1);
    let delay = if c.delay_sec >= 0.0 { c.delay_sec } else { cfg.cta_delay_sec.max(0.0) };
    let stagger = cfg.cta_stagger_sec.max(0.0);
    (0..burst)
        .map(|i| {
            let send = (c.spoken_at + delay + i as f64 * stagger).clamp(0.0, duration.max(0.0));
            Event {
                id: String::new(),
                event_type: EventType::Cta,
                send_at: round2(send),
                text: truncate100(&c.text),
                spoken_at: Some(round2(c.spoken_at)),
                lead_time_sec: None,
                confidence: Some(round2(c.confidence)),
                source_text: Some(c.source_text.clone()),
                account_hint: None, // 排序后统一赋值
                persona: c.persona,
                note: None,
                delay_sec: Some(round2(delay)),
                burst_group: Some(group.to_string()),
                cue_kind: None,
                data: None,
            }
        })
        .collect()
}

/// cue 透传：send = spoken 精确到秒，account_hint=None。
fn cue_event(c: &DetectedCue) -> Event {
    Event {
        id: String::new(),
        event_type: EventType::Cue,
        send_at: round2(c.spoken_at),
        text: truncate100(&c.label),
        spoken_at: Some(round2(c.spoken_at)),
        lead_time_sec: None,
        confidence: Some(round2(c.confidence)),
        source_text: Some(c.source_text.clone()),
        account_hint: None, // cue 永不分配账号
        persona: None,
        note: None,
        delay_sec: None,
        burst_group: None,
        cue_kind: Some(c.cue_kind),
        data: c.data.clone(),
    }
}

/// text 超 100 字按字符截断，保证过 schema maxLength。
fn truncate100(s: &str) -> String {
    if s.chars().count() <= 100 {
        return s.to_string();
    }
    s.chars().take(100).collect()
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
