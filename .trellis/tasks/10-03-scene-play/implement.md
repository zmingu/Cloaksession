# implement — scene-play

## 执行清单

### 1. Scene 模型与存储
- [ ] 1.1 `crates/tauri-app/src/driver/scene_play.rs` 建 TriggerMode/SceneLineAction/SceneLine/Scene 结构体
- [ ] 1.2 profile-manager 新增 `scenes` + `scene_lines` 表 + migration（scene_id PK/lines FK + ord + action_type + time_offset_sec + trigger_mode + group_id）
- [ ] 1.3 Scene CRUD：`create_scene`/`get_scene`/`list_scenes`/`update_scene`/`delete_scene`
- [ ] 1.4 SceneLine CRUD：`add_line`/`update_line`/`delete_line`/`reorder_lines`

### 2. 时间计算与调度
- [ ] 2.1 `next_local_timestamp(secs_of_day)` 用 chrono，当天已过顺延明天
- [ ] 2.2 `build_schedule(scene, pool, start_ms)` round-robin + ±300ms jitter
- [ ] 2.3 每条独立 tokio::spawn + sleep_until(trigger_at + jitter)
- [ ] 2.4 cancel select! 优先中断 sleep_until

### 3. 小号池
- [ ] 3.1 `get_ready_sub_account_pool(group_id)` 取 kind=kuaishou_sub + group_id 过滤 + 已登录 + 已进直播间
- [ ] 3.2 `allow_dynamic_pool` 模式：到点动态取最新池

### 4. actionType 派发
- [ ] 4.1 Danmaku → sub_account::send_danmaku
- [ ] 4.2 Like → sub_account::click_like（幂等）
- [ ] 4.3 Follow → sub_account::click_follow（幂等，已关注返回 ok）
- [ ] 4.4 每条结果 record_interaction 落库

### 5. 状态广播
- [ ] 5.1 `scene.started` emit `{ scene_id, schedule, started_at }`
- [ ] 5.2 `scene.progress` emit `{ scene_id, sent_count, total_count, last_item, ok, error }`
- [ ] 5.3 `scene.finished` emit `{ scene_id, stopped?, reason? }`
- [ ] 5.4 PlaySession 状态管理 `HashMap<i64, PlaySession>`

### 6. IPC 命令层
- [ ] 6.1 `crates/tauri-app/src/commands/scene_play.rs` 新建
- [ ] 6.2 `play_scene`/`stop_scene`/`get_scene_state`
- [ ] 6.3 Scene/Line CRUD 命令
- [ ] 6.4 `commands/mod.rs` 注册 + 前端 ipc.ts wrapper

### 7. 验证
- [ ] 7.1 `cargo test -p tauri-app --locked`（next_local_timestamp + build_schedule + pool 过滤 + mock 派发）
- [ ] 7.2 `cargo test -p profile-manager --locked`（scenes/scene_lines migration + CRUD）
- [ ] 7.3 `cargo check --workspace --locked`
- [ ] 7.4 真号场景播放验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo test -p profile-manager --locked
cargo check --workspace --locked
```

## Rollback 点
- 1 模型+存储 commit
- 2 调度 commit
- 3 小号池 commit
- 4 派发 commit
- 5 广播 commit
- 6 IPC 层 commit
- 7 验证后准备 archive

## 依赖前置
- `sub-account`（send_danmaku/click_like/click_follow 接口 + 小号池）

## Notes
- local-time 顺延明天，单次执行不循环（循环属 live-room-monitor 联动）。
- 池为空：非 dynamic 报错，dynamic 到点动态取。
- click_like/click_follow 幂等。
