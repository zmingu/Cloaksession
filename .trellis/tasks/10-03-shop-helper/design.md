# design — shop-helper

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/shop_helper.rs`。在跟播助手页（zs.kwaixiaodian.com/page/helper）操作商品上车/下车。

### 关键边界（架构记忆硬约束）
**与已实现的 CPS 加货架（cps.kwaixiaodian.com）是两个不同流程，代码与流程严格隔离。**
- CPS 加货架：开播前准备，手动粘贴商品文本按 `ID[:：]\s*(\d+)` 解析。已实现，不在本任务。
- 跟播助手上车：zs.kwaixiaodian.com/page/helper，小黄车商品上车/下车。本任务。

### DOM helpers 内嵌策略
jieger 的关键策略：大量逻辑用 `page.evaluate` 在浏览器内完成（快手 DOM 多变，readonly DOM helpers 字符串内嵌比 Playwright locator 更鲁棒）。Rust 侧复刻：`TaskPage::evaluate(HELPER_GOODS_DOM_HELPERS)` 注入 JS helpers，再 evaluate 调用。

### 商品操作
```rust
pub enum HelperGoodStatus { Available, OnShelf, OffShelf, Unknown }
pub enum HelperGoodTab { InCart, ToAdd }  // 小黄车商品 / 待上车商品
pub struct HelperGoodInfo {
    pub goods_id: String,
    pub goods_name: String,
    pub raw_text: String,
    pub available_actions: Vec<String>,  // 上车/下车
    pub status: HelperGoodStatus,
    pub source_tab: String,
}
```
- `read_helper_goods(task_page, tab)`：evaluate DOM helpers 读取商品列表。
- `add_to_cart(task_page, goods_id)`：上车。
- `remove_from_cart(task_page, goods_id)`：下车。
- `switch_tab(task_page, tab)`：切换小黄车/待上车 Tab。

### readId 多策略提取
jieger 的 `readId` 从 DOM 节点提取商品 ID（文本 ID:xxx / id 属性 goods-xxx / input value / data-id / 嵌套属性），全量复刻为 JS helpers。

### NOISE_TEXTS 过滤
`没有更多商品啦`/`暂无数据`/`加载中` 等噪声文本过滤，`isNoiseText` 判定。

### 脚本化上车
被 shop-product-script 调用（`goods_on_shelf`/`goods_off_shelf`）。定义接口供 shop-product-script 调用。

### 风险与取舍
- **DOM 多变**：快手 DOM 多变是 jieger 用 evaluate 内嵌 helpers 而非 locator 的原因；Rust 侧保留此策略。
- **选择器校准**：shopHelperSelectors 真号校准。
- **上车写动作**：涉及商品上架，真实账号写动作需单独验收。

### 验证
- 单元测试：readId 多策略提取逻辑（JS helpers 单测）、NOISE_TEXTS 过滤、status 推断。
- 离线 wire 测试：mock TaskPage evaluate 返回 DOM 结构，验证读取。
- 真号上车验收单独确认。
