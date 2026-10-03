# implement — auto-message

## 执行清单

### 1. 变量插值
- [ ] 1.1 `crates/tauri-app/src/driver/auto_message.rs` 建 `interpolate(template, ctx)` 函数
- [ ] 1.2 正则：`{用户名}|{nickname}` / `{主播名称}|{anchor}` / `{当前时间}` / `{当前日期}` / `{随机数字}` / `{随机|...}`
- [ ] 1.3 `format_time` (HH:mm) / `format_date` (YYYY-MM-DD) 用 chrono
- [ ] 1.4 `insert_random_spaces(message, probability=0.15)` 用 `.chars()` 迭代 + rand
- [ ] 1.5 单元测试：各变量 + 候选语法 + Unicode + insertRandomSpaces

### 2. 时间轴调度
- [ ] 2.1 `MessageLine { offset_sec, message, account_id }` 结构体
- [ ] 2.2 `start_auto_message(profile_id, lines, cancel)`：started_at = now，每条 spawn sleep_until(trigger_at)
- [ ] 2.3 到点：interpolate(line.message, ctx) → sub_account::send_danmaku(account_id, message)
- [ ] 2.4 cancel.select! 优先中断 sleep_until
- [ ] 2.5 AutoMessageState 广播（started_at/total/sent/schedule）

### 3. IPC 命令层
- [ ] 3.1 `crates/tauri-app/src/commands/auto_message.rs` 新建
- [ ] 3.2 `#[tauri::command] async fn start_auto_message(profile_id, lines) -> Result<(), String>`
- [ ] 3.3 `#[tauri::command] async fn stop_auto_message(profile_id) -> Result<(), String>`
- [ ] 3.4 `#[tauri::command] async fn get_auto_message_state(profile_id) -> Result<AutoMessageState, String>`
- [ ] 3.5 emit 状态 + `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 4. 验证
- [ ] 4.1 `cargo test -p tauri-app --locked`（interpolate + 时间轴 mock 触发）
- [ ] 4.2 `cargo check --workspace --locked`
- [ ] 4.3 真号时间轴弹幕验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 变量插值 commit
- 2 调度 commit
- 3 IPC 层 commit
- 4 验证后准备 archive

## 依赖前置
- `sub-account`（send_danmaku 接口）

## Notes
- 用 sleep_until 绝对时间戳防漂移。
- insertRandomSpaces 用 .chars() 防截断 Unicode。
