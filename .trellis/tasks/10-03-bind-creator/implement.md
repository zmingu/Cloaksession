# implement — bind-creator

## 执行清单

### 1. 授权记录存储
- [ ] 1.1 `crates/tauri-app/src/driver/bind_creator.rs` 建 AuthorizeItem/AuthorizeListResult 结构体
- [ ] 1.2 profile-manager 新增 `jinniu_authorize` 表 + migration（jinniu_id/userId/userName/status/authorizeTime）
- [ ] 1.3 `upsert_batch(jinniu_id, items)` / `get_by_jinniu_id(jinniu_id)` 落库/查询

### 2. 授权列表同步
- [ ] 2.1 `sync_authorize_list(jinniu_id, task_page)` 导航 `niu.e.kuaishou.com/account/authorize`（homeType=new）
- [ ] 2.2 `dismiss_feedback_modal(task_page)` evaluate `[class*="feedbackModal"]` display:none
- [ ] 2.3 evaluate 提取 `.ant-table-tbody tr.ant-table-row` 表格行（userId/userName/status/authorizeTime）
- [ ] 2.4 upsert_batch 落库
- [ ] 2.5 `get_authorize_list(jinniu_id)` 从 DB 读

### 3. 绑定操作
- [ ] 3.1 搜索达人（SEARCH_TIMEOUT_MS=10s）
- [ ] 3.2 操作（ACTION_TIMEOUT_MS=5s）
- [ ] 3.3 从 jieger 源码提取完整绑定流程

### 4. IPC 命令层
- [ ] 4.1 `crates/tauri-app/src/commands/bind_creator.rs` 新建
- [ ] 4.2 `sync_authorize_list`/`get_authorize_list`/`bind_creator` 命令
- [ ] 4.3 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 5. 验证
- [ ] 5.1 `cargo test -p tauri-app --locked`（AuthorizeItem 解析 + mock 表格 evaluate）
- [ ] 5.2 `cargo test -p profile-manager --locked`（jinniu_authorize migration + CRUD）
- [ ] 5.3 `cargo check --workspace --locked`
- [ ] 5.4 真号授权验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo test -p profile-manager --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 存储层 commit
- 2 同步列表 commit
- 3 绑定操作 commit
- 4 IPC 层 commit
- 5 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（KUAISHOU_JINNIU_CONFIG）
- `jinniu-promote`（共享金牛后台上下文 + 通用工具）

## Notes
- 独立 `jinniu_authorize` 表（达人授权是 jinniu 子户多对多关系）。
- dismissFeedbackModal 用 evaluate display:none。
- ant-table 选择器较稳定但仍需校准。
