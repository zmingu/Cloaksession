# implement — shop-product-script

## 执行清单

### 1. 模型与存储
- [ ] 1.1 `crates/tauri-app/src/driver/shop_product_script.rs` 建 ScriptLineAction/ShopProductScriptLine/ShopProductScript 结构体
- [ ] 1.2 profile-manager 新增 `shop_product_scripts` + `shop_product_script_lines` 表 + migration
- [ ] 1.3 Script CRUD：`create_script`/`get_script`/`list_scripts`/`update_script`/`delete_script`
- [ ] 1.4 Line CRUD：`add_line`/`update_line`/`delete_line`/`reorder_lines`
- [ ] 1.5 `trigger_offset_sec = video_time_sec - lead_sec` 计算函数

### 2. 播放引擎
- [ ] 2.1 `build_schedule(script, started_at)` 每条 trigger_at = started_at + trigger_offset_sec*1000
- [ ] 2.2 每条独立 tokio::spawn + sleep_until(trigger_at)
- [ ] 2.3 action 派发：OnShelf→shop_helper::goods_on_shelf / OffShelf→goods_off_shelf / Explain→goodsList::explain_goods / CancelExplain→cancel_explain_goods
- [ ] 2.4 PlaySession 管理 + cancel select!
- [ ] 2.5 `start_at` 共享参数（与其他时间轴脚本共用）

### 3. ProductScriptSource trait
- [ ] 3.1 `ProductScriptSource` trait 定义（scan_goods_knowledge）
- [ ] 3.2 实现此 trait：话术 → GoodsKnowledge 转换（title/alias/highlights/qna）

### 4. 状态广播
- [ ] 4.1 `ProductScriptPlayState` 结构体
- [ ] 4.2 emit `product-script-state-changed`

### 5. IPC 命令层
- [ ] 5.1 `crates/tauri-app/src/commands/shop_product_script.rs` 新建
- [ ] 5.2 `play_product_script`/`stop_product_script`/`get_state`
- [ ] 5.3 Script/Line CRUD 命令
- [ ] 5.4 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 6. 验证
- [ ] 6.1 `cargo test -p tauri-app --locked`（build_schedule + trigger_offset + mock 派发）
- [ ] 6.2 `cargo test -p profile-manager --locked`（migration + CRUD）
- [ ] 6.3 `cargo check --workspace --locked`
- [ ] 6.4 真号话术播放验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo test -p profile-manager --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 模型+存储 commit
- 2 播放引擎 commit
- 3 trait commit
- 4 广播 commit
- 5 IPC 层 commit
- 6 验证后准备 archive

## 依赖前置
- 无（独立，可早做）
- 被调用方：`auto-popup`（ProductScriptSource）、`live-room-monitor`（play_product_script 触发）
- 协作：`shop-helper`（goods_on_shelf/goods_off_shelf 接口）

## Notes
- 不搬旧话术数据。
- trigger_offset_sec = video_time_sec - lead_sec 提前量逻辑保留。
- 独立可早做，无前置依赖。
