//! 氛围弹幕生成：在 read 事件的空档期，按比例均匀补充 filler。
//!
//! 无随机依赖：采用确定性均匀铺点，并避开每个 read 的 spoken_at 邻域，
//! 防止与"主播正在念的弹幕"撞车显假。

use crate::config::PipelineConfig;
use crate::detect::DetectedItem;

/// 一条待生成的氛围弹幕（时刻 + 文本）。
#[derive(Debug, Clone)]
pub struct FillerItem {
    pub send_at: f64,
    pub text: String,
}

/// 根据识别条数与 filler_ratio 计算要补多少条，并在时间轴上均匀铺点。
///
/// - `items`：已识别的 read+cta+cue（用其 spoken_at 作为避让点）。
/// - `duration`：录像总时长，用于确定铺点范围。
pub fn generate_fillers(
    items: &[DetectedItem],
    duration: f64,
    config: &PipelineConfig,
) -> Vec<FillerItem> {
    if config.filler_ratio <= 0.0 || config.filler_pool.is_empty() || duration <= 0.0 {
        return Vec::new();
    }
    // 目标条数：以 read+cta 数为基准；若无，则按时长兜底（每 30s 一条）。
    let base = if items.is_empty() {
        (duration / 30.0).floor() as usize
    } else {
        items.len()
    };
    let target = ((base as f64) * config.filler_ratio).round() as usize;
    if target == 0 {
        return Vec::new();
    }

    // 避让点：全部 read+cta+cue 的 spoken_at ± 3s 内不铺 filler。
    let avoid: Vec<f64> = items.iter().map(|r| r.spoken_at()).collect();
    const AVOID_RADIUS: f64 = 3.0;

    // 在 (0, duration) 内均匀取 target 个候选点，逐个避让后落点。
    let mut out = Vec::with_capacity(target);
    let step = duration / (target as f64 + 1.0);
    let mut pool_idx = 0usize;
    for k in 1..=target {
        let mut t = step * (k as f64);
        // 命中避让区则后移半步，最多尝试几次。
        let mut tries = 0;
        while avoid.iter().any(|a| (t - a).abs() < AVOID_RADIUS) && tries < 4 {
            t += step * 0.5;
            tries += 1;
        }
        if t >= duration {
            break;
        }
        let text = config.filler_pool[pool_idx % config.filler_pool.len()].clone();
        pool_idx += 1;
        out.push(FillerItem {
            send_at: round2(t),
            text,
        });
    }
    out
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
