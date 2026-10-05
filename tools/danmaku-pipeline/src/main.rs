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

use std::collections::HashMap;
use std::error::Error;
use std::process::exit;

use danmaku_pipeline::build_with_detector;
use danmaku_pipeline::config::PipelineConfig;
use danmaku_pipeline::detect::llm::{CannedClient, ChatClient, LlmDetector};
use danmaku_pipeline::detect::rule::RuleDetector;
use danmaku_pipeline::detect::FallbackDetector;
use danmaku_pipeline::transcript::Transcript;

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
    if let Some(v) = opts.get("cta-delay") {
        config.cta_delay_sec = v.parse().map_err(|_| "非法 --cta-delay")?;
    }
    if let Some(v) = opts.get("cta-burst") {
        config.cta_burst = v.parse().map_err(|_| "非法 --cta-burst")?;
    }
    if let Some(v) = opts.get("cta-stagger") {
        config.cta_stagger_sec = v.parse().map_err(|_| "非法 --cta-stagger")?;
    }
    if let Some(v) = opts.get("asr-model") {
        config.asr_model = Some(v.clone());
    }
    if let Some(v) = opts.get("llm-model") {
        config.llm_model = Some(v.clone());
    }

    let raw = std::fs::read_to_string(&transcript_path)?;
    let transcript = Transcript::from_file_content(&transcript_path, &raw)?;

    // 检测器选择：rule（默认，离线）| llm | auto（LLM 为主，规则兜底）。
    let detector_kind = opts.get("detector").map(|s| s.as_str()).unwrap_or("rule");
    let chunk: usize = opts
        .get("llm-chunk")
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let llm_client = build_llm_client(&opts)?;
    let rule = RuleDetector;

    let script = match detector_kind {
        "rule" => build_with_detector(&transcript, &rule, config),
        "llm" => {
            let client = llm_client.ok_or(
                "--detector llm 需要 --llm-response <file>（离线回放），\
                 或用 --features llm-http 构建并设置 DANMAKU_LLM_BASE_URL/API_KEY/MODEL",
            )?;
            let det = LlmDetector::new(client, chunk);
            build_with_detector(&transcript, &det, config)
        }
        "auto" => match llm_client {
            Some(client) => {
                let llm = LlmDetector::new(client, chunk);
                let fb = FallbackDetector {
                    primary: &llm,
                    fallback: &rule,
                };
                build_with_detector(&transcript, &fb, config)
            }
            None => {
                eprintln!("[warn] 未配置 LLM 客户端，auto 退化为规则版");
                build_with_detector(&transcript, &rule, config)
            }
        },
        other => return Err(format!("未知 --detector: {other}（rule|llm|auto）").into()),
    };
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

/// 构造 LLM 客户端：
/// - 若给了 --llm-response <file>，用 CannedClient 做离线回放（无需网络）；
/// - 否则在启用 llm-http feature 时用真正的 HTTP 客户端（读环境变量）；
/// - 都没有则返回 None。
fn build_llm_client(opts: &HashMap<String, String>) -> Result<Option<Box<dyn ChatClient>>, Box<dyn Error>> {
    if let Some(path) = opts.get("llm-response") {
        let resp = std::fs::read_to_string(path)?;
        return Ok(Some(Box::new(CannedClient { response: resp })));
    }
    #[cfg(feature = "llm-http")]
    {
        let cfg = danmaku_pipeline::detect::llm::LlmConfig::from_env();
        let client = danmaku_pipeline::detect::llm::HttpChatClient::new(cfg)?;
        Ok(Some(Box::new(client)))
    }
    #[cfg(not(feature = "llm-http"))]
    {
        Ok(None)
    }
}

/// 极简 --key value 解析（避免引入 clap）。支持 --flag（无值时置空串）。
fn parse_opts(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
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
         \x20 --cta-delay <秒>       cta 刷屏延迟，默认 2.5\n\
         \x20 --cta-burst <条数>     cta 刷屏条数，默认 5\n\
         \x20 --cta-stagger <秒>     cta 刷屏条间 stagger，默认 0.5\n\
         \x20 --config <路径>       先从 JSON 载入配置，再被上述单项覆盖\n\
         \x20 --detector <种类>     rule(默认,离线) | llm | auto(LLM为主,规则兜底)\n\
         \x20 --llm-response <路径> 离线回放：用文件里的 LLM 响应替代真实调用\n\
         \x20 --llm-chunk <数量>    LLM 单次请求携带的分段数，默认 200\n\
         \x20 --asr-model <名>      写入 meta 溯源\n\
         \x20 --llm-model <名>      写入 meta 溯源\n\n\
         LLM 说明: --detector llm/auto 需要 --llm-response（离线回放）\n\
         或用 `cargo build --features llm-http` 构建并设置环境变量\n\
         DANMAKU_LLM_BASE_URL / DANMAKU_LLM_API_KEY / DANMAKU_LLM_MODEL。"
    );
}
