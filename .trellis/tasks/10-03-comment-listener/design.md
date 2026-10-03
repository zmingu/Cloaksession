# design — comment-listener

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/comment_listener.rs`。监听端在应用层，用 cdp-driver TaskPage 注入 MutationObserver + 桥接回调。

### CommentEvent 契约
```rust
pub enum CommentEventType { Comment, RoomEnter, RoomLike, RoomFollow, LiveOrder, LivePaid }
pub struct CommentEvent {
    pub r#type: CommentEventType,
    pub msg_id: String,
    pub user_id: Option<String>,
    pub nickname: String,
    pub content: Option<String>,
    pub time: i64,
}
```
**与 danmaku-pipeline 契约对齐**：`tools/danmaku-pipeline/schema/danmaku-script.schema.json` 的输入结构。监听端产的 CommentEvent 须能序列化为 pipeline 输入；`send_at` 为发送权威（架构记忆）。

### MutationObserver 注入（chromiumoxide 等价）
jieger 用 `page.exposeFunction` + `page.addInitScript` + MutationObserver。chromiumoxide 等价：
- `TaskPage::evaluate(JS)` 注入 MutationObserver 脚本，observer 监听评论列表容器子节点新增。
- 桥接回调：chromiumoxide 无 `exposeFunction` 直接等价——用 `Runtime.bindingCalled` 事件（CDP `Runtime.addBinding`）注册 Node 端回调，页面 JS 调 `window.__ksLiveOnEvent__(JSON.stringify(event))` 触发。
- 导航后重新注入：监听 `Page.frameNavigated` 事件，自动 re-inject。

### 降级 polling
MutationObserver 失败时降级：周期 `evaluate` 提取评论列表 DOM（`fallbackIntervalMs=2000`），diff `seen` Set 去重。

### 单例监听
per-account：`HashMap<String, ListenerState>`。ListenerState 持有 TaskPage lease + seen Set + unsubscribe。

### 写库
CommentEvent 落库。复用 `profile-manager` 新增 `live_events` 表（或 settings-store JSON，决策在 implement.md 1.4）。表结构：msg_id PK + type + user_id + nickname + content + time + account_id。

### IPC 广播
- 前端订阅：`app_handle.emit("comment-event", event)`。
- 内部订阅者（auto-reply）：`Arc<Mutex<Vec<fn(&CommentEvent)>>>` 或 channel（implement.md 决策）。

### 与 danmaku-pipeline 边界
- 本任务（监听端）：产 CommentEvent JSON。
- pipeline（识别端）：消费 CommentEvent，产 danmaku-script JSON（含 send_at）。
- 发送端：不在本仓库（见 `snow-danmaku-reference` 记忆）。
- 本任务不引用 pipeline 代码，只对齐 schema 契约。

### 风险与取舍
- **exposeFunction 等价**：chromiumoxide 的 `Runtime.addBinding` 是 CDP 原生，需验证在 TaskPage lease 下可用。若 addBinding 不可用，降级为纯 polling。
- **CommentEvent 序列化与 pipeline schema 对齐**：实现时读 `tools/danmaku-pipeline/schema/danmaku-script.schema.json` 逐字段对齐。
- **写库表决策**：live_events 表 vs settings-store JSON——live_events 量大且需查询，倾向独立表。

### 验证
- 单元测试：CommentEvent 序列化与 schema 对齐、seen 去重。
- 离线 wire 测试：mock `Runtime.bindingCalled` 事件流，验证 dispatch。
- 真号直播间监听验收单独确认。
