# implement — huibo-live

## 执行清单

### 1. 类型与状态
- [ ] 1.1 `crates/tauri-app/src/driver/huibo_live.rs` 建 HuiboVideo/ShopLiveState 结构体（字段从 jieger entities.ts + liveControl 提取）
- [ ] 1.2 确认 ShopLiveState 是否已由 live-launch 提供；若否则本任务实现 `get_shop_live_state(account_id, task_page)`

### 2. 视频列表与开播流程
- [ ] 2.1 `fetch_video_list(task_page)` evaluate 提取自传视频列表（replayId/title/duration/thumbnail）
- [ ] 2.2 `start_huibo_live_flow(task_page, replay_id)` 选视频 + 开播流程（evaluate + click）
- [ ] 2.3 `get_huibo_video_list(account_id)`：ensure_auth → fetchVideoList
- [ ] 2.4 `start_huibo_live(account_id, replay_id)`：ensure_auth → startHuiboLiveFlow → getShopLiveState

### 3. IPC 命令层
- [ ] 3.1 `crates/tauri-app/src/commands/huibo_live.rs` 新建
- [ ] 3.2 `get_huibo_video_list`/`start_huibo_live`/`get_shop_live_state` 命令
- [ ] 3.3 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 4. 验证
- [ ] 4.1 `cargo test -p tauri-app --locked`（HuiboVideo/ShopLiveState 解析 + mock 视频列表 evaluate）
- [ ] 4.2 `cargo check --workspace --locked`
- [ ] 4.3 真号回播验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 类型+状态 commit
- 2 视频列表+开播 commit
- 3 IPC 层 commit
- 4 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（ensure_auth 进中控页）
- `live-room-monitor`（监控联动回播）

## Notes
- jieger 最小模块（51 行），依赖 liveControl 的 getShopLiveState。
- ShopLiveState 若 live-launch 已提供则复用，否则本任务实现。
