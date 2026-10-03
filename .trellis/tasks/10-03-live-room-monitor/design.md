# design — live-room-monitor

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/live_room_monitor.rs`。监控是应用层调度中枢，调用下游 task（scene-play/shop-product-script/sub-account）的触发接口。

### 监控状态机
```rust
pub enum LiveRoomMonitorStatus { Idle, Checking, Offline, Live, Triggering, Triggered, Error }
pub enum LiveRoomLiveStatus { Unknown, Offline, Live }
pub struct LiveRoomMonitorState {
    enabled: bool,
    live_room_url: Option<String>,
    scene_id: Option<i64>,
    group_id: Option<String>,
    product_script_id: Option<i64>,
    product_script_account_id: Option<String>,
    auto_exit_sub_accounts: bool,
    status: LiveRoomMonitorStatus,
    live_status: LiveRoomLiveStatus,
    triggered_for_current_live: bool,
    entering_rooms: bool,
    exiting_rooms: bool,
    last_checked_at: Option<i64>,
    next_check_at: Option<i64>,
    last_triggered_at: Option<i64>,
    last_enter_all_result: Option<EnterAllResult>,
    last_exit_all_result: Option<ExitAllResult>,
    last_product_script_result: Option<ProductScriptResult>,
    error: Option<String>,
}
```

### 监控循环
- tokio 周期任务：`FAST_POLL_MS=5s`（直播临近/刚触发后快轮询）、`SLOW_POLL_MS=60s`（离线慢轮询）。
- `CHECK_NAVIGATION_TIMEOUT_MS=20s`、`CHECK_SETTLE_MS=800ms`。
- `checking` 互斥标志防止重叠（同 jieger）。

### 开播检测
- 导航到 liveRoomUrl，settle 800ms，提取 `LiveRoomStatusSnapshot { title, href, text, input_visible, media_visible }`。
- 关键词判定：
  - `INVALID_ROOM_KEYWORDS`（页面不存在/404 等）→ fatal error。
  - `OFFLINE_ROOM_KEYWORDS`（暂未开播/已下播 等）→ LiveStatus::Offline。
  - `LIVE_ROOM_KEYWORDS`（直播中/正在直播 等）+ `LIVE_VIEWER_RE`（N人观看）→ LiveStatus::Live。
  - `ACCESS_LIMITED_KEYWORDS`（错误代码22 等）→ 限流，非 fatal 但降级。
- 备选：`KuaishouLiveDetailResponse` 接口查 `author.living` 字段。

### 联动触发（MonitorTransition）
`should_trigger` 判定：`detected_status == Live && !triggered_for_current_live`。
触发时调用下游接口：
- `scene_play::start_scene(scene_id, group_id)` — 拉起小号互动场景。
- `shop_product_script::play_product_script(product_script_id, product_script_account_id)` — 自动上车脚本。
- sub-account 批量 `enter_live_room`（按 group_id 过滤小号池）。

`auto_exit_sub_accounts`：直播结束（Live→Offline）时批量 `exit_live_room`/`close_session`。

### 状态广播
`LiveRoomMonitorState` 变化 → `app_handle.emit("live-room-monitor-state-changed", state)`。

### 下游接口契约
本任务定义「触发接口」契约，下游 task 实现：
```rust
pub trait LiveRoomTrigger {
    async fn on_live_start(&self, scene_id: i64, group_id: Option<&str>) -> Result<()>;
    async fn on_live_end(&self, group_id: Option<&str>, auto_exit: bool) -> Result<()>;
}
```
scene-play / shop-product-script / sub-account 各自实现此 trait（或在 design.md 决策用直接函数调用）。

### 风险与取舍
- **关键词表维护**：OFFLINE/LIVE/INVALID/ACCESS_LIMITED 四组关键词是 jieger 经验值，可能过期——真号校准时更新。
- **下游触发失败**：触发接口返回错误时，`last_product_script_result.error` 记录，不中断监控循环。
- **轮询频率**：快慢两档自动切换，triggered 后转快轮询一段时间再回慢。
- **MONITOR_SESSION_ID**：jieger 用 `__live_room_monitor__` 魔术 ID，Cloaksession 用 profile_id 或独立 monitor 会话。

### 验证
- 单元测试：MonitorTransition 逻辑（should_trigger 判定）、关键词分类。
- 集成测试：mock 下游 trait，验证 on_live_start/on_live_end 调用。
- 真号直播间监控验收单独确认。
