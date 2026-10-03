# implement — jinniu-promote

## 执行清单

### 1. 通用金牛操作（jinniuActions 等价）
- [ ] 1.1 `crates/tauri-app/src/driver/jinniu_promote.rs` 建 KS_JINNIU_URLS/KS_JINNIU_SELECTORS 常量（homeType=new 强制）
- [ ] 1.2 `extract_account_id_from_url(url)` → `__accountId__` 参数
- [ ] 1.3 `find_store_create_page(pages)` 查找 storeCreate 页
- [ ] 1.4 `find_account_source_page(pages)` 优先级：首页/创编/管理 > fallback
- [ ] 1.5 `open_new_tab_with_url(context, url, url_keyword)` 新 tab goto + URL 校验
- [ ] 1.6 `click_button_by_text(task_page, button_text)` 文本驱动 click（精确匹配优先）
- [ ] 1.7 `wait_for_text` / `find_visible_modal_by_text` / `click_text_in_scope`
- [ ] 1.8 单元测试：extract_account_id + find_* 优先级 + click_button_by_text 文本匹配

### 2. 推广操作（jinniuPromote 325 行）
- [ ] 2.1 从 jieger `tasks/jinniuPromote/index.ts` 逐个提取 F1（推广用户切换）/ F5-F9（建计划/管理/自动 ROI）操作
- [ ] 2.2 每个 F 操作实现为独立函数，用 1.x 通用工具
- [ ] 2.3 `__accountId__` 子户上下文拼接（build_startup_url）
- [ ] 2.4 homeType=new 强制断言（单元测试）

### 3. IPC 命令层
- [ ] 3.1 `crates/tauri-app/src/commands/jinniu_promote.rs` 新建
- [ ] 3.2 各 F 操作的 Tauri 命令（具体命令名实现时定）
- [ ] 3.3 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 4. 验证
- [ ] 4.1 `cargo test -p tauri-app --locked`（通用工具 + homeType 断言 + mock 文本驱动 click）
- [ ] 4.2 `cargo check --workspace --locked`
- [ ] 4.3 真号金牛推广验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 通用工具 commit
- 2 推广操作 commit
- 3 IPC 层 commit
- 4 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（KUAISHOU_JINNIU_CONFIG + build_startup_url）
- 复用：business_accounts scope=jinniu（账户隔离已实现）

## Notes
- **homeType=new 强制**不可破坏（快手命名反的：new=旧版）。
- 金牛 Profile 与小店 Profile 隔离（scope=jinniu 保证）。
- F1-F9 具体操作实现时从 jieger 源码逐个提取，不臆测。
