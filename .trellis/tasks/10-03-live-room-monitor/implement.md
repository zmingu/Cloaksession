# implement — live-room-monitor

## 执行清单

### 1. 状态结构与监控循环
- [ ] 1.1 `crates/tauri-app/src/driver/live_room_monitor.rs` 建 LiveRoomMonitorStatus/LiveRoomLiveStatus/LiveRoomMonitorState 结构体
- [ ] 1.2 `MonitorConfig` 结构体（liveRoomUrl/sceneId/groupId/productScriptId/productScriptAccountId/autoExitSubAccounts）
- [ ] 1.3 监控循环 tokio task：FAST_POLL_MS=5s / SLOW_POLL_MS=60s 自动切换
- [ ] 1.4 `checking` 互斥（tokio::sync::Mutex 防重叠）
- [ ] 1.5 `start_monitor(config, cancel)` / `stop_monitor()` / `get_state()`

### 2. 开播检测
- [ ] 2.1 `check_live_status(live_room_url, task_page)` 导航 + settle 800ms + 提取 LiveRoomStatusSnapshot
- [ ] 2.2 四组关键词常量：INVALID_ROOM_KEYWORDS/OFFLINE_ROOM_KEYWORDS/ACCESS_LIMITED_KEYWORDS/LIVE_ROOM_KEYWORDS + LIVE_VIEWER_RE
- [ ] 2.3 关键词分类逻辑 → LiveRoomLiveStatus
- [ ] 2.4 备选：KuaishouLiveDetailResponse 接口查 author.living（如页面关键词不确定）

### 3. 联动触发
- [ ] 3.1 `MonitorTransition` 逻辑：`should_trigger = live_status==Live && !triggered_for_current_live`
- [ ] 3.2 `LiveRoomTrigger` trait 定义（on_live_start/on_live_end）
- [ ] 3.3 触发时调 `scene_play::start_scene` + `shop_product_script::play_product_script` + sub-account `enter_live_room`（按 groupId 过滤）
- [ ] 3.4 `auto_exit_sub_accounts`：Live→Offline 时批量 exit_live_room/close_session
- [ ] 3.5 触发结果记录到 `last_enter_all_result`/`last_product_script_result`

### 4. IPC 命令层
- [ ] 4.1 `crates/tauri-app/src/commands/live_room_monitor.rs` 新建
- [ ] 4.2 `#[tauri::command] async fn start_live_room_monitor(config) -> Result<(), String>`
- [ ] 4.3 `#[tauri::command] async fn stop_live_room_monitor() -> Result<(), String>`
- [ ] 4.4 `#[tauri::command] async fn get_live_room_monitor_state() -> Result<LiveRoomMonitorState, String>`
- [ ] 4.5 状态变化 `app_handle.emit("live-room-monitor-state-changed", state)`
- [ ] 4.6 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 5. 验证
- [ ] 5.1 `cargo test -p tauri-app --locked`（MonitorTransition + 关键词分类 + mock trait 触发）
- [ ] 5.2 `cargo check --workspace --locked`
- [ ] 5.3 真号直播间监控验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1-2 状态机+检测层可独立 commit
- 3 联动触发层 commit
- 4 IPC 层 commit
- 5 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（导航直播间页）
- 协作（下游触发）：`sub-account`、`scene-play`、`shop-product-script`、`huibo-live`

## Notes
- 关键词表是 jieger 经验值，真号校准时更新。
- 下游触发失败不中断监控循环，记录到 last_*_result。
- 快慢轮询自动切换：triggered 后转快轮询。
