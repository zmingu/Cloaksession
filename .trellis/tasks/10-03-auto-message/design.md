# design — auto-message

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/auto_message.rs`。在 sub-account 的小号会话上运行，调 sub-account 的 `send_danmaku` 接口。

### 时间轴调度
jieger 每条独立 setTimeout，绝对时间戳计算避免漂移。Rust 用 tokio 周期任务：
- `MessageLine { offset_sec, message, account_id? }`。
- 启动时刻为 0 点，`trigger_at = started_at + offset_sec * 1000`。
- 每条独立 `tokio::spawn` + `sleep_until(Instant::from_millis(trigger_at))`，到点调 `sub_account::send_danmaku`。

### 变量插值（interpolate）
jieger 的 `variables.ts` 全量迁移到 Rust：
- `{用户名}` / `{nickname}` → ctx.nickname
- `{主播名称}` / `{anchor}` → ctx.anchor
- `{当前时间}` → HH:mm
- `{当前日期}` → YYYY-MM-DD
- `{随机数字}` → 1-100 随机
- `{随机|A|B|C}` → 候选语法，随机选一
- `insertRandomSpaces` → 字符间随机插空格（probability=0.15，防检测重复）

Rust 正则用 `regex` crate，候选语法 `{随机\|([^}]+)}` 正则等价。

### 状态广播
`AutoMessageState { started_at, total_count, sent_count, schedule: Vec<ScheduledItem> }` → emit。

### 取消
`TaskCancel` 复用；drop cancel 即取消所有 spawn 的调度任务（select! cancel.cancelled() 优先）。

### 与 sub-account 接口契约
```rust
pub async fn send_danmaku(account_id: &str, message: &str) -> Result<(), SubError>;
```
auto-message 调用此接口，不直接操作 TaskPage。

### 风险与取舍
- **漂移**：用绝对时间戳 `sleep_until` 而非相对 `sleep` 累加，与 jieger 一致。
- **变量插值正则**：Rust regex 与 JS regex 语法略有差异（`{随机|A|B|C}` 的 `\|` 转义），实现时验证。
- **insertRandomSpaces**：char 迭代用 `.chars()`，注意 Unicode（emoji 等）。

### 验证
- 单元测试：interpolate 各变量替换 + 候选语法 + insertRandomSpaces。
- 集成测试：mock send_danmaku，验证时间轴触发顺序。
