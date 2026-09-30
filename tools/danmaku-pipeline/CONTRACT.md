# 弹幕脚本契约（Danmaku Script Contract）v1.0

本地直播录像 → 生成定时弹幕脚本 → 快手小号按时间轴真发。本文件定义**上游管线**与**下游发送端**之间唯一的数据接口，两边只依赖这份 JSON，互不耦合实现。

- Schema：[`schema/danmaku-script.schema.json`](schema/danmaku-script.schema.json)（JSON Schema draft 2020-12）
- 示例：[`examples/danmaku-script.example.json`](examples/danmaku-script.example.json)

## 背景问题

直播录像被循环重播/搬运时，主播在录像里念的是**当时的真实弹幕**，但重播时那些弹幕已不存在，观众看到主播「对着空气念弹幕」会很假。解决办法：用观众小号在**主播念之前几秒**把对应弹幕真发出去，把互动重新对上，让重播直播间看起来是实时的。

## 数据流与职责边界

```
本地视频
  → ① 抽音频 + ASR(词级时间戳)          ┐
  → ② LLM 判定"念/回应弹幕" + 还原原文    ├─ 上游管线（本仓库 tools/danmaku-pipeline，Rust）
  → ③ 生成弹幕时间线(read + filler)      ┘   产出：danmaku-script.json
        │
        │  ← 本契约在这里对齐 →
        ▼
  → ④ 快手小号按 send_at 定时真发         ── 下游发送端（jieger / 另一会话，不在本仓库）
```

- **上游**只负责产出符合本 Schema 的 JSON，不关心账号真实身份、代理、指纹、发送风控。
- **下游**只消费 JSON，负责账号映射、拟人化发送、限速抖动、风控。
- 语言架构与 Cloaksession 保持一致（Rust），不引入 Python。

## 时间模型（关键）

- 时间轴以**视频起点为 0 秒**，所有时间都是相对秒数（float，保留毫秒）。
- 重播开始瞬间，发送端启动计时器（t=0），此后按每条 `send_at` 触发发送。
- `read` 类弹幕的核心公式：

  ```
  send_at ≈ spoken_at - lead_time_sec
  ```

  即**弹幕先发、主播后念**，模拟真实直播里主播看到弹幕才念的因果顺序。`lead_time_sec` 默认取 `meta.default_lead_time_sec`（建议 3~8 秒），单条可覆盖。
- 上游负责算好 `send_at`；发送端以 `send_at` 为**权威触发时间**，只在其上叠加小随机抖动（防风控），不改变整体顺序。

## 事件类型

| type | 含义 | 必填时间字段 | 典型来源 |
| --- | --- | --- | --- |
| `read` | 主播真念过/回应过的弹幕，已还原原文 | `send_at`、`spoken_at` | ② LLM 从转写中识别并还原 |
| `filler` | 管线补充的氛围弹幕，让直播间热闹 | `send_at` | ③ 在 read 空档期按比例生成 |

`read` 又分两种还原：

- **直接复述**："有个宝宝问这个多少钱" → `text` = "这个多少钱"（近似逐字，`confidence` 高）。
- **隐式回应**："对，这个是纯棉的" → 反推 `text` = "是纯棉的吗"（`confidence` 中低，`note` 标注建议人工确认）。

## 字段速查

顶层：`version`、`meta`、`events`。

`meta`（生成参数与溯源）：`platform`(必填)、`video_duration_sec`(必填)、`source_video`、`timezone`、`generated_at`、`generator`、`asr_model`、`llm_model`、`default_lead_time_sec`、`filler_ratio`。

`event`：

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `id` | 是 | 稳定唯一 ID，去重/断点续发/回执关联用 |
| `type` | 是 | `read` / `filler` |
| `send_at` | 是 | 相对视频起点秒数，发送端权威触发时间 |
| `text` | 是 | 最终弹幕文本（1~100 字） |
| `spoken_at` | read 必填 | 主播念出该弹幕的时刻（秒） |
| `lead_time_sec` | 否 | 本条实际提前量，缺省用 meta 默认值 |
| `confidence` | 否 | read 的可信度 0~1，发送端可按阈值过滤 |
| `source_text` | 否 | 该段 ASR 原始转写，人工校对用 |
| `account_hint` | 否 | `auto` 或稳定虚拟观众 ID（如 viewer-1），不含真实账号 |
| `persona` | 否 | 人设提示 buyer/fan/newcomer/neutral/other |
| `note` | 否 | 人工校对备注，发送端可忽略 |

## 发送端消费约定

1. 以 `send_at` 升序执行；每条只发一次，用 `id` 做幂等去重。
2. `account_hint = auto` 时自由轮换账号；相同的非 auto 值（如 `viewer-1`）应尽量映射到**同一个**小号，保持虚拟观众对话连贯。
3. `persona` 仅为挑号建议，可忽略。
4. 可按 `confidence` 阈值过滤低置信 read 条目（默认建议 ≥ 0.6）。
5. **风控由发送端负责**：一号一代理一指纹、发送随机抖动（±1~2s）、全局限速、内容去重与轻改写、账号轮换。上游不保证内容间隔安全。
6. `text` 已是成品，发送端不应二次改写语义（可做安全过滤/表情附加）。

## 兼容与演进

- `meta` 允许附加未知字段（`additionalProperties: true`），便于上游加溯源信息而不破坏下游解析。
- `event` 为封闭结构（`additionalProperties: false`），新增字段需同步升级本契约。
- 破坏性变更递增 `version` 主版本；发送端应校验 `version` 后再解析。

## 校验

任何 JSON Schema 校验器均可验证产物，例如：

```powershell
npx ajv-cli validate -s tools/danmaku-pipeline/schema/danmaku-script.schema.json -d tools/danmaku-pipeline/examples/danmaku-script.example.json --spec=draft2020
```
