# design — auto-popup

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/auto_popup.rs`。在 sub-account 小号会话 + 中控页操作商品。

### 队列轮换
```rust
pub struct AutoPopUpConfig {
    pub goods_ids: Option<Vec<String>>,
    pub interval: (u32, u32),  // [min,max] 秒
    pub per_goods_interval: Option<HashMap<String, (u32, u32)>>,
    pub goods_items: Option<Vec<AutoPopUpGoodsItem>>,  // 增强队列
    pub random: bool,
    pub retry: Option<AutoPopUpRetryConfig>,
}
pub struct AutoPopUpGoodsItem { pub id: String, pub repeat_count: Option<u32>, pub interval: Option<(u32,u32)> }
```
队列构建：goods_items 优先，否则 goods_ids。random=true 时随机选队列项，否则顺序。每个商品按 interval（全局或 per-goods）发送。

### explainGoods（goodsList 等价）
- `fetch_goods_list(task_page)`：`evaluateAll` 提取商品列表（serial/title/price）。
- `wait_for_goods_visible(task_page, keyword, timeout=15s)`：周期 150ms 轮询商品可见，scrollIntoView。
- `explain_goods(task_page, keyword)`：waitForGoodsVisible → click explainButton。
- `cancel_explain_goods(task_page)`：click cancelExplainButton。

### goodsKnowledge 对接
`scan_goods_knowledge` 扫描商品知识库（来自 shop-product-script）。定义消费接口：
```rust
pub trait ProductScriptSource {
    async fn scan_goods_knowledge(account_id: &str) -> Vec<GoodsKnowledge>;
}
```

### shortcutManager
jieger 用 Electron globalShortcut。Tauri 等价：`tauri::AppHandle::plugin(tauri_plugin_global_shortcut)`。注册快捷键触发 explainGoods。

### runWithRetry
失败重试：`max_retries` × `retry_delay_ms`。Rust 用 tokio + retry crate 或手写循环。

### 状态广播
`AutoPopUpState { running, current_goods_id, sent_count, error }` → emit。

### 与 sub-account/中控页协作
- explainGoods 在**中控页**操作（主播身份，非小号观众身份）。
- 需先 ensure_auth（ks-platform-primitives，KUAISHOU_CONFIG）进中控页。
- 与 sub-account 的小号会话是**不同 Profile**（主播 vs 观众）。

### 风险与取舍
- **选择器校准**：goods 选择器（item/explainButton/cancelExplainButton）真号校准。
- **快捷键冲突**：Tauri globalShortcut 全局快捷键需用户授权，避免与系统冲突。
- **retry 策略**：explainGoods 失败可能是商品下架，重试有上限不无限。

### 验证
- 单元测试：队列构建逻辑（顺序/随机/per-goods interval）+ runWithRetry。
- 集成测试：mock TaskPage explainGoods 流程。
- 真号弹品验收单独确认。
