# design — auto-reply

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/auto_reply.rs`。消费 comment-listener 事件，匹配命中后经 sub-account send_danmaku 发送。

### replyEngine（商品知识命中）
jieger 的 `replyEngine.ts` 全量迁移：
- `scoreKnowledge(question, goods)`：token 命中 +10，Q&A 命中 +30，价格/优惠意图 +8/+5。
- `answerFromKnowledge`：Q&A 精确命中优先；否则拼装 title/price/promotion/status/highlights。
- `tryKnowledgeReply(ctx)`：取 goodsKnowledge 列表，逐个 score，取最高分（>0）返回。

### 知识库对接
`GoodsKnowledge`（title/alias/highlights/price/promotion/status/qna[]）来自 shop-product-script 子任务。本任务定义消费接口：
```rust
pub trait GoodsKnowledgeSource {
    async fn list_goods_knowledge(account_id: &str) -> Vec<GoodsKnowledge>;
}
```
shop-product-script 实现此 trait。

### 事件消费
订阅 comment-listener 的内部 channel（`tokio::sync::broadcast` 或 mpsc）。每条 CommentEvent：
1. `normalize(content)`：lowercase + 去空格。
2. `tryKnowledgeReply(ctx)`：商品知识命中 → `source=knowledge`。
3. 未命中 → 预留 MCP 钩子（`source=ai`，本任务不实现 AI，只留接口）。
4. 命中 → `sub_account::send_danmaku(account_id, reply)`。

### ReplyResult
```rust
pub struct ReplyResult {
    pub ok: bool,
    pub reply: Option<String>,
    pub source: ReplySource,  // Knowledge | Ai | Template
    pub goods_id: Option<String>,
    pub intent: Option<String>,
    pub error: Option<String>,
    pub knowledge_hit: bool,
}
```

### 回复记录
`auto_reply_records` 表（profile-manager migration）：account_id/content/reply/source/goods_id/timestamp。

### 关键词匹配
- `normalize`：lowercase + `\s+` 去空格（与 jieger 一致）。
- 意图正则：`/价格|多少钱|优惠|活动|券|到手/`、`/怎么用|用法|适合|人群|保质期|库存/` 等。

### MCP 钩子（不实现）
`source=Ai` 分支预留 `Option<McpClient>`，本任务只留 trait 钩子，不接 MCP 实际调用（架构记忆：aiChat 不迁，走 MCP——但 MCP 接入属另一任务）。

### 风险与取舍
- **知识库空时**：`list_goods_knowledge` 返回空 → tryKnowledgeReply 返回 null → 跳过（不回复）。
- **广播 channel 背压**：comment-listener 事件高频，auto-reply 处理慢时用 mpsc 有界 channel + drop_oldest 或 broadcast lag。
- **normalize 一致性**：Rust 的 `to_lowercase()` 与 JS 略有差异（Unicode），实现时验证中文场景。

### 验证
- 单元测试：scoreKnowledge/answerFromKnowledge 各分支、normalize、意图正则。
- 集成测试：mock comment-listener channel + mock GoodsKnowledgeSource + mock send_danmaku。
