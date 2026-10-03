# implement — auto-reply

## 执行清单

### 1. replyEngine
- [ ] 1.1 `crates/tauri-app/src/driver/auto_reply.rs` 建 GoodsKnowledge/ReplyContext/ReplyResult 结构体
- [ ] 1.2 `normalize(s)` lowercase + 去空格
- [ ] 1.3 `score_knowledge(question, goods)` token 命中+10 / Q&A+30 / 价格意图+8 / 用法意图+5
- [ ] 1.4 `answer_from_knowledge(question, goods)` Q&A 精确优先 → 拼装 title/price/promotion/status/highlights
- [ ] 1.5 `try_knowledge_reply(ctx)` 取 goods 列表逐个 score，最高分>0 返回
- [ ] 1.6 意图正则：价格/优惠/用法/库存 等常量
- [ ] 1.7 单元测试：各分支 + normalize 中文

### 2. 事件消费
- [ ] 2.1 `GoodsKnowledgeSource` trait 定义（list_goods_knowledge）
- [ ] 2.2 订阅 comment-listener 的 mpsc/broadcast channel
- [ ] 2.3 每条 CommentEvent：normalize → try_knowledge_reply → 命中则 sub_account::send_danmaku
- [ ] 2.4 MCP 钩子：`source=Ai` 分支留 trait，不实现实际调用
- [ ] 2.5 channel 背压：mpsc 有界 + drop_oldest 或 broadcast lag

### 3. 回复记录
- [ ] 3.1 `auto_reply_records` 表 + migration（account_id/content/reply/source/goods_id/timestamp）
- [ ] 3.2 `record_reply(account_id, content, reply, source)` 落库

### 4. IPC 命令层
- [ ] 4.1 `crates/tauri-app/src/commands/auto_reply.rs` 新建
- [ ] 4.2 `#[tauri::command] async fn start_auto_reply(profile_id) -> Result<(), String>`
- [ ] 4.3 `#[tauri::command] async fn stop_auto_reply(profile_id) -> Result<(), String>`
- [ ] 4.4 `#[tauri::command] async fn list_reply_records(profile_id, limit) -> Result<Vec<ReplyRecord>, String>`
- [ ] 4.5 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 5. 验证
- [ ] 5.1 `cargo test -p tauri-app --locked`（score/answer 各分支 + mock channel 消费）
- [ ] 5.2 `cargo test -p profile-manager --locked`（auto_reply_records migration）
- [ ] 5.3 `cargo check --workspace --locked`
- [ ] 5.4 真号自动回复验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo test -p profile-manager --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 replyEngine commit
- 2 事件消费 commit
- 3 记录层 commit
- 4 IPC 层 commit
- 5 验证后准备 archive

## 依赖前置
- `comment-listener`（事件源 channel）
- `sub-account`（send_danmaku 接口）
- `shop-product-script`（GoodsKnowledgeSource trait 实现，可后期对接）

## Notes
- MCP 钩子只留 trait，不实现实际 AI 调用（架构记忆：aiChat 不迁，走 MCP——接入属另一任务）。
- 知识库空时跳过不回复。
- normalize 的 to_lowercase 中文场景实现时验证。
