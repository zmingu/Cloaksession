# danmaku-pipeline（上游管线）

直播录像 → 分析主播「念/回应弹幕」→ 生成符合[契约](../CONTRACT.md)的定时弹幕脚本 JSON。

下游发送端（jieger / 另一会话）读取该 JSON，用快手小号按 `send_at` 定时真发。本 crate **只产 JSON**，不碰真实账号/代理/风控。语言与 Cloaksession 一致（Rust），不引入 Python。

## 主链路（当前离线可跑）

```
转写(whisper.json / SRT)
  → 规则识别 read（主播念/回应弹幕的时刻 + 还原原文）
  → 组装：send_at = spoken_at - lead_time，补 filler，排序，轮换 account_hint
  → DanmakuScript JSON（符合 schema）
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
   - **规则版**（`detect::rule::RuleDetector`）：靠"有个宝宝问""弹幕说"等提示语切正文，离线零依赖，作兜底。
   - **LLM 版**（`detect::llm::LlmDetector`）：语义识别隐式回应（"对，这个是纯棉的" → 反推"是纯棉的吗"）、还原口语弹幕、给置信度与人设。HTTP 走 `llm-http` feature；解析/映射为纯函数，可用 `--llm-response` 离线回放验证。
4. 组装（`builder::ScriptBuilder`）+ 补氛围弹幕（`filler`）。

## 模块

| 文件 | 职责 |
| --- | --- |
| `model.rs` | 契约输出类型（DanmakuScript/Event/Meta），对齐 schema |
| `transcript.rs` | 转写中间模型 + whisper.json / SRT 导入 |
| `config.rs` | 提前量、氛围比例、置信度阈值、观众数、词库 |
| `detect/` | 识别器 trait + 规则版 + LLM 版 + 降级组合（FallbackDetector） |
| `asr/` | ASR trait + 导入实现 + whisper.cpp（预留）+ ffmpeg 抽音频 |
| `filler.rs` | 空档期确定性均匀补氛围弹幕，避让 read |
| `builder.rs` | 算 send_at、过滤置信度、排序、重排 id、轮换账号 |
| `main.rs` | CLI（build --detector rule/llm/auto、transcribe） |

## 校验产物

```powershell
npx -p ajv-cli@5 -p ajv-formats@2 ajv validate -s schema/danmaku-script.schema.json -d out.json --spec=draft2020 -c ajv-formats
```

## 待办（下一步）

- 用真实录像跑 LLM 版，按结果微调 prompt 与 chunk 策略。
- 接入 whisper.cpp 本地转写（feature 门控），省去外部 ASR 步骤。
- 规则词库随真实录像迭代（兜底用）。
- LLM 与规则结果的融合去重（当前 auto 为"LLM 空才回退"，可做逐时段融合）。
