# ASR 操作手册：真实录像 → 转写 → 导入管线

本管线不在 crate 内跑推理（whisper.cpp 绑定预留中），当前用**外部 ASR 产出转写文件**再导入。
下面给出 Windows 上可直接用的命令。目标：拿到带词级时间戳的 `verbose_json` 或 `.srt`。

## 0. 前置：抽音频（可选，多数 ASR 能直接吃视频）

```powershell
ffmpeg -y -i input.mp4 -ac 1 -ar 16000 -vn audio.wav
```

管线内也有 `asr::extract_audio` 做同样的事（需系统已装 ffmpeg）。

## 1. 推荐：faster-whisper（快、显存友好、词级时间戳）

安装（Python 环境，仅用于产出转写文件，不进 crate）：

```powershell
pip install faster-whisper
```

用 CLI 包装 `faster-whisper-xxl` 或写几行脚本产出 verbose_json。最省事的是官方 `whisper` 或 `whisperX`（见下）。faster-whisper 走脚本时，导出结构需含 `segments[].start/end/text`，可选 `words[].word/start/end`——与本管线 `transcript.rs` 对齐即可。

## 2. 备选：openai-whisper（命令行直接出 json）

```powershell
pip install -U openai-whisper
whisper input.mp4 --model large-v3 --language zh --word_timestamps True --output_format json --output_dir out
```

产出 `out/input.json`，字段含 `segments[].start/end/text` 与（开 word_timestamps 后）`segments[].words[]`。直接喂管线：

```powershell
cargo run -- build --transcript out/input.json --detector auto --out script.json
```

## 3. 备选：whisperX（对齐更准，词级时间戳质量高）

```powershell
pip install whisperx
whisperx input.mp4 --model large-v3 --language zh --output_format json --output_dir out
```

字段兼容（`word` 或 `text` 皆可，管线用 serde alias 兼容）。

## 4. 若只有字幕文件

管线也吃 `.srt`（无词级时间戳，用段级时间）：

```powershell
cargo run -- build --transcript input.srt --detector auto --out script.json
```

## 5. 接 LLM 识别（质量最好）

- 离线先验证解析链路：`--detector llm --llm-response <保存的LLM响应>`。
- 真实调用：`cargo build --features llm-http`，设置环境变量后 `--detector auto`：

```powershell
$env:DANMAKU_LLM_BASE_URL="https://api.openai.com/v1"
$env:DANMAKU_LLM_API_KEY="sk-..."
$env:DANMAKU_LLM_MODEL="gpt-4o-mini"
cargo run --features llm-http -- build --transcript out/input.json --detector auto --out script.json
```

## 6. 质量建议

- 录像含 BGM/带货嘈杂时，ASR 前可做人声分离（UVR / Demucs）提升准确率。
- 中文优先 large-v3；显存紧张用 faster-whisper 的 int8。
- LLM 版对隐式回应还原明显更好；规则版作离线兜底。
- 产出脚本务必人工过一遍高风险条目（低 confidence、买家导流类），再交发送端。
