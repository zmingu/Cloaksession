# implement — live-launch

## 执行清单

### 1. ffmpeg 管理层
- [ ] 1.1 `crates/tauri-app/src/driver/live_launch.rs` 建 StreamingStatus/StreamingState 结构体
- [ ] 1.2 `PLACEHOLDER_ARGS` 常量数组（黑屏 1fps 50kbps aac 32k flv）
- [ ] 1.3 `resolve_bundled_ffmpeg_path()` + `check_prerequisites()` 实现
- [ ] 1.4 `start_streaming(account_id, video_path, rtmp_server, stream_key)` tokio::process::Command spawn
- [ ] 1.5 stderr tail 收集（tokio::io::BufReader 读到 VecDeque<String>，保留最近 20 行）
- [ ] 1.6 `stop_streaming(account_id)` kill 子进程
- [ ] 1.7 `start_heartbeat(account_id, target)` / `stop_heartbeat(account_id)` 占位推流
- [ ] 1.8 StreamingState 存储 `RwLock<HashMap<String, StreamingState>>` + 状态转换函数

### 2. 推流凭据获取
- [ ] 2.1 `fetch_stream_credentials(profile_id)` 经 TaskPage navigate 中控页 + evaluate JS 取 RTMP server/streamKey/liveStreamId
- [ ] 2.2 placeholder 标志位判定（凭据空/占位值）
- [ ] 2.3 与 ks-platform-primitives ensure_auth 协作（先 ensure_auth 进中控页，再取凭据）

### 3. IPC 命令层
- [ ] 3.1 `crates/tauri-app/src/commands/live_launch.rs` 新建
- [ ] 3.2 `#[tauri::command] async fn start_live_streaming(profile_id, video_path, rtmp_server, stream_key) -> Result<StreamingState, String>`
- [ ] 3.3 `#[tauri::command] async fn stop_live_streaming(profile_id) -> Result<(), String>`
- [ ] 3.4 `#[tauri::command] async fn check_ffmpeg_prerequisites() -> Result<{ffmpeg_ok, ffmpeg_path}, String>`
- [ ] 3.5 `#[tauri::command] async fn get_streaming_state(profile_id) -> Result<StreamingState, String>`
- [ ] 3.6 状态变化 `app_handle.emit("live-launch-state-changed", state)`
- [ ] 3.7 `commands/mod.rs` 注册命令 + 前端 ipc.ts wrapper

### 4. 验证
- [ ] 4.1 `cargo test -p tauri-app --locked`（mock ffmpeg spawn/exit/stderr）
- [ ] 4.2 `cargo check --workspace --locked`
- [ ] 4.3 真号推流集成验收（单独确认）

## 验证命令
```bash
cargo test -p tauri-app --locked
cargo check --workspace --locked
```

## Rollback 点
- 1.x ffmpeg 层可独立 commit
- 2-3 凭据+IPC 层 commit
- 4 验证后准备 archive

## 依赖前置
- `ks-platform-primitives`（ensure_auth 进中控页取凭据）
- 协作：`mate-login`（伴侣登录态供推流 token）

## Notes
- ffmpeg 不打包，用户环境自带；不可用时 heartbeat 跳过、真实推流报错。
- `-stream_loop -1` 循环推流用 ffmpeg 原生参数。
