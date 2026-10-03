# design — shop-product-script

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/shop_product_script.rs`。商品话术库 CRUD + 播放引擎，被 auto-popup 和 live-room-monitor 调用。

### 模型
```rust
pub enum ScriptLineAction { OnShelf, OffShelf, Explain, CancelExplain }  // 上车/下车/讲解/停止讲解
pub struct ShopProductScriptLine {
    pub id: i64, pub ord: i32,
    pub action: ScriptLineAction,
    pub goods_id: String, pub goods_name: String,
    pub video_time_sec: i64,  // 视频时间轴秒
    pub lead_sec: i64,  // 提前量
    pub trigger_offset_sec: i64,  // = video_time_sec - lead_sec
}
pub struct ShopProductScript { pub id: i64, pub account_id: String, pub lines: Vec<ShopProductScriptLine> }
```

### 存储决策
独立表 `shop_product_scripts` + `shop_product_script_lines`（profile-manager migration）。理由：话术需 CRUD + 排序 + 关联商品 ID，结构化查询，不适合 JSON。

### 播放引擎
- `build_schedule(script, started_at)`：每条 `trigger_at = started_at + trigger_offset_sec * 1000`。
- 每条独立 tokio::spawn + sleep_until(trigger_at)。
- action 派发：
  - OnShelf → shop_helper::goods_on_shelf
  - OffShelf → shop_helper::goods_off_shelf
  - Explain → goodsList::explain_goods（auto-popup 子任务的接口）
  - CancelExplain → goodsList::cancel_explain_goods
- `PlaySession { timers, cancelled }`，cancel 经 select!。

### ProductScriptSource trait
为 auto-popup 提供 `scan_goods_knowledge`：
```rust
pub trait ProductScriptSource {
    async fn scan_goods_knowledge(account_id: &str) -> Vec<GoodsKnowledge>;
}
```
本任务实现此 trait（话术 → GoodsKnowledge 转换）。

### startAt 共享
与其他视频时间轴脚本共用起始时间戳（`start_at` 参数），不传则当前时间。

### 状态广播
`ProductScriptPlayState { script_id, account_id, started_at, total_count, done_count, schedule }` → emit。

### 风险与取舍
- **action 派发跨 task**：OnShelf/OffShelf 调 shop-helper，Explain/CancelExplain 调 auto-popup 的 goodsList 接口——需定义清晰接口。
- **trigger_offset_sec 计算**：`video_time_sec - lead_sec`，提前量逻辑保留。
- **不搬旧话术数据**：只迁功能。

### 验证
- 单元测试：build_schedule + trigger_offset 计算 + action 派发 mock。
- 集成测试：mock shop-helper/goodsList 接口，验证播放顺序。
