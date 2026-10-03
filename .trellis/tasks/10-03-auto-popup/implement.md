# implement — auto-popup

## 执行清单

### 1. 队列轮换
- [ ] 1.1 `crates/tauri-app/src/driver/auto_popup.rs` 建 AutoPopUpConfig/AutoPopUpGoodsItem 结构体
- [ ] 1.2 `build_queue(config)` 队列构建：goods_items 优先，否则 goods_ids；random 选/顺序
- [ ] 1.3 每个商品按 interval（全局/per-goods）调度
- [ ] 1.4 单元测试：队列顺序/随机/per-goods interval

### 2. explainGoods（goodsList 等价）
- [ ] 2.1 `fetch_goods_list(task_page)` evaluateAll 提取 serial/title/price
- [ ] 2.2 `wait_for_goods_visible(task_page, keyword, timeout=15s)` 周期 150ms 轮询 + scrollIntoView
- [ ] 2.3 `explain_goods(task_page, keyword)` waitForVisible → click explainButton
- [ ] 2.4 `cancel_explain_goods(task_page)` click cancelExplainButton

### 3. goodsKnowledge 与快捷键
- [ ] 3.1 `ProductScriptSource` trait 定义（scan_goods_knowledge）
- [ ] 3.2 `scan_goods_knowledge(account_id)` 调用 trait
- [ ] 3.3 `tauri_plugin_global_shortcut` 注册快捷键触发 explain_goods
- [ ] 3.4 `register_shortcuts`/`unregister_shortcuts`

### 4. runWithRetry 与调度
- [ ] 4.1 `run_with_retry(fn, max_retries, retry_delay_ms)` tokio 循环
- [ ] 4.2 `start_auto_popup(config, cancel)` 调度循环
- [ ] 4.3 AutoPopUpState 广播

### 5. IPC 命令层
- [ ] 5.1 `crates/tauri-app/src/commands/auto_popup.rs` 新建
- [ ] 5.2 `start_auto_popup`/`stop_auto_popup`/`get_auto_popup_state`
- [ ] 5.3 `register_shortcut`/`unregister_shortcut`
- [ ] 5.4 `fetch_goods_list`（供前端展示商品列表）
- [ ] 5.5 emit 状态 + `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 6. 验证
- [ ] 6.1 `cargo test -p tauri-app --locked`（队列 + retry + mock explainGoods）
- [ ] 6.2 `cargo check --workspace --locked`
- [ ] 6.3 真号弹品验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 队列 commit
- 2 explainGoods commit
- 3 知识库+快捷键 commit
- 4 retry+调度 commit
- 5 IPC 层 commit
- 6 验证后准备 archive

## 依赖前置
- `sub-account`（小号会话，但 explainGoods 在中控页非小号）
- `shop-product-script`（ProductScriptSource trait）
- `ks-platform-primitives`（ensure_auth 进中控页）

## Notes
- explainGoods 在中控页（主播身份），与小号观众身份是不同 Profile。
- 快捷键用 tauri_plugin_global_shortcut，需用户授权。
