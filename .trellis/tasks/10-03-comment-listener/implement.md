# implement — comment-listener

## 执行清单

### 1. CommentEvent 与契约对齐
- [ ] 1.1 `crates/tauri-app/src/driver/comment_listener.rs` 建 CommentEventType/CommentEvent 结构体
- [ ] 1.2 读 `tools/danmaku-pipeline/schema/danmaku-script.schema.json` + `CONTRACT.md`，对齐字段名/类型
- [ ] 1.3 CommentEvent serde 序列化测试通过（与 schema 兼容）
- [ ] 1.4 写库表决策：profile-manager 新增 `live_events` 表（msg_id PK/type/user_id/nickname/content/time/account_id）+ migration

### 2. MutationObserver 注入
- [ ] 2.1 `KuaishouCommentParser::start(task_page, cancel)` 实现
- [ ] 2.2 `Runtime.addBinding` 注册 `__ksLiveOnEvent__` 桥接回调（CDP 原生）
- [ ] 2.3 注入 MutationObserver JS：监听 `KS_SELECTORS.comments.list` 子节点新增，调 `window.__ksLiveOnEvent__(JSON.stringify(event))`
- [ ] 2.4 `Page.frameNavigated` 事件监听，导航后 re-inject
- [ ] 2.5 `bindingCalled` 事件 → 解析 JSON → dispatch 到 listeners + 写库 + emit

### 3. 降级 polling
- [ ] 3.1 MutationObserver 失败检测（注入后无事件 + 超时）
- [ ] 3.2 降级 polling：周期 `evaluate` 提取评论列表 DOM，diff seen Set 去重
- [ ] 3.3 `fallbackIntervalMs=2000`

### 4. 单例监听与订阅
- [ ] 4.1 `HashMap<String, ListenerState>` per-account 单例
- [ ] 4.2 `start_listener(account_id, task_page, cancel)` / `stop_listener(account_id)`
- [ ] 4.3 内部订阅 channel：`tokio::sync::broadcast` 或 `mpsc`（auto-reply 消费）
- [ ] 4.4 前端 emit `comment-event`

### 5. IPC 命令层
- [ ] 5.1 `crates/tauri-app/src/commands/comment_listener.rs` 新建
- [ ] 5.2 `#[tauri::command] async fn start_comment_listener(profile_id) -> Result<(), String>`
- [ ] 5.3 `#[tauri::command] async fn stop_comment_listener(profile_id) -> Result<(), String>`
- [ ] 5.4 `#[tauri::command] async fn list_recent_comments(profile_id, limit) -> Result<Vec<CommentEvent>, String>`
- [ ] 5.5 emit `comment-event` + `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 6. 验证
- [ ] 6.1 `cargo test -p tauri-app --locked`（CommentEvent serde + seen 去重 + mock bindingCalled）
- [ ] 6.2 `cargo test -p profile-manager --locked`（live_events migration + CRUD）
- [ ] 6.3 `cargo check --workspace --locked`
- [ ] 6.4 真号直播间监听验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo test -p profile-manager --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 契约层 commit
- 2-3 注入+降级 commit
- 4 单例+订阅 commit
- 5 IPC 层 commit
- 6 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（进入直播间页）
- 契约对齐：`tools/danmaku-pipeline/`（独立 crate，不引用代码只对齐 schema）

## Notes
- 发送端不在本仓库，本任务只到「产 CommentEvent JSON」为止。
- `send_at` 为发送权威（架构记忆），CommentEvent 的 `time` 字段对齐 pipeline 期望。
