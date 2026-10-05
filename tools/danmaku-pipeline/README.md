# danmaku-pipeline（上游管线）

直播录像 → 分析主播「念/回应弹幕」→ 生成符合[契约](../CONTRACT.md)的定时弹幕脚本 JSON。

下游发送端（jieger / 另一会话）读取该 JSON，用快手小号按 `send_at` 定时真发。本 crate **只产 JSON**，不碰真实账号/代理/风控。语言与 Cloaksession 一致（Rust），不引入 Python。

## 主链路（当前离线可跑）

```
转写(whisper.json / SRT)
  → 识别 read/cta/cue（规则兜底 + LLM 语义判定回 seg 索引）
  → 组装：read send=spoken-lead；cta 展开刷屏；cue 精确透传；补 filler，排序，轮换 account_hint
  → DanmakuScript JSON（符合 schema v1.1）
```

## 用法

```powershell
# 规则版（默认，离线，零依赖）
cargo run -- build --transcript examples/transcript.example.json --out out.json --lead 5 --filler-ratio 0.5 --viewers 8

# LLM 版离线回放（用已保存的 LLM 响应验证解析/映射，无需网络）
cargo run -- build --transcript examples/transcript.example.json --detector llm --llm-response examples/llm-response.canned.txt --out out.json

# LLM 版真实调用（需带网络与密钥；构建时开 feature）
$env:DANMAKU_LLM_BASE_URL="https://api.openai.com/v1"; $env:DANMAKU_LLM_API_KEY="sk-..."; $env:DANMAKU_LLM_MODEL="gpt-4o-mini"
cargo run --features llm-http -- build --transcript real.json --detector auto --out out.json
```

参数见 `cargo run -- --help`。`--transcript` 支持 whisper 系 verbose_json 或 `.srt`。
`--detector` 取 `rule`（默认）| `llm` | `auto`（LLM 为主、规则兜底）。

## 端到端流程

1. `ffmpeg` 抽 16k 单声道 wav（`asr::extract_audio`，需系统装 ffmpeg）。
2. ASR 转写（见 [ASR-RUNBOOK.md](ASR-RUNBOOK.md)）：
   - 现用 **外部 ASR 产物导入**（`asr::imported::ImportedAsr`）——先用 whisper/faster-whisper/whisperX 产出 verbose_json 或 SRT。
   - 本地 `whisper.cpp` 绑定（`asr::whisper_cpp`）**预留**，feature 门控，尚未接入。
3. 弹幕识别：
   - **规则版**（`detect::rule::RuleDetector`）：read 靠"有个宝宝问""弹幕说"等提示语切正文；cta 靠扣1/打1/刷1等喊话（burst=5/delay=2.5）；cue 靠上车/小黄车（onboard_call）与试用/退款/运费险等（pitch），离线零依赖，作兜底。
   - **LLM 版**（`detect::llm::LlmDetector`）：AI 只做语义判断回 seg 索引，输出 read（还原口语弹幕）/ cta（跟进文本+burst/delay）/ cue（短标签+cue_kind+data），置信度与人设全交 AI。HTTP 走 `llm-http` feature；解析/映射为纯函数，可用 `--llm-response` 离线回放验证。
4. 组装（`builder::ScriptBuilder`）+ 补氛围弹幕（`filler`）：三公式 send_read=spoken-lead / send_cta=spoken+delay+i*stagger / send_cue=spoken；cta 按 burst_group 展开多号同发；cue 精确透传不占账号。

## 模块

| 文件 | 职责 |
| --- | --- |
| `model.rs` | 契约输出类型（DanmakuScript/Event/Meta/CueKind），对齐 schema v1.1 |
| `transcript.rs` | 转写中间模型 + whisper.json / SRT 导入 |
| `config.rs` | 提前量、氛围比例、置信度阈值、观众数、词库、cta_delay/burst/stagger |
| `detect/` | 识别器 trait（产出 DetectedItem 三类）+ 规则版 + LLM 版 + 降级组合（FallbackDetector） |
| `asr/` | ASR trait + 导入实现 + whisper.cpp（预留）+ ffmpeg 抽音频 |
| `filler.rs` | 空档期确定性均匀补氛围弹幕，避让全部 read+cta+cue |
| `builder.rs` | 算 send_at（三公式）、cta 展开、cue 透传、过滤置信度、排序校验、重排 id、轮换账号 |
| `main.rs` | CLI（build --detector rule/llm/auto、--cta-delay/--cta-burst/--cta-stagger、transcribe） |

## 校验产物

```powershell
npx -p ajv-cli@5 -p ajv-formats@2 ajv validate -s schema/danmaku-script.schema.json -d out.json --spec=draft2020 -c ajv-formats
```

## 待办（下一步）

- 用真实录像跑 LLM 版，按结果微调三类 prompt（read 还原口吻、cta 跟进文本、cue 打标签载荷）与 chunk 策略。
- 接入 whisper.cpp 本地转写（feature 门控），省去外部 ASR 步骤。
- 规则词库随真实录像迭代（兜底用：cta 喊话变体、cue 上车/卖点关键词）。
- LLM 与规则结果的融合去重（当前 auto 为"LLM 空才回退"，可做逐时段融合）。
- 真录像联调：cue 精确到秒后校验上车点与挂链时差；cta 刷屏组按多号同发压测风控间隔。
