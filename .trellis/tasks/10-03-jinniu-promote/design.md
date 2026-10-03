# design — jinniu-promote

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/jinniu_promote.rs`。在金牛后台（niu.e.kuaishou.com/home?homeType=new）执行推广操作。

### 关键边界（架构记忆硬约束）
- 金牛独立 Profile，与快手小店隔离（business_accounts scope=jinniu 已实现）。
- **homeType=new 强制**：快手命名反的（new=旧版，super=新版），所有 URL 强制 `homeType=new`，避免新版 popover 结构变化导致余额/账号信息提取失败。

### 通用金牛操作（jinniuActions 等价）
jieger 的 `jinniuActions.ts` 是通用 Playwright 操作工具，全量迁移：
- `extract_account_id_from_url(url)` → `__accountId__` 参数。
- `find_store_create_page(pages)`：查找「全站推广创编」页（URL 含 storeCreateKeyword）。
- `find_account_source_page(pages)`：查找含 `__accountId__` 的账号源页面（首页/创编/管理优先）。
- `open_new_tab_with_url(context, url, url_keyword)`：新 tab goto。
- `click_button_by_text(page, button_text)`：按按钮文本点击（精确匹配优先）。
- `wait_for_text(page, text)` / `find_visible_modal_by_text` / `click_text_in_scope` 等通用文本驱动工具。

### 推广操作（jinniuPromote 325 行）
- F1：推广用户切换。
- F5-F9：建计划/管理/自动 ROI。
具体操作语义从 jieger 源码逐个提取（实现时对照 `tasks/jinniuPromote/index.ts`）。

### 账户上下文
`__accountId__` 子户 ID 拼接到 URL（`build_startup_url`，ks-platform-primitives 已提供）。

### 文本驱动策略
jieger 用 `clickButtonByText` 等文本驱动（非固定选择器），因金牛后台 DOM 多变。Rust 侧保留此策略：`TaskPage::evaluate` + `document.querySelector` 按文本匹配。

### 风险与取舍
- **F1-F9 语义**：jieger 的 F 编号是内部功能编号，实现时从源码提取具体操作（建计划/管理/ROI 等）。
- **homeType=new 不可破坏**：写进常量与测试断言。
- **Profile 隔离**：金牛 Profile 与小店 Profile 不共享登录态（business_accounts scope 保证）。

### 验证
- 单元测试：extract_account_id_from_url、find_store_create_page/find_account_source_page 优先级、click_button_by_text 文本匹配。
- 离线 wire 测试：mock TaskPage evaluate 返回 DOM，验证文本驱动 click。
- 真号金牛推广验收单独确认（涉及广告账户写动作）。
