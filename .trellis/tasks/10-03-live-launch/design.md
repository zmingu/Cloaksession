# design — live-launch

## 技术设计

### 模块位置
新增 `crates/tauri-app/src/driver/live_launch.rs`（ffmpeg 子进程管理属应用层，不归 cdp-driver）。推流凭据获取用 cdp-driver TaskPage。新 IPC 命令挂 `commands/live_launch.rs`。

### ffmpeg 子进程管理
- 用 `tokio::process::Command`（异步子进程），`windowsHide: true` 等价（Windows 不弹窗）。
- `PLACEHOLDER_ARGS_BEFORE_TARGET`（黑屏 1fps 50kbps aac 32k flv）作为 Rust 常量数组。
- 真实视频推流：`-re -i <videoPath> -c copy -f flv <target>`（循环用 `-stream_loop -1`）。
- stderr 收集：`Child::stderr` pipe，tokio::io 读取 tail（保留最近 N 行）。
- exit code 记录到 StreamingState。

### 推流凭据获取（TaskPage）
- `TaskPage::navigate(liveControlUrl)` → `evaluate(JS 取 RTMP server/streamKey/liveStreamId)`。
- placeholder 标志位：凭据为空或明显占位值时 `placeholder=true`。
- 与 ks-platform-primitives 的 connect 协作：先 `ensure_auth` 进中控页，再取凭据。

### StreamingState 状态机
```rust
pub enum StreamingStatus { Idle, Starting, Streaming, Stopping, Stopped, Error }
pub struct StreamingState {
    account_id: String,
    status: StreamingStatus,
    video_path: Option<String>,
    target: Option<String>,
    live_stream_id: Option<String>,
    started_at: Option<i64>,
    stopped_at: Option<i64>,
    exit_code: Option<i32>,
    error: Option<String>,
}
```
状态存储：`tokio::sync::RwLock<HashMap<String, StreamingState>>`（per-account）。状态变化 → `app_handle.emit("live-launch-state-changed", state)`。

### heartbeat 占位推流
- `start_heartbeat(account_id, target)` / `stop_heartbeat(account_id)`。
- heartbeat 与正式推流互斥：start streaming 时 stop heartbeat。
- ffmpeg 不可用（`check_prerequisites` 找不到 ffmpeg）时跳过 heartbeat，warn 日志。

### ffmpeg 路径解析
`resolve_bundled_ffmpeg_path()`：Windows `ffmpeg.exe`，资源目录 `resources/ffmpeg/`。打包后 `process.resourcesPath` 等价 → Tauri `resource_dir()`。

### 凭据 placeholder 语义
当无视频路径时启动 placeholder 推流（黑屏静音），保持 RTMP 连接不断。有视频路径时真实推流。

### 风险与取舍
- **ffmpeg 二进制**：不打包，用户环境自带；`check_prerequisites` 失败时降级（heartbeat 跳过，真实推流报错）。
- **循环推流**：`-stream_loop -1` 是 ffmpeg 原生参数，无需 Rust 侧循环 spawn。
- **进程取消**：tokio::Child 的 kill 在 cancel 时调用；drop Child 不保证 kill，需显式管理。

### 验证
- 单元测试：StreamingState 状态转换、`resolve_bundled_ffmpeg_path` 路径拼接。
- 集成测试：mock ffmpeg（用 `echo` 等假命令验证 spawn/exit/stderr 收集）。
- 真号推流验收单独确认。
