//! 规则版弹幕识别器。
//!
//! 思路：直播中主播念弹幕常带明显提示语（"有个宝宝问…""弹幕说…""刚有人问…"）。
//! 本识别器按这些提示语切出弹幕正文，剥离口语前缀和尾部语气词，给出可信度。
//! 这是离线可用的基线；语义更强的还原由 `llm` 版负责（预留）。

use super::{DanmakuDetector, DetectedRead};
use crate::model::Persona;
use crate::transcript::Transcript;

/// 强提示语：命中即高置信（主播明确在转述观众）。
const STRONG_CUES: &[&str] = &[
    "有个宝宝", "有位宝宝", "这位宝宝", "有宝宝", "有个家人", "家人们问", "家人们说",
    "有个朋友", "有朋友问", "有人问", "有人说", "有粉丝问", "有粉丝说",
    "弹幕说", "弹幕问", "弹幕在问", "刚刚弹幕", "刚才弹幕", "刚有人问", "刚才有人",
    "评论区问", "评论区说", "有人在问", "有人刷",
];

/// 提示语后常见的口语引导前缀，需从正文剥离。
const LEAD_FILLERS: &[&str] = &[
    "在问我说", "在问我", "问我说", "在问", "在说", "想问一下", "想问问", "想问",
    "问的是", "说的是", "问道", "说道", "问", "说", "刷", "打",
];

/// 尾部语气词/口头语，需从正文剥离。
const TAIL_FILLERS: &[&str] = &[
    "哈", "呀", "呢", "啊", "哦", "喔", "嘛", "啦", "呗", "哦对", "我说一下",
    "我看看", "我念一下", "对吧",
];

/// 买家意图关键词，命中则标记 persona=buyer。
const BUYER_KEYWORDS: &[&str] = &[
    "多少钱", "价格", "链接", "优惠", "便宜", "尺码", "尺寸", "包邮", "颜色", "现货", "库存", "怎么买",
];

/// 断句标点。
const PUNCTS: &[char] = &['，', '。', '！', '？', '、', '；', ',', '.', '!', '?', ';', ' '];

/// 规则版识别器。
#[derive(Debug, Default)]
pub struct RuleDetector;

impl DanmakuDetector for RuleDetector {
    fn detect(&self, transcript: &Transcript) -> Vec<DetectedRead> {
        let mut out = Vec::new();
        for seg in &transcript.segments {
            if let Some((text, conf)) = extract_from_segment(&seg.text) {
                if text.chars().count() < 2 {
                    continue; // 太短，噪声
                }
                let persona = if BUYER_KEYWORDS.iter().any(|k| text.contains(k)) {
                    Some(Persona::Buyer)
                } else {
                    None
                };
                out.push(DetectedRead {
                    spoken_at: seg.start,
                    text,
                    source_text: seg.text.trim().to_string(),
                    confidence: conf,
                    persona,
                });
            }
        }
        out
    }
}

/// 从一段文本尝试抽取弹幕正文，返回（正文, 置信度）。
fn extract_from_segment(segment: &str) -> Option<(String, f64)> {
    // 找到第一个命中的强提示语。
    let (cue_pos, cue) = STRONG_CUES
        .iter()
        .filter_map(|c| segment.find(c).map(|p| (p, *c)))
        .min_by_key(|(p, _)| *p)?;

    // 提示语之后的部分。
    let after = &segment[cue_pos + cue.len()..];
    // 到第一个断句标点为止，作为弹幕正文候选。
    let raw = match after.find(PUNCTS) {
        Some(idx) => &after[..idx],
        None => after,
    };
    let cleaned = clean_body(raw);
    if cleaned.is_empty() {
        return None;
    }
    // 强提示语基线 0.9；正文越完整略降噪声风险，这里给固定档位。
    let confidence = 0.9;
    Some((cleaned, confidence))
}

/// 剥离前导口语引导与尾部语气词。
fn clean_body(raw: &str) -> String {
    let mut s = raw.trim();
    // 反复剥离前缀（可能叠加，如"在问"+"说"）。
    let mut changed = true;
    while changed {
        changed = false;
        for lead in LEAD_FILLERS {
            if let Some(rest) = s.strip_prefix(lead) {
                s = rest.trim_start();
                changed = true;
            }
        }
    }
    // 反复剥离尾部语气词。
    changed = true;
    while changed {
        changed = false;
        for tail in TAIL_FILLERS {
            if let Some(rest) = s.strip_suffix(tail) {
                s = rest.trim_end();
                changed = true;
            }
        }
    }
    s.trim().to_string()
}
