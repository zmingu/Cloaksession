# design — scene-play

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/scene_play.rs`。在 sub-account 小号会话上运行，调 send_danmaku/click_like/click_follow。

### Scene 模型
```rust
pub enum TriggerMode { RelativeTime, LocalTime }
pub enum SceneLineAction { Danmaku, Like, Follow }
pub struct SceneLine { pub id: i64, pub ord: i32, pub message: String, pub time_offset_sec: i64, pub action_type: SceneLineAction }
pub struct Scene { pub id: i64, pub lines: Vec<SceneLine>, pub trigger_mode: TriggerMode, pub group_id: Option<String> }
```
Scene 存储：profile-manager 新增 `scenes` + `scene_lines` 表（或 settings-store JSON，决策：独立表，因需 CRUD + 排序）。

### 时间计算
- `relative-time`：`trigger_at = start_ms + time_offset_sec * 1000`。
- `local-time`：`next_local_timestamp(secs_of_day)` —— 当天该时刻已过则顺延明天。用 chrono NaiveTime/NaiveDate。

### 时间扰动（反风控）
每条 ±300ms 随机 jitter：`jitter = (rand() - 0.5) * 600`，降低 round-robin 节奏机械感。

### 小号池
`get_ready_sub_account_pool(group_id)`：
- 取 kind=kuaishou_sub 的 business_accounts，按 group_id 过滤。
- 已登录（BrowserSession 存在 + status=connected）。
- 已进入直播间（page.url 符合 kuaishou.com/live/ 观众侧 URL）。
- 返回 `Vec<PoolMember { id, name }>`。

### round-robin 分配
`build_schedule(scene, pool, start_ms)`：每条 `member = pool[idx % pool.len()]`，加 jitter。

### 动态池（allowDynamicPool）
启动时无小号池 → 允许每条到点时动态 `get_ready_sub_account_pool` 取最新池。

### actionType 派发
- Danmaku → `sub_account::send_danmaku(account_id, message)`
- Like → `sub_account::click_like(account_id, count)`（幂等）
- Follow → `sub_account::click_follow(account_id)`（幂等，已关注直接返回 ok）

### 互动落库
每条派发结果 `record_interaction(account_id, action, message, result)` 落 sub_account_interactions（sub-account 子任务建表）。

### 调度
每条独立 `tokio::spawn` + `sleep_until(Instant::from_millis(trigger_at + jitter))`，到点派发。`PlaySession { timers, cancelled, dynamic_pool, group_id, next_pool_index }`。cancel 经 select! 优先。

### 状态广播
- `scene.started` → `{ scene_id, schedule, started_at }`
- `scene.progress` → `{ scene_id, sent_count, total_count, last_item, ok, error }`
- `scene.finished` → `{ scene_id, stopped?, reason? }`

### 与 auto-message 共用调度
auto-message 也是时间轴派发，但只有 danmaku 无 like/follow，且无小号分配。可考虑共用底层 `schedule_lines`（design.md 决策：先独立实现，后续重构合并）。

### 风险与取舍
- **local-time 顺延**：当天已过顺延明天，若场景是「每天定时」则需循环重启（本任务只单次，循环属 live-room-monitor 联动）。
- **池为空**：非 dynamic 模式下池为空 → 报错不启动；dynamic 模式允许启动，到点动态取。
- **click_like/click_follow 幂等**：已关注/已点赞不重复操作。

### 验证
- 单元测试：next_local_timestamp 顺延、build_schedule round-robin + jitter、pool 过滤。
- 集成测试：mock sub-account 接口，验证 danmaku/like/follow 派发顺序。
- 真号场景播放验收单独确认。
