//! 转写中间模型：带时间戳的分段/词，以及外部 ASR 产物导入。
//!
//! 兼容 OpenAI/faster-whisper/whisperX 的 `verbose_json`（含 segments、可选 words），
//! 也支持从 `.srt` 字幕导入（无词级时间戳）。

use serde::{Deserialize, Serialize};

use crate::error::PipelineError;

/// 词级时间戳。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    /// 词文本。不同工具用 word 或 text 字段。
    #[serde(alias = "text")]
    pub word: String,
    /// 起始秒。
    pub start: f64,
    /// 结束秒。
    pub end: f64,
}

/// 一段转写。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// 段起始秒。
    pub start: f64,
    /// 段结束秒。
    pub end: f64,
    /// 段文本。
    pub text: String,
    /// 可选词级时间戳。
    #[serde(default)]
    pub words: Vec<Word>,
}

/// 整段录像的转写结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    /// 语言代码，如 zh。
    #[serde(default)]
    pub language: Option<String>,
    /// 总时长（秒），可选。
    #[serde(default)]
    pub duration: Option<f64>,
    /// 分段列表。
    pub segments: Vec<Segment>,
}

impl Transcript {
    /// 录像时长：优先 duration，否则取最后一段结束时间。
    pub fn total_duration(&self) -> f64 {
        if let Some(d) = self.duration {
            return d;
        }
        self.segments.last().map(|s| s.end).unwrap_or(0.0)
    }

    /// 从 whisper 风格 verbose_json 文本解析。
    pub fn from_whisper_json(raw: &str) -> Result<Self, PipelineError> {
        let t: Transcript = serde_json::from_str(raw)?;
        Ok(t)
    }

    /// 从 SRT 字幕文本解析（无词级时间戳）。
    pub fn from_srt(raw: &str) -> Result<Self, PipelineError> {
        let mut segments = Vec::new();
        // SRT 以空行分块：序号 / 时间轴 / 一至多行文本。
        for block in raw.split("\n\n").flat_map(|b| b.split("\r\n\r\n")) {
            let lines: Vec<&str> = block.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            if lines.len() < 2 {
                continue;
            }
            // 找到含 "-->" 的时间轴行。
            let time_idx = match lines.iter().position(|l| l.contains("-->")) {
                Some(i) => i,
                None => continue,
            };
            let (start, end) = match parse_srt_time_line(lines[time_idx]) {
                Some(v) => v,
                None => continue,
            };
            let text = lines[time_idx + 1..].join(" ");
            if text.is_empty() {
                continue;
            }
            segments.push(Segment {
                start,
                end,
                text,
                words: Vec::new(),
            });
        }
        if segments.is_empty() {
            return Err(PipelineError::Parse("SRT 未解析出任何字幕段".into()));
        }
        Ok(Transcript {
            language: None,
            duration: None,
            segments,
        })
    }

    /// 按扩展名自动选择解析方式。
    pub fn from_file_content(path: &str, raw: &str) -> Result<Self, PipelineError> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".srt") {
            Self::from_srt(raw)
        } else {
            Self::from_whisper_json(raw)
        }
    }
}

/// 解析形如 `00:00:12,300 --> 00:00:15,800` 的时间轴行。
fn parse_srt_time_line(line: &str) -> Option<(f64, f64)> {
    let mut parts = line.split("-->");
    let start = parse_srt_timestamp(parts.next()?.trim())?;
    let end = parse_srt_timestamp(parts.next()?.trim())?;
    Some((start, end))
}

/// 解析 `HH:MM:SS,mmm`（逗号或点分隔毫秒）为秒。
fn parse_srt_timestamp(s: &str) -> Option<f64> {
    let s = s.replace(',', ".");
    let (hms, millis) = match s.split_once('.') {
        Some((a, b)) => (a, b),
        None => (s.as_str(), "0"),
    };
    let comps: Vec<&str> = hms.split(':').collect();
    if comps.len() != 3 {
        return None;
    }
    let h: f64 = comps[0].parse().ok()?;
    let m: f64 = comps[1].parse().ok()?;
    let sec: f64 = comps[2].parse().ok()?;
    let ms: f64 = format!("0.{millis}").parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec + ms)
}
