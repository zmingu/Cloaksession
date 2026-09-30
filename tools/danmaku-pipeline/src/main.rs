//! danmaku-pipeline CLI。
//!
//! 用法：
//!   danmaku-pipeline build --transcript <whisper.json|.srt> [--out <script.json>]
//!         [--platform kuaishou] [--lead 5.0] [--filler-ratio 0.5]
//!         [--confidence 0.6] [--viewers 8] [--config <config.json>]
//!         [--asr-model <name>] [--llm-model <name>]
//!
//!   danmaku-pipeline transcribe --input <video> [--out-audio <wav>]
//!         本地 whisper.cpp 绑定尚未接入，当前会提示改用外部 ASR + build 导入。

use std::process::exit;

use danmaku_pipeline::build_from_transcript;
use danmaku_pipeline::config::PipelineConfig;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_usage();
        exit(2);
    }
    let cmd = args[1].as_str();
    let rest = &args[2..];
    let result = match cmd {
        "build" => cmd_build(rest),
        "transcribe" => cmd_transcribe(rest),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        other => {
            eprintln!("未知子命令: {other}\n");
            print_usage();
            exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("[error] {e}");
        exit(1);
    }
}

fn cmd_build(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let opts = parse_opts(args);
    let transcript_path = opts
        .get("transcript")
        .ok_or("缺少 --transcript <whisper.json|.srt>")?
        .clone();

    // 基础配置：可先从 --config 载入，再用单项参数覆盖。
    let mut config = match opts.get("config") {
        Some(p) => {
            let raw = std::fs::read_to_string(p)?;
            PipelineConfig::from_json(&raw)?
        }
        None => PipelineConfig::default(),
    };
    if let Some(v) = opts.get("platform") {
        config.platform = v.clone();
    }
    if let Some(v) = opts.get("lead") {
        config.default_lead_time_sec = v.parse().map_err(|_| "非法 --lead")?;
    }
    if let Some(v) = opts.get("filler-ratio") {
        config.filler_ratio = v.parse().map_err(|_| "非法 --filler-ratio")?;
    }
    if let Some(v) = opts.get("confidence") {
        config.confidence_threshold = v.parse().map_err(|_| "非法 --confidence")?;
    }
    if let Some(v) = opts.get("viewers") {
        config.viewer_count = v.parse().map_err(|_| "非法 --viewers")?;
    }
    if let Some(v) = opts.get("asr-model") {
        config.asr_model = Some(v.clone());
    }
    if let Some(v) = opts.get("llm-model") {
        config.llm_model = Some(v.clone());
    }

    let raw = std::fs::read_to_string(&transcript_path)?;
    let script = build_from_transcript(&transcript_path, &raw, config)?;
    let json = serde_json::to_string_pretty(&script)?;

    match opts.get("out") {
        Some(out) => {
            std::fs::write(out, json)?;
            eprintln!(
                "[ok] 生成 {} 条事件 → {}",
                script.events.len(),
                out
            );
        }
        None => println!("{json}"),
    }
    Ok(())
}

fn cmd_transcribe(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let opts = parse_opts(args);
    let _input = opts.get("input").ok_or("缺少 --input <video>")?;
    eprintln!(
        "本地 whisper.cpp 绑定尚未接入。请先用外部 ASR（whisper/faster-whisper/whisperX）\n\
         产出 verbose_json 或 SRT，再运行：danmaku-pipeline build --transcript <文件>。"
    );
    Ok(())
}

/// 极简 --key value 解析（避免引入 clap）。支持 --flag（无值时置空串）。
fn parse_opts(args: &[String]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(key) = a.strip_prefix("--") {
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                map.insert(key.to_string(), args[i + 1].clone());
                i += 2;
            } else {
                map.insert(key.to_string(), String::new());
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    map
}

fn print_usage() {
    eprintln!(
        "danmaku-pipeline — 直播录像 → 弹幕脚本（上游管线）\n\n\
         子命令:\n\
         \x20 build       从转写(whisper.json/SRT)生成符合契约的弹幕脚本 JSON\n\
         \x20 transcribe  (预留) 本地 whisper.cpp 转写\n\n\
         build 参数:\n\
         \x20 --transcript <路径>   必填，whisper verbose_json 或 .srt\n\
         \x20 --out <路径>          输出脚本 JSON；缺省打印到 stdout\n\
         \x20 --platform <名>       默认 kuaishou\n\
         \x20 --lead <秒>           默认提前量，默认 5.0\n\
         \x20 --filler-ratio <比例> 氛围弹幕:保真弹幕，默认 0.5\n\
         \x20 --confidence <阈值>   read 置信度阈值，默认 0.6\n\
         \x20 --viewers <数量>      虚拟观众数（轮换 account_hint），默认 8\n\
         \x20 --config <路径>       先从 JSON 载入配置，再被上述单项覆盖\n\
         \x20 --asr-model <名>      写入 meta 溯源\n\
         \x20 --llm-model <名>      写入 meta 溯源"
    );
}
