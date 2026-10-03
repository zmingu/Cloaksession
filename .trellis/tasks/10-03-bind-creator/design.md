# design — bind-creator

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/bind_creator.rs`。达人授权流程在金牛后台 `niu.e.kuaishou.com/account/authorize` 页操作。

### 授权流程
jieger 的 `bindCreator.ts`：
1. `syncAuthorizeList(jinniu_id)`：导航到 authorize 页，`.ant-table` 等待，evaluate 提取表格行（userId/userName/status/authorizeTime）。
2. `getAuthorizeList(jinniu_id)`：从 DB 读授权记录。
3. `dismissFeedbackModal`：隐藏 feedbackModal（`[class*="feedbackModal"]`，`display:none`）。
4. 绑定操作：搜索达人（`SEARCH_TIMEOUT_MS=10s`）+ 操作（`ACTION_TIMEOUT_MS=5s`）。

### 授权记录存储
jieger 用 `jinniuAuthorize` 表（getByJinniuId/upsertBatch）。Cloaksession 决策：
- **复用 business_accounts**：新增 kind `jinniu_creator`（达人作为独立 kind），或
- **独立 `jinniu_authorize` 表**（jinniu_id/userId/userName/status/authorizeTime）。
决策：独立表（达人授权是 jinniu 子户下的多对多关系，不适合塞 business_accounts）。profile-manager migration。

### AuthorizeItem
```rust
pub struct AuthorizeItem {
    pub user_id: String,
    pub user_name: String,
    pub status: String,
    pub authorize_time: String,
}
```

### 与 jinniu-promote 协作
共享金牛后台上下文（`__accountId__` + homeType=new）。本任务导航到 `account/authorize` 子页。

### dismissFeedbackModal
feedbackModal 是快手后台常见弹窗，`evaluate(display:none)` 隐藏。Rust 侧 `TaskPage::evaluate` 复刻。

### 风险与取舍
- **ant-table 选择器**：`.ant-table-tbody tr.ant-table-row` 是 Ant Design 标准，较稳定但仍需校准。
- **搜索超时**：10s 搜索达人，找不到返回错误。
- **授权状态语义**：status 字段值从源码提取（已授权/待确认等）。

### 验证
- 单元测试：AuthorizeItem 解析、表格行 evaluate JS 正确性。
- 离线 wire 测试：mock TaskPage evaluate 返回表格 DOM。
- 真号授权验收单独确认。
