//! jieger 开播控制：heartbeat 占位推流 + 本地视频循环推流（ffmpeg）+ RTMP 凭据获取。
//!
//! # 取流来源：两种模式
//!
//! - **mate（伴侣）模式**（对齐 jieger，推荐）：读 `mate_accounts` 凭证 →
//!   [`super::live_mate`] 三步开播（authStatus → prePush → startPush）→
//!   `splitRtmpUrl` → [`StreamCredentials`]。**全程不启浏览器**（伴侣账号无 profile）。
//!   入口见 [`TauriBrowserDriver::fetch_mate_stream_credentials`] /
//!   [`TauriBrowserDriver::live_launch_start_mate_stream`] /
//!   [`TauriBrowserDriver::live_launch_stop_mate_stream`]。
//! - **profile（浏览器抓页）模式**（保留，标注为历史路径）：先 `require_session`
//!   再进开播中控页抓 `rtmpServer/streamKey/liveStreamId`，见
//!   [`TauriBrowserDriver::fetch_stream_credentials`]。该路径仍被
//!   `live_launch_credentials` / `live_launch_heartbeat_start` /
//!   `live_launch_stream_start` 命令引用，**未删除**；mate 模式不复用它。
//!
//! # 索引：两个互不混用的桶
//!
//! [`LiveLaunchRegistry`] 按 [`SubjectKind`] 分桶：`profiles`（key = `profile_id`）
//! 与 `mates`（key = `mate_account_id`）。同桶内 heartbeat 与正式推流互斥；
//! 跨桶互不影响（即使 id 字符串相同）。
//!
//! # 范围与协作
//!
//! - 本模块只实现开播推流这一件事：ffmpeg 子进程管理、推流状态机、RTMP
//!   凭据获取。**不碰 cdp-driver 既有 API**，只调用已有的公开能力
//!  （`BrowserSession::new_page` / `TaskPage::navigate` / `evaluate`）。
//! - 与 ks-platform-primitives 的 `ensure_auth` 协作点是 profile 模式的
//!   [`TauriBrowserDriver::fetch_stream_credentials`]
//!   开头的会话守卫：先要求浏览器已启动（`require_session`，未启动直接报错，
//!   调用方先 `launch`），再进中控页抓凭据。真正的平台登录态校验以后由
//!   ks-platform-primitives 的 ensure_auth 接管，本模块不重复实现。
//! - ffmpeg **不打包**：用户环境自带。[`check_prerequisites`] 按
//!   `resources/ffmpeg/`（Tauri `resource_dir()` 下）→ PATH 顺序查找，
//!   找不到时 heartbeat 与正式推流都直接报错，不静默跳过。
//! - 循环用 ffmpeg 原生 `-stream_loop -1`，不在 Rust 侧循环拉起进程。
//! - 取消时必须显式调用 `tokio::process::Child::kill`（`drop` 不保证杀掉
//!   子进程），见 [`supervise_push`]。
//! - 状态变化通过 `live-launch-state-changed` 事件推送前端，见
//!   [`LIVE_LAUNCH_STATE_CHANGED`]。
//! - token **不落日志**：mate 取流/关播/心跳的请求与错误文案由 `live_mate`
//!   层保证不含 token（见该模块文档）。

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::Duration;

use cdp_driver::TaskCancel;
use multizen_core::{MultizenError, Result};
use serde::Serialize;
use tauri::{Emitter, Manager};
use tokio::sync::{mpsc, RwLock};

use super::{live_mate, TauriBrowserDriver};

/// 状态变化推送事件名。前端 `listen("live-launch-state-changed")`。
pub const LIVE_LAUNCH_STATE_CHANGED: &str = "live-launch-state-changed";

/// Heartbeat 占位推流的固定参数：黑屏（1280x720）1fps、视频 50kbps、
/// 音频 aac 32k、flv 封装。调用方在末尾追加输出 target
///（RTMP 地址，或 warmup 时的 `null` 槽位，见 [`placeholder_argv`]）。
pub const PLACEHOLDER_ARGS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-f",
    "lavfi",
    "-i",
    "color=c=black:s=1280x720:r=1",
    "-f",
    "lavfi",
    "-i",
    "anullsrc=r=44100:cl=stereo",
    "-c:v",
    "libx264",
    "-r",
    "1",
    "-b:v",
    "50k",
    "-pix_fmt",
    "yuv420p",
    "-c:a",
    "aac",
    "-b:a",
    "32k",
    "-ar",
    "44100",
    "-f",
    "flv",
];

/// stderr 只保留最近这么多行（内存有界 + IPC 快照有界）。
pub const STDERR_TAIL_LINES: usize = 20;

/// 单行 stderr 上限（字符数），防止一行刷爆内存/事件 payload。
const STDERR_LINE_CAP: usize = 2000;

/// 中控页抓凭据的 JS（best-effort）：优先读页面预留的 data 属性，
/// 退化为全文正则找 `rtmp(s)://…`。页面契约（jieger 中控页）落地后
/// 再收紧选择器；抓不到走 placeholder 凭据，不抛错。
const CREDENTIALS_EXTRACTOR_JS: &str = r#"(() => {
  const pick = (...sels) => {
    for (const s of sels) {
      const el = document.querySelector(s);
      if (el && el.textContent && el.textContent.trim()) return el.textContent.trim();
    }
    return "";
  };
  const attr = (name) => {
    const el = document.querySelector(`[data-${name}]`);
    return el ? (el.getAttribute(`data-${name}`) || "").trim() : "";
  };
  const body = (document.body && (document.body.innerText || "")) || "";
  const server = attr("rtmp-server") || pick("[data-rtmp-server]") ||
    (body.match(/rtmps?:\/\/[^\s"'<>]+/) || [""])[0];
  const key = attr("stream-key") || pick("[data-stream-key]") || "";
  const liveId = attr("live-id") || pick("[data-live-id]") || "";
  return { rtmpServer: server, streamKey: key, liveStreamId: liveId };
})()"#;

const LEASE_WAIT: Duration = Duration::from_secs(10);
const NAV_WAIT: Duration = Duration::from_secs(20);
const EVAL_WAIT: Duration = Duration::from_secs(8);
const STOP_WAIT: Duration = Duration::from_secs(10);

/// 推流状态机。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamingStatus {
    Idle,
    Starting,
    Streaming,
    Stopping,
    Stopped,
    Error,
}

impl StreamingStatus {
    /// 是否持有活跃推流（启动中也算：与另一路推流互斥）。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Streaming)
    }

    /// 合法转换表。非法转换调用方直接拒绝，不静默跳转。
    pub fn can_transition(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Idle, Self::Starting)
                | (Self::Starting, Self::Streaming)
                | (Self::Starting, Self::Error)
                | (Self::Starting, Self::Stopping)
                | (Self::Streaming, Self::Stopping)
                | (Self::Stopping, Self::Stopped)
                | (Self::Stopping, Self::Error)
                | (Self::Stopped, Self::Starting)
                | (Self::Error, Self::Starting)
        )
    }
}

/// 推流模式：heartbeat 占位（黑屏）与正式视频推流互斥（单账号同时只许一路）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamMode {
    Heartbeat,
    Realtime,
}

/// 推流主体类型：浏览器环境（profile 桶）与直播伴侣账号（mate 桶）。
///
/// 两个桶的 key **互不混用**：即使某个 `mate_account_id` 与某个 `profile_id`
/// 恰好是同一个字符串，也分别落在各自桶中，互不影响（互斥判定只看同桶）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SubjectKind {
    /// 浏览器环境（`profiles` 表，key = `profile_id`）。
    Profile,
    /// 直播伴侣账号（`mate_accounts` 表，key = `mate_account_id`）。
    Mate,
}

/// RTMP 推流凭据。`placeholder == true` 表示没抓到真实值（未登录/页面契约
/// 未命中/抓取失败），调用方据此决定：heartbeat 可降级 warmup，正式推流拒绝。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamCredentials {
    pub rtmp_server: String,
    pub stream_key: String,
    pub live_stream_id: String,
    pub placeholder: bool,
}

impl StreamCredentials {
    pub fn placeholder() -> Self {
        Self {
            rtmp_server: "rtmp://127.0.0.1:9/live/placeholder".into(),
            stream_key: "placeholder".into(),
            live_stream_id: String::new(),
            placeholder: true,
        }
    }

    /// 完整推流地址。注意：只进 ffmpeg argv，不进日志/事件（见 redacted）。
    pub fn target_url(&self) -> String {
        format!("{}/{}", self.rtmp_server, self.stream_key)
    }

    /// 脱敏展示：server 保留，key 打码。事件/快照只许用这个。
    pub fn redacted_target(&self) -> String {
        format!("{}/***", self.rtmp_server)
    }

    pub fn is_usable(&self) -> bool {
        !self.placeholder && !self.rtmp_server.is_empty() && !self.stream_key.is_empty()
    }
}

/// 从 evaluate 返回值解析凭据。server/key 缺任意一个即视为未命中 → None。
fn parse_credentials(value: &serde_json::Value) -> Option<StreamCredentials> {
    let server = value
        .get("rtmpServer")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let key = value
        .get("streamKey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if server.is_empty() || key.is_empty() {
        return None;
    }
    if !server.starts_with("rtmp://") && !server.starts_with("rtmps://") {
        return None;
    }
    let live_id = value
        .get("liveStreamId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    Some(StreamCredentials {
        rtmp_server: server.to_string(),
        stream_key: key.to_string(),
        live_stream_id: live_id,
        placeholder: false,
    })
}

/// mate 模式凭据组装：把 `liveMate` 的 `pushRtmpUrl` + `liveStreamId` 拆成
/// [`StreamCredentials`]（`placeholder=false`，`liveStreamId` 保留）。
///
/// 纯函数，可离线单测（不触碰网络/ffmpeg）。
pub fn mate_stream_credentials(push_rtmp_url: &str, live_stream_id: &str) -> StreamCredentials {
    let split = live_mate::split_rtmp_url(push_rtmp_url);
    StreamCredentials {
        rtmp_server: split.rtmp_server,
        stream_key: split.stream_key,
        live_stream_id: live_stream_id.to_string(),
        placeholder: false,
    }
}

/// 对外的单账号推流快照（可序列化，进 IPC 与事件）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingState {
    /// 推流主体 id：profile 模式为 `profile_id`，mate 模式为 `mate_account_id`。
    pub profile_id: String,
    /// 主体类型（profile 桶 / mate 桶）。
    pub subject_kind: SubjectKind,
    pub status: StreamingStatus,
    pub mode: Option<StreamMode>,
    /// 脱敏 target（`rtmp://host/***`），stream_key 永不外泄。
    pub target: Option<String>,
    pub pid: Option<u32>,
    /// ffmpeg stderr 最近 [`STDERR_TAIL_LINES`] 行。
    pub stderr_tail: Vec<String>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub placeholder_credentials: bool,
    /// mate 模式取流得到的 `liveStreamId`（关播时回传平台）；profile 模式多为空。
    pub live_stream_id: Option<String>,
    pub started_at: Option<String>,
}

impl StreamingState {
    fn idle(kind: SubjectKind, subject_id: &str) -> Self {
        Self {
            profile_id: subject_id.to_string(),
            subject_kind: kind,
            status: StreamingStatus::Idle,
            mode: None,
            target: None,
            pid: None,
            stderr_tail: Vec::new(),
            exit_code: None,
            error: None,
            placeholder_credentials: false,
            live_stream_id: None,
            started_at: None,
        }
    }
}

/// 状态变化事件 payload（字段与 [`StreamingState`] 对齐的子集）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveLaunchStateChanged {
    pub profile_id: String,
    pub subject_kind: SubjectKind,
    pub status: StreamingStatus,
    pub mode: Option<StreamMode>,
    pub target: Option<String>,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub placeholder_credentials: bool,
    pub live_stream_id: Option<String>,
}

impl LiveLaunchStateChanged {
    fn of(state: &StreamingState) -> Self {
        Self {
            profile_id: state.profile_id.clone(),
            subject_kind: state.subject_kind,
            status: state.status,
            mode: state.mode,
            target: state.target.clone(),
            pid: state.pid,
            exit_code: state.exit_code,
            error: state.error.clone(),
            placeholder_credentials: state.placeholder_credentials,
            live_stream_id: state.live_stream_id.clone(),
        }
    }
}

/// ffmpeg 可用性检查结果（进 IPC）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerequisitesReport {
    pub available: bool,
    pub ffmpeg_path: Option<String>,
    pub searched: Vec<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// ffmpeg 路径解析
// ---------------------------------------------------------------------------

/// 平台可执行文件名：Windows `ffmpeg.exe`，其余 `ffmpeg`。
pub fn ffmpeg_file_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

/// Tauri `resource_dir()` 下的打包（预留）位置：`resources/ffmpeg/ffmpeg(.exe)`。
/// 注意：ffmpeg **不打包**（用户环境自带），此路径只是优先查找位。
pub fn resolve_bundled_ffmpeg_path(resource_dir: &Path) -> PathBuf {
    resource_dir.join("ffmpeg").join(ffmpeg_file_name())
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// 按给定目录顺序查找 ffmpeg 可执行文件。纯函数，可单元测试。
pub fn find_ffmpeg(search_dirs: &[PathBuf]) -> Option<PathBuf> {
    search_dirs
        .iter()
        .map(|dir| dir.join(ffmpeg_file_name()))
        .find(|p| is_executable(p))
}

/// PATH 环境变量中查找 ffmpeg。返回完整路径或 None。
pub fn find_ffmpeg_in_path() -> Option<PathBuf> {
    let name = ffmpeg_file_name();
    // 直接可执行（当前目录/PATH 已解析）先试一次。
    let direct = PathBuf::from(name);
    if is_executable(&direct) {
        return Some(direct);
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// 前置检查：bundled 预留位 → 可执行文件所在目录 → PATH。
/// 找不到则报错（heartbeat 与正式推流共用此门，不静默跳过）。
pub fn check_prerequisites(resource_dir: Option<&Path>) -> std::result::Result<PathBuf, String> {
    let mut searched: Vec<String> = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = resource_dir {
        candidates.push(dir.join("ffmpeg"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("ffmpeg"));
        }
    }
    // 开发期兜底：manifest 目录下的 resources/ffmpeg（与 release resource_dir 同构）。
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/ffmpeg"));
    for dir in &candidates {
        searched.push(dir.join(ffmpeg_file_name()).display().to_string());
    }
    if let Some(found) = find_ffmpeg(&candidates) {
        return Ok(found);
    }
    searched.push(format!("PATH:{ffmpeg_file_name}", ffmpeg_file_name = ffmpeg_file_name()));
    find_ffmpeg_in_path().ok_or_else(|| {
        format!(
            "找不到可用的 ffmpeg（{}）：请先在用户环境安装 ffmpeg 并加入 PATH；ffmpeg 不随应用打包",
            searched.join("；")
        )
    })
}

pub fn prerequisites_report(resource_dir: Option<&Path>) -> PrerequisitesReport {
    let mut searched: Vec<String> = Vec::new();
    if let Some(dir) = resource_dir {
        searched.push(
            resolve_bundled_ffmpeg_path(dir).display().to_string(),
        );
    }
    searched.push(format!("PATH:{}", ffmpeg_file_name()));
    match check_prerequisites(resource_dir) {
        Ok(path) => PrerequisitesReport {
            available: true,
            ffmpeg_path: Some(path.display().to_string()),
            searched,
            error: None,
        },
        Err(e) => PrerequisitesReport {
            available: false,
            ffmpeg_path: None,
            searched,
            error: Some(e),
        },
    }
}

// ---------------------------------------------------------------------------
// ffmpeg argv 组装（纯函数，可测试）
// ---------------------------------------------------------------------------

/// Heartbeat 占位推流 argv：`PLACEHOLDER_ARGS + [target]`，target 为 RTMP 全地址。
pub fn placeholder_argv(rtmp_target: &str) -> Vec<String> {
    PLACEHOLDER_ARGS
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(rtmp_target.to_string()))
        .collect()
}

/// Heartbeat 无真实凭据时的 warmup argv：黑屏推往本地 null 槽位，
/// 只热管道、不占房间。`placeholder_credentials == true` 时用这个。
pub fn placeholder_warmup_argv() -> Vec<String> {
    PLACEHOLDER_ARGS
        .iter()
        .map(|s| s.to_string())
        .chain(["-f".to_string(), "null".to_string(), "-".to_string()])
        .collect()
}

/// 正式视频推流 argv：原生 `-stream_loop -1` 循环，`-c copy` 不转码。
pub fn realtime_argv(video_path: &str, rtmp_target: &str) -> Vec<String> {
    vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-stream_loop".into(),
        "-1".into(),
        "-i".into(),
        video_path.into(),
        "-c".into(),
        "copy".into(),
        "-f".into(),
        "flv".into(),
        rtmp_target.into(),
    ]
}

// ---------------------------------------------------------------------------
// stderr tail（纯函数 + 异步收集）
// ---------------------------------------------------------------------------

/// 追加一行并截断到最近 [`STDERR_TAIL_LINES`] 行。
pub fn push_stderr_line(tail: &mut VecDeque<String>, line: String) {
    let mut line = line;
    if line.len() > STDERR_LINE_CAP {
        line.truncate(STDERR_LINE_CAP);
    }
    tail.push_back(line);
    while tail.len() > STDERR_TAIL_LINES {
        tail.pop_front();
    }
}

/// 把子进程 stderr 逐行收进共享 tail（`None` = 没拿到管道，直接返回）。
async fn drain_stderr_to_tail(
    stderr: Option<tokio::process::ChildStderr>,
    tail: Arc<StdMutex<VecDeque<String>>>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let Some(stderr) = stderr else { return };
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                if let Ok(mut guard) = tail.lock() {
                    push_stderr_line(&mut guard, line);
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
}

// ---------------------------------------------------------------------------
// per-account 注册表（进程级单例）
// ---------------------------------------------------------------------------

struct LiveEntry {
    kind: SubjectKind,
    status: StreamingStatus,
    mode: Option<StreamMode>,
    target_redacted: Option<String>,
    pid: Option<u32>,
    exit_code: Option<i32>,
    error: Option<String>,
    placeholder_credentials: bool,
    /// mate 模式取流得到的 `liveStreamId`（关播回传平台；profile 模式为 None）。
    live_stream_id: Option<String>,
    started_at: Option<String>,
    stderr: Arc<StdMutex<VecDeque<String>>>,
    kill_tx: Option<mpsc::Sender<()>>,
}

impl LiveEntry {
    fn fresh(mode: StreamMode, target_redacted: String, placeholder: bool) -> Self {
        Self {
            kind: SubjectKind::Profile,
            status: StreamingStatus::Starting,
            mode: Some(mode),
            target_redacted: Some(target_redacted),
            pid: None,
            exit_code: None,
            error: None,
            placeholder_credentials: placeholder,
            live_stream_id: None,
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            stderr: Arc::new(StdMutex::new(VecDeque::new())),
            kill_tx: None,
        }
    }

    fn snapshot(&self, subject_id: &str) -> StreamingState {
        let stderr_tail = self
            .stderr
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default();
        StreamingState {
            profile_id: subject_id.to_string(),
            subject_kind: self.kind,
            status: self.status,
            mode: self.mode,
            target: self.target_redacted.clone(),
            pid: self.pid,
            stderr_tail,
            exit_code: self.exit_code,
            error: self.error.clone(),
            placeholder_credentials: self.placeholder_credentials,
            live_stream_id: self.live_stream_id.clone(),
            started_at: self.started_at.clone(),
        }
    }
}

/// 一个主体桶：`key -> 条目`。profile 桶与 mate 桶各自独立，key 不混用。
#[derive(Default)]
struct Bucket {
    entries: HashMap<String, LiveEntry>,
}

impl Bucket {
    fn get(&self, id: &str) -> Option<&LiveEntry> {
        self.entries.get(id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut LiveEntry> {
        self.entries.get_mut(id)
    }

    fn insert(&mut self, id: String, entry: LiveEntry) {
        self.entries.insert(id, entry);
    }
}

struct LiveLaunchRegistry {
    profiles: RwLock<Bucket>,
    mates: RwLock<Bucket>,
}

impl LiveLaunchRegistry {
    fn new() -> Self {
        Self {
            profiles: RwLock::new(Bucket::default()),
            mates: RwLock::new(Bucket::default()),
        }
    }

    /// 主体类型 → 对应桶（`REGISTRY` 为 static，返回引用可长期持有）。
    fn bucket(&self, kind: SubjectKind) -> &RwLock<Bucket> {
        match kind {
            SubjectKind::Profile => &self.profiles,
            SubjectKind::Mate => &self.mates,
        }
    }
}

static REGISTRY: LazyLock<LiveLaunchRegistry> = LazyLock::new(LiveLaunchRegistry::new);

/// 读快照（无记录时返回 Idle）。按桶隔离读取。
async fn snapshot_of(kind: SubjectKind, subject_id: &str) -> StreamingState {
    let bucket = REGISTRY.bucket(kind).read().await;
    bucket
        .get(subject_id)
        .map(|e| e.snapshot(subject_id))
        .unwrap_or_else(|| StreamingState::idle(kind, subject_id))
}

/// 读取并**清空** mate 条目的 `live_stream_id`（关播成功后回收，避免重复关播）。
async fn take_mate_live_stream_id(subject_id: &str) -> Option<String> {
    let mut bucket = REGISTRY.mates.write().await;
    bucket
        .get_mut(subject_id)
        .and_then(|entry| entry.live_stream_id.take())
}

/// 占位：Idle/Stopped/Error 允许启动，其余（Starting/Streaming）互斥拒绝。
/// 成功时写入 Starting 条目并返回 stderr 共享句柄 + redacted target。
/// `live_stream_id` 仅 mate 模式传入（关播回传平台）。
async fn try_reserve(
    kind: SubjectKind,
    subject_id: &str,
    mode: StreamMode,
    creds: &StreamCredentials,
    live_stream_id: Option<&str>,
) -> std::result::Result<Arc<StdMutex<VecDeque<String>>>, String> {
    let mut bucket = REGISTRY.bucket(kind).write().await;
    if let Some(existing) = bucket.get(subject_id) {
        if existing.status.is_active() {
            return Err(format!(
                "账号 {subject_id} 已有推流在运行（{:?}），heartbeat 与正式推流互斥，请先停止",
                existing.mode
            ));
        }
    }
    let mut entry = LiveEntry::fresh(mode, creds.redacted_target(), creds.placeholder);
    entry.kind = kind;
    entry.live_stream_id = live_stream_id.map(|s| s.to_string());
    let stderr = entry.stderr.clone();
    // 重启时清掉上一轮的退出码/错误，避免旧状态污染。
    entry.exit_code = None;
    entry.error = None;
    entry.pid = None;
    bucket.insert(subject_id.to_string(), entry);
    Ok(stderr)
}

/// 应用一次状态转换（带 `can_transition` 守卫）并返回快照；有 `app` 时 emit。
/// `app = None` 供离线测试使用（真实路径一律传 `Some`）。
async fn set_status(
    kind: SubjectKind,
    subject_id: &str,
    next: StreamingStatus,
    app: Option<&tauri::AppHandle>,
) -> StreamingState {
    let mut bucket = REGISTRY.bucket(kind).write().await;
    let Some(entry) = bucket.get_mut(subject_id) else {
        drop(bucket);
        return snapshot_of(kind, subject_id).await;
    };
    if entry.status != next {
        if entry.status.can_transition(next) {
            entry.status = next;
        } else {
            tracing::warn!(
                subject = %subject_id,
                from = ?entry.status,
                to = ?next,
                "live-launch: 非法状态转换，已忽略"
            );
            let snapshot = entry.snapshot(subject_id);
            drop(bucket);
            return snapshot;
        }
    }
    let snapshot = entry.snapshot(subject_id);
    drop(bucket);
    if let Some(app) = app {
        app.emit(LIVE_LAUNCH_STATE_CHANGED, &LiveLaunchStateChanged::of(&snapshot))
            .ok();
    }
    snapshot
}

async fn set_pid(kind: SubjectKind, subject_id: &str, pid: u32) {
    let mut bucket = REGISTRY.bucket(kind).write().await;
    if let Some(entry) = bucket.get_mut(subject_id) {
        entry.pid = Some(pid);
    }
}

async fn set_kill_tx(kind: SubjectKind, subject_id: &str, tx: mpsc::Sender<()>) {
    let mut bucket = REGISTRY.bucket(kind).write().await;
    if let Some(entry) = bucket.get_mut(subject_id) {
        entry.kill_tx = Some(tx);
    }
}

async fn take_kill_tx(kind: SubjectKind, subject_id: &str) -> Option<mpsc::Sender<()>> {
    let mut bucket = REGISTRY.bucket(kind).write().await;
    bucket.get_mut(subject_id)?.kill_tx.clone()
}

async fn fail_start(
    kind: SubjectKind,
    subject_id: &str,
    error: String,
    app: Option<&tauri::AppHandle>,
) -> StreamingState {
    {
        let mut bucket = REGISTRY.bucket(kind).write().await;
        if let Some(entry) = bucket.get_mut(subject_id) {
            // Starting → Error 合法；若已被并发 stop 改为 Stopping 则保留。
            if entry.status == StreamingStatus::Starting {
                entry.status = StreamingStatus::Error;
            }
            entry.error = Some(error.clone());
            entry.kill_tx = None;
        }
    }
    let snapshot = snapshot_of(kind, subject_id).await;
    if let Some(app) = app {
        app.emit(LIVE_LAUNCH_STATE_CHANGED, &LiveLaunchStateChanged::of(&snapshot))
            .ok();
    }
    snapshot
}

// ---------------------------------------------------------------------------
// supervisor：stderr 收集 + 退出码记录 + 状态终态
// ---------------------------------------------------------------------------

/// 持有 Child 的唯一 supervisor。`kill_rx` 收到信号时**显式** `kill()` 再
/// `wait()`（不依赖 drop）；子进程自己退出则记为非预期 Error。
async fn supervise_push(
    app: Option<tauri::AppHandle>,
    kind: SubjectKind,
    subject_id: String,
    mut child: tokio::process::Child,
    stderr: Option<tokio::process::ChildStderr>,
    tail: Arc<StdMutex<VecDeque<String>>>,
    mut kill_rx: mpsc::Receiver<()>,
) {
    let drain = drain_stderr_to_tail(stderr, tail.clone());
    tokio::pin!(drain);
    let exit_status = loop {
        tokio::select! {
            _ = &mut drain => {
                // stderr 先关（ffmpeg 仍可能跑），继续等退出/kill。
                match child.wait().await {
                    Ok(status) => break Some(status),
                    Err(e) => {
                        tracing::warn!(subject = %subject_id, error = %e, "live-launch: wait 失败");
                        break None;
                    }
                }
            }
            _ = kill_rx.recv() => {
                // 取消路径：显式 kill（drop 不保证杀子进程），再 wait 回收。
                if let Err(e) = child.kill().await {
                    tracing::debug!(subject = %subject_id, error = %e, "live-launch: kill 失败（可能已退出）");
                }
                match child.wait().await {
                    Ok(status) => break Some(status),
                    Err(e) => {
                        tracing::warn!(subject = %subject_id, error = %e, "live-launch: kill 后 wait 失败");
                        break None;
                    }
                }
            }
            status = child.wait() => {
                break status.ok();
            }
        }
    };
    // drain 收尾：把 kill/退出瞬间最后几行 stderr 也收进来。
    let _ = tokio::time::timeout(Duration::from_millis(500), drain).await;

    let code = exit_status.and_then(|s| s.code());
    // wait 完成后再读状态：stop() 先落 Stopping 再发 kill，所以 Stopping
    // 意味着这次退出是用户停止的（即使 kill 发出时进程刚好自然退出）。
    let requested_stop = {
        let bucket = REGISTRY.bucket(kind).read().await;
        bucket
            .get(&subject_id)
            .is_none_or(|e| e.status == StreamingStatus::Stopping)
    };
    {
        let mut bucket = REGISTRY.bucket(kind).write().await;
        if let Some(entry) = bucket.get_mut(&subject_id) {
            entry.exit_code = code;
            entry.kill_tx = None;
            if requested_stop {
                if entry.status.can_transition(StreamingStatus::Stopped) {
                    entry.status = StreamingStatus::Stopped;
                }
                entry.error = None;
            } else {
                if entry.status.can_transition(StreamingStatus::Error) {
                    entry.status = StreamingStatus::Error;
                }
                entry.error = Some(match code {
                    Some(c) => format!("ffmpeg 非预期退出（exit code {c}），见 stderr 尾部"),
                    None => "ffmpeg 非预期退出（被信号终止），见 stderr 尾部".into(),
                });
            }
        }
    }
    let snapshot = snapshot_of(kind, &subject_id).await;
    if let Some(app) = app.as_ref() {
        app.emit(LIVE_LAUNCH_STATE_CHANGED, &LiveLaunchStateChanged::of(&snapshot))
            .ok();
    }
}

/// 停止某桶内主体的 ffmpeg 推流：发 kill 信号 → supervisor 显式 `kill()` +
/// `wait()` → 落终态；等待终态超时则报错。profile / mate 共用（只按桶区分）。
///
/// **不触碰 liveMate**：mate 模式的关播（`stopLiveMatePush`）在
/// [`TauriBrowserDriver::live_launch_stop_mate_stream`] 里先于本函数调用。
async fn stop_push(
    kind: SubjectKind,
    subject_id: &str,
    app: Option<&tauri::AppHandle>,
) -> std::result::Result<StreamingState, String> {
    let tx = take_kill_tx(kind, subject_id).await;
    let snapshot = snapshot_of(kind, subject_id).await;
    if !snapshot.status.is_active() {
        return Err(format!(
            "账号 {subject_id} 当前未在推流（{:?}）",
            snapshot.status
        ));
    }
    // 先落 Stopping 再发 kill，避免 supervisor 把 kill 判成非预期退出。
    set_status(kind, subject_id, StreamingStatus::Stopping, app).await;
    match tx {
        Some(tx) => {
            if tx.send(()).await.is_err() {
                // supervisor 已退出（如子进程刚自然结束）：返回当前快照，
                // 终态则成功，否则报错。
                let snapshot = snapshot_of(kind, subject_id).await;
                if snapshot.status.is_active() {
                    return Err(format!("账号 {subject_id} 推流停止信号发送失败"));
                }
                return Ok(snapshot);
            }
        }
        None => {
            // supervisor 已退出（如子进程刚 crash）：直接返回当前快照。
            return Ok(snapshot_of(kind, subject_id).await);
        }
    }
    let deadline = tokio::time::Instant::now() + STOP_WAIT;
    loop {
        let snapshot = snapshot_of(kind, subject_id).await;
        if !snapshot.status.is_active() {
            return Ok(snapshot);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("账号 {subject_id} 停止超时，ffmpeg 进程可能仍在运行"));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

impl TauriBrowserDriver {
    /// 当前快照（无记录 → Idle）。profile 桶。
    pub async fn live_launch_status(&self, profile_id: &str) -> StreamingState {
        snapshot_of(SubjectKind::Profile, profile_id).await
    }

    /// mate 模式当前快照（无记录 → Idle）。mate 桶，key = `mate_account_id`。
    pub async fn live_launch_mate_status(&self, mate_account_id: &str) -> StreamingState {
        snapshot_of(SubjectKind::Mate, mate_account_id).await
    }

    /// 伴侣（mate）模式取流：读 `mate_accounts` 凭证 → `liveMate` 三步开播
    /// （authStatus → prePush → startPush）→ `split_rtmp_url` → [`StreamCredentials`]。
    ///
    /// 全程**不启浏览器**（伴侣账号无 profile）。`placeholder=false` 表示真实凭据；
    /// `live_stream_id` 一并保留，供停止时回传 `stopLiveMatePush`。
    /// 失败文案由 `live_mate` 层产出，**不含 token**。
    pub async fn fetch_mate_stream_credentials(
        &self,
        mate_account_id: &str,
    ) -> Result<StreamCredentials> {
        let creds = self.live_mate_start_push(mate_account_id).await?;
        Ok(mate_stream_credentials(
            &creds.push_rtmp_url,
            &creds.live_stream_id,
        ))
    }

    /// RTMP 凭据获取：先 ensure 会话（`require_session`，未 launch 直接报错；
    /// 登录态强校验以后由 ks-platform-primitives `ensure_auth` 接管），
    /// 再按 `control_url` 新开中控页 tab，经 TaskPage navigate + evaluate 抓取
    /// `rtmpServer/streamKey/liveStreamId`。
    ///
    /// - `control_url` 为空 → 不导航，直接返回 placeholder 凭据
    ///   （中控页契约未落地前的默认行为，由 `placeholder` 标志位标明）。
    /// - 抓取/解析失败 → placeholder 凭据（warn 日志），不抛错；
    ///   无活跃会话 → 硬错误。
    /// - 抓取用的临时 tab 无论成败都尽力关闭，不污染用户页面。
    pub async fn fetch_stream_credentials(
        &self,
        profile_id: &str,
        control_url: Option<&str>,
    ) -> Result<StreamCredentials> {
        // ensure_auth 协作点：会话必须存在（调用方先 launch）。
        let session = self.require_session(profile_id).await?;
        let Some(url) = control_url.filter(|u| !u.trim().is_empty()) else {
            tracing::debug!(profile = %profile_id, "live-launch: 未配置中控页，返回 placeholder 凭据");
            return Ok(StreamCredentials::placeholder());
        };

        let target = session
            .new_page(url)
            .await
            .map_err(|e| MultizenError::Mcp(format!("中控页建 tab 失败：{e}")))?;
        let result = self.scrape_credentials(&session, &target, url).await;
        // 临时 tab 尽力关闭，失败只记日志。
        if let Err(e) = session.close_page(&target).await {
            tracing::debug!(profile = %profile_id, error = %e, "live-launch: 中控临时 tab 关闭失败");
        }
        Ok(match result {
            Ok(creds) => creds,
            Err(e) => {
                tracing::warn!(profile = %profile_id, error = %e, "live-launch: 凭据抓取失败，降级 placeholder");
                StreamCredentials::placeholder()
            }
        })
    }

    async fn scrape_credentials(
        &self,
        session: &cdp_driver::session::BrowserSession,
        target: &str,
        url: &str,
    ) -> Result<StreamCredentials> {
        let cancel = TaskCancel::new();
        let mut task = session
            .task_page(target, cancel, LEASE_WAIT)
            .await
            .map_err(|e| MultizenError::Mcp(format!("中控页 lease 失败：{e}")))?;
        task.navigate(url, NAV_WAIT)
            .await
            .map_err(|e| MultizenError::Mcp(format!("中控页导航失败：{e}")))?;
        let value = task
            .evaluate(CREDENTIALS_EXTRACTOR_JS, EVAL_WAIT)
            .await
            .map_err(|e| MultizenError::Mcp(format!("凭据 JS 读取失败：{e}")))?;
        parse_credentials(&value)
            .ok_or_else(|| MultizenError::Mcp("中控页未读出 RTMP 凭据".into()))
    }

    /// 启动 heartbeat 占位推流（黑屏）。
    /// - 有真实凭据 → 黑屏保活推到 RTMP 房间；
    /// - placeholder 凭据 → 黑屏推往本地 null 槽位 warmup（`placeholder=true` 标明）；
    /// - ffmpeg 不可用 → 直接报错（跳过真实推流并报错，不静默）。
    /// 与正式推流互斥：已有活跃推流时拒绝。
    pub async fn live_launch_start_heartbeat(
        &self,
        app: &tauri::AppHandle,
        profile_id: &str,
        control_url: Option<&str>,
    ) -> std::result::Result<StreamingState, String> {
        let resource_dir = app.path().resource_dir().ok();
        let ffmpeg = check_prerequisites(resource_dir.as_deref())?;
        let creds = self
            .fetch_stream_credentials(profile_id, control_url)
            .await
            .map_err(|e| e.to_string())?;
        let (argv, warmup) = if creds.is_usable() {
            (placeholder_argv(&creds.target_url()), false)
        } else {
            (placeholder_warmup_argv(), true)
        };
        tracing::info!(
            profile = %profile_id,
            warmup,
            target = %creds.redacted_target(),
            "live-launch: heartbeat 启动"
        );
        self.spawn_push(
            Some(app),
            SubjectKind::Profile,
            profile_id,
            StreamMode::Heartbeat,
            &ffmpeg,
            argv,
            creds,
            None,
        )
        .await
    }

    /// 启动正式推流：本地视频 `-stream_loop -1 -c copy -f flv` 循环推到 RTMP。
    /// 要求真实凭据（placeholder 拒绝）+ 视频文件存在；与 heartbeat 互斥。
    pub async fn live_launch_start_stream(
        &self,
        app: &tauri::AppHandle,
        profile_id: &str,
        video_path: &str,
        control_url: Option<&str>,
    ) -> std::result::Result<StreamingState, String> {
        if video_path.trim().is_empty() {
            return Err("请先选择本地视频文件".into());
        }
        if !Path::new(video_path).is_file() {
            return Err(format!("本地视频文件不存在：{video_path}"));
        }
        let resource_dir = app.path().resource_dir().ok();
        let ffmpeg = check_prerequisites(resource_dir.as_deref())?;
        let creds = self
            .fetch_stream_credentials(profile_id, control_url)
            .await
            .map_err(|e| e.to_string())?;
        if !creds.is_usable() {
            return Err("未获取到有效推流凭据（placeholder）：请先登录并进入开播中控页后再开播".into());
        }
        tracing::info!(
            profile = %profile_id,
            target = %creds.redacted_target(),
            video = %video_path,
            "live-launch: 正式推流启动"
        );
        let argv = realtime_argv(video_path, &creds.target_url());
        self.spawn_push(
            Some(app),
            SubjectKind::Profile,
            profile_id,
            StreamMode::Realtime,
            &ffmpeg,
            argv,
            creds,
            None,
        )
        .await
    }

    async fn spawn_push(
        &self,
        app: Option<&tauri::AppHandle>,
        kind: SubjectKind,
        subject_id: &str,
        mode: StreamMode,
        ffmpeg: &Path,
        argv: Vec<String>,
        creds: StreamCredentials,
        live_stream_id: Option<&str>,
    ) -> std::result::Result<StreamingState, String> {
        let tail = try_reserve(kind, subject_id, mode, &creds, live_stream_id).await?;
        let mut cmd = tokio::process::Command::new(ffmpeg);
        cmd.args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            // 明确不依赖 drop：取消路径由 supervisor 显式 kill()。
            .kill_on_drop(false);
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let msg = format!("ffmpeg 启动失败（{}）：{e}", ffmpeg.display());
                tracing::error!(subject = %subject_id, error = %e, "live-launch: spawn 失败");
                fail_start(kind, subject_id, msg.clone(), app).await;
                return Err(msg);
            }
        };
        let pid = child.id();
        let stderr = child.stderr.take();
        if let Some(pid) = pid {
            set_pid(kind, subject_id, pid).await;
        }
        let (kill_tx, kill_rx) = mpsc::channel::<()>(1);
        set_kill_tx(kind, subject_id, kill_tx).await;
        tokio::spawn(supervise_push(
            app.cloned(),
            kind,
            subject_id.to_string(),
            child,
            stderr,
            tail,
            kill_rx,
        ));
        Ok(set_status(kind, subject_id, StreamingStatus::Streaming, app).await)
    }

    /// 停止当前推流（heartbeat/正式通用）：发 kill 信号，supervisor 显式
    /// `kill()` + `wait()` 后落终态；等待终态超时则报错。profile 桶。
    pub async fn live_launch_stop(
        &self,
        app: &tauri::AppHandle,
        profile_id: &str,
    ) -> std::result::Result<StreamingState, String> {
        stop_push(SubjectKind::Profile, profile_id, Some(app)).await
    }

    /// 启动伴侣（mate）正式推流：mate 取流（`liveMate` 三步）→ 本地视频
    /// `-stream_loop -1 -c copy -f flv` 循环推到 RTMP。与同账号 heartbeat/正式互斥。
    ///
    /// **全程不启浏览器**（伴侣账号无 profile），复用既有 ffmpeg 管理（不重写）。
    /// 取流得到的 `liveStreamId` 存进 mate 桶条目，供 [`Self::live_launch_stop_mate_stream`] 回收。
    pub async fn live_launch_start_mate_stream(
        &self,
        app: &tauri::AppHandle,
        mate_account_id: &str,
        video_path: &str,
    ) -> std::result::Result<StreamingState, String> {
        if video_path.trim().is_empty() {
            return Err("请先选择本地视频文件".into());
        }
        if !Path::new(video_path).is_file() {
            return Err(format!("本地视频文件不存在：{video_path}"));
        }
        let resource_dir = app.path().resource_dir().ok();
        let ffmpeg = check_prerequisites(resource_dir.as_deref())?;
        let creds = self
            .fetch_mate_stream_credentials(mate_account_id)
            .await
            .map_err(|e| e.to_string())?;
        if !creds.is_usable() {
            return Err("直播伴侣未返回有效推流地址".into());
        }
        tracing::info!(
            mate_account = %mate_account_id,
            target = %creds.redacted_target(),
            video = %video_path,
            live_stream_id = %creds.live_stream_id,
            "live-launch: 伴侣正式推流启动"
        );
        let argv = realtime_argv(video_path, &creds.target_url());
        self.spawn_push(
            Some(app),
            SubjectKind::Mate,
            mate_account_id,
            StreamMode::Realtime,
            &ffmpeg,
            argv,
            creds.clone(),
            Some(&creds.live_stream_id),
        )
        .await
    }

    /// 启动伴侣（mate）heartbeat 占位推流（黑屏）：同样先 mate 取流再推黑屏。
    pub async fn live_launch_start_mate_heartbeat(
        &self,
        app: &tauri::AppHandle,
        mate_account_id: &str,
    ) -> std::result::Result<StreamingState, String> {
        let resource_dir = app.path().resource_dir().ok();
        let ffmpeg = check_prerequisites(resource_dir.as_deref())?;
        let creds = self
            .fetch_mate_stream_credentials(mate_account_id)
            .await
            .map_err(|e| e.to_string())?;
        if !creds.is_usable() {
            return Err("直播伴侣未返回有效推流地址".into());
        }
        tracing::info!(
            mate_account = %mate_account_id,
            target = %creds.redacted_target(),
            live_stream_id = %creds.live_stream_id,
            "live-launch: 伴侣 heartbeat 启动"
        );
        let argv = placeholder_argv(&creds.target_url());
        self.spawn_push(
            Some(app),
            SubjectKind::Mate,
            mate_account_id,
            StreamMode::Heartbeat,
            &ffmpeg,
            argv,
            creds.clone(),
            Some(&creds.live_stream_id),
        )
        .await
    }

    /// 停止伴侣（mate）推流：**先** `live_mate::stop_live_mate_push(id, liveStreamId)`
    /// 关播，**再**停 ffmpeg 子进程（复用 [`stop_push`]）。
    ///
    /// `liveStreamId` 取自 mate 桶条目（启动时写入）；平台关播失败时**不阻塞**
    /// 本地 ffmpeg 停止（best-effort，只记日志），但会把原因回填到快照 `error`。
    pub async fn live_launch_stop_mate_stream(
        &self,
        app: &tauri::AppHandle,
        mate_account_id: &str,
    ) -> std::result::Result<StreamingState, String> {
        let live_stream_id = take_mate_live_stream_id(mate_account_id).await;
        let stop_note = match live_stream_id.as_deref() {
            Some(id) if !id.is_empty() => match self.live_mate_stop_push(mate_account_id, id).await {
                Ok(result) => {
                    tracing::info!(
                        mate_account = %mate_account_id,
                        live_stream_id = %id,
                        reason = %result.reason.unwrap_or_default(),
                        "live-launch: 伴侣已关播"
                    );
                    None
                }
                Err(e) => {
                    tracing::warn!(
                        mate_account = %mate_account_id,
                        live_stream_id = %id,
                        error = %e,
                        "live-launch: 伴侣关播失败（仍继续停 ffmpeg）"
                    );
                    Some(format!("平台关播失败：{e}"))
                }
            },
            _ => {
                tracing::debug!(mate_account = %mate_account_id, "live-launch: 无 liveStreamId，跳过关播");
                None
            }
        };
        let snapshot = stop_push(SubjectKind::Mate, mate_account_id, Some(app)).await?;
        if let Some(note) = stop_note {
            let mut bucket = REGISTRY.mates.write().await;
            if let Some(entry) = bucket.get_mut(mate_account_id) {
                entry.error = Some(note);
            }
        }
        Ok(snapshot)
    }

    /// 伴侣心跳（best-effort）：转发 `live_mate::send_live_mate_heartbeat`。
    ///
    /// jieger 的 `sendLiveMateHeartbeat` **无调用方**（无周期任务），故这里不内置
    /// 定时器：由调用方按需触发（开播期间周期调用由后续节点/上层调度负责）。
    pub async fn live_launch_mate_heartbeat(&self, mate_account_id: &str) -> Result<()> {
        self.live_mate_heartbeat(mate_account_id).await
    }

    /// 仅供测试：把 liveMate 五个端点指向本地 mock（离线跑取流链路）。
    #[cfg(test)]
    pub(crate) fn set_live_mate_endpoints(&mut self, endpoints: live_mate::LiveMateEndpoints) {
        self.live_mate = live_mate::LiveMateClient::with_endpoints(endpoints);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transition_matrix() {
        use StreamingStatus as S;
        let allowed = [
            (S::Idle, S::Starting),
            (S::Starting, S::Streaming),
            (S::Starting, S::Error),
            (S::Starting, S::Stopping),
            (S::Streaming, S::Stopping),
            (S::Stopping, S::Stopped),
            (S::Stopping, S::Error),
            (S::Stopped, S::Starting),
            (S::Error, S::Starting),
        ];
        for from in [S::Idle, S::Starting, S::Streaming, S::Stopping, S::Stopped, S::Error] {
            for to in [S::Idle, S::Starting, S::Streaming, S::Stopping, S::Stopped, S::Error] {
                let expect = allowed.contains(&(from, to));
                assert_eq!(
                    from.can_transition(to),
                    expect,
                    "transition {from:?} -> {to:?}"
                );
            }
        }
        assert!(S::Starting.is_active() && S::Streaming.is_active());
        assert!(!S::Idle.is_active() && !S::Stopped.is_active() && !S::Error.is_active());
    }

    #[test]
    fn placeholder_args_shape_black_screen_flv() {
        // 契约：黑屏 1fps 50kbps + aac 32k + flv，target 追加在末尾。
        assert!(PLACEHOLDER_ARGS.contains(&"color=c=black:s=1280x720:r=1"));
        let argv = placeholder_argv("rtmp://host/live/key");
        assert_eq!(argv.last().unwrap(), "rtmp://host/live/key");
        let joined = argv.join(" ");
        for needle in ["-r 1", "-b:v 50k", "aac", "-b:a 32k", "-f flv"] {
            assert!(joined.contains(needle), "缺少 {needle}: {joined}");
        }
        let warmup = placeholder_warmup_argv();
        assert_eq!(&warmup[warmup.len() - 3..], ["-f", "null", "-"]);
    }

    #[test]
    fn realtime_argv_uses_native_loop_and_copy() {
        let argv = realtime_argv("C:\\vid\\a.mp4", "rtmp://host/live/key");
        let joined = argv.join(" ");
        assert!(joined.contains("-stream_loop -1"), "{joined}");
        assert!(joined.contains("-c copy"), "{joined}");
        assert!(joined.contains("-f flv"), "{joined}");
        assert_eq!(argv.last().unwrap(), "rtmp://host/live/key");
        assert!(argv.windows(2).any(|w| w == ["-i", "C:\\vid\\a.mp4"]));
    }

    #[test]
    fn resolve_bundled_ffmpeg_path_splicing() {
        #[cfg(target_os = "windows")]
        let expect = "ffmpeg.exe";
        #[cfg(not(target_os = "windows"))]
        let expect = "ffmpeg";
        let dir = Path::new("/app/resources");
        let got = resolve_bundled_ffmpeg_path(dir);
        assert_eq!(got, dir.join("ffmpeg").join(expect));
        assert_eq!(ffmpeg_file_name(), expect);
    }

    #[test]
    fn find_ffmpeg_prefers_first_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        // 空目录 → None。
        assert!(find_ffmpeg(&[a.clone(), b.clone()]).is_none());
        // b 有可执行文件 → 找到 b。
        std::fs::write(b.join(ffmpeg_file_name()), "fake").unwrap();
        assert_eq!(
            find_ffmpeg(&[a.clone(), b.clone()]),
            Some(b.join(ffmpeg_file_name()))
        );
        // a 也有 → 优先 a。
        std::fs::write(a.join(ffmpeg_file_name()), "fake").unwrap();
        assert_eq!(
            find_ffmpeg(&[a.clone(), b.clone()]),
            Some(a.join(ffmpeg_file_name()))
        );
    }

    #[test]
    fn parse_credentials_accepts_rtmp_and_rejects_junk() {
        let good = serde_json::json!({
            "rtmpServer": "rtmp://push.example/live",
            "streamKey": "key123",
            "liveStreamId": "9",
        });
        let creds = parse_credentials(&good).unwrap();
        assert!(!creds.placeholder && creds.is_usable());
        assert_eq!(creds.target_url(), "rtmp://push.example/live/key123");
        assert_eq!(creds.redacted_target(), "rtmp://push.example/live/***");
        assert!(!creds.redacted_target().contains("key123"));
        for bad in [
            serde_json::json!({"rtmpServer": "", "streamKey": "k"}),
            serde_json::json!({"rtmpServer": "rtmp://h/l"}),
            serde_json::json!({"rtmpServer": "https://h/l", "streamKey": "k"}),
            serde_json::json!({}),
        ] {
            assert!(parse_credentials(&bad).is_none(), "{bad}");
        }
        let ph = StreamCredentials::placeholder();
        assert!(ph.placeholder && !ph.is_usable());
    }

    #[test]
    fn stderr_tail_keeps_last_20() {
        let mut tail = VecDeque::new();
        for i in 0..25 {
            push_stderr_line(&mut tail, format!("line{i}"));
        }
        assert_eq!(tail.len(), STDERR_TAIL_LINES);
        let kept: Vec<_> = tail.into_iter().collect();
        assert_eq!(kept[0], "line5");
        assert_eq!(kept[19], "line24");
    }

    /// mock ffmpeg：向 stderr 写 25 行并以 exit 3 退出。
    fn mock_stderr_exit_command() -> tokio::process::Command {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "(for /L %i in (1,1,25) do @echo line%i 1>&2) & exit /b 3"]);
            cmd
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args([
                "-c",
                "i=1; while [ $i -le 25 ]; do echo \"line$i\" >&2; i=$((i+1)); done; exit 3",
            ]);
            cmd
        }
    }

    #[tokio::test]
    async fn mock_ffmpeg_spawn_exit_and_stderr_tail() {
        let mut cmd = mock_stderr_exit_command();
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        let mut child = cmd.spawn().expect("mock 进程 spawn");
        let stderr = child.stderr.take();
        let tail: Arc<StdMutex<VecDeque<String>>> = Arc::new(StdMutex::new(VecDeque::new()));
        drain_stderr_to_tail(stderr, tail.clone()).await;
        let status = child.wait().await.expect("mock 进程 wait");
        assert_eq!(status.code(), Some(3));
        let kept: Vec<String> = tail.lock().unwrap().iter().cloned().collect();
        assert_eq!(kept.len(), STDERR_TAIL_LINES);
        assert_eq!(kept[0], "line6");
        assert_eq!(kept[19], "line25");
    }

    #[tokio::test]
    async fn mock_ffmpeg_explicit_kill_terminates() {
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = tokio::process::Command::new("cmd");
            c.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = tokio::process::Command::new("sh");
            c.args(["-c", "sleep 30"]);
            c
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        let mut child = cmd.spawn().expect("常驻 mock spawn");
        assert!(child.id().is_some());
        // 取消路径：显式 kill（不依赖 drop），再 wait 回收。
        child.kill().await.expect("显式 kill");
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("kill 后 wait 超时")
            .expect("wait");
        // 被 kill 的退出码平台各异（Windows None/unix signal），只断言已终止。
        assert!(!status.success());
    }

    // --- mate（伴侣）模式：离线 -------------------------------------------------
    //
    // 覆盖：取流（mock liveMate 端点）→ 得 RTMP → 启停状态流转；**不启真实
    // ffmpeg**（用 `cmd`/`sh` mock 常驻进程），不发任何真实网络请求。

    /// 极简 liveMate mock：authStatus/prePush/startPush/stopPush 回固定 JSON，
    /// 记录命中路径。
    async fn spawn_live_mate_mock(
        start: serde_json::Value,
        stop: serde_json::Value,
    ) -> (
        live_mate::LiveMateEndpoints,
        Arc<StdMutex<Vec<String>>>,
        tokio::task::JoinHandle<()>,
    ) {
        use axum::extract::{Request, State as AxumState};
        use axum::response::IntoResponse;
        use axum::{Json, Router};

        #[derive(Clone)]
        struct MockState {
            start: serde_json::Value,
            stop: serde_json::Value,
            paths: Arc<StdMutex<Vec<String>>>,
        }
        async fn handler(
            AxumState(state): AxumState<MockState>,
            req: Request,
        ) -> axum::response::Response {
            let path = req.uri().path().to_string();
            state.paths.lock().unwrap().push(path.clone());
            let reply = if path.ends_with("/startPush") {
                state.start.clone()
            } else if path.ends_with("/stopPush") {
                state.stop.clone()
            } else if path.ends_with("/prePush") {
                serde_json::json!({"result": 1, "liveStreamId": "LS-77", "prePushAttach": "ATT-88"})
            } else {
                serde_json::json!({"result": 1, "status": "available"})
            };
            Json(reply).into_response()
        }
        let paths = Arc::new(StdMutex::new(Vec::new()));
        let state = MockState {
            start,
            stop,
            paths: paths.clone(),
        };
        let router = Router::new().fallback(handler).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let endpoints = live_mate::LiveMateEndpoints {
            auth_status: format!("{base}/authStatus"),
            pre_push: format!("{base}/prePush"),
            start_push: format!("{base}/startPush"),
            stop_push: format!("{base}/stopPush"),
            dynamic_icon: format!("{base}/dynamicIcon"),
        };
        (endpoints, paths, handle)
    }

    /// 把 mate 账号写入登录凭证（token/st/uid），**不落日志**。
    async fn log_in_mate(driver: &TauriBrowserDriver, id: &str) {
        let id = id.to_string();
        driver
            .mate_store(move |pm| {
                pm.mate_account_record_login(
                    &id,
                    "1234567890",
                    "伴侣用户",
                    None,
                    &profile_manager::MateLoginTokens {
                        token: Some("tok-abc-1234567890".to_string()),
                        mate_st: Some("st-abcdef".to_string()),
                        ..Default::default()
                    },
                    0,
                )
            })
            .await
            .unwrap();
    }

    /// mock ffmpeg：常驻到被 kill（不解析参数，不推流）。
    #[cfg(target_os = "windows")]
    fn mock_ffmpeg() -> (&'static str, Vec<String>) {
        (
            "cmd",
            vec![
                "/C".to_string(),
                "ping -n 30 127.0.0.1 >NUL".to_string(),
            ],
        )
    }
    #[cfg(not(target_os = "windows"))]
    fn mock_ffmpeg() -> (&'static str, Vec<String>) {
        ("sh", vec!["-c".to_string(), "sleep 30".to_string()])
    }

    #[test]
    fn mate_stream_credentials_splits_rtmp_and_keeps_live_id() {
        let creds = mate_stream_credentials("rtmp://push.ksapisrv.com/live/abc123", "LS-9");
        assert!(!creds.placeholder);
        assert!(creds.is_usable());
        assert_eq!(creds.rtmp_server, "rtmp://push.ksapisrv.com/live");
        assert_eq!(creds.stream_key, "abc123");
        assert_eq!(creds.live_stream_id, "LS-9");
        assert_eq!(creds.target_url(), "rtmp://push.ksapisrv.com/live/abc123");
        assert!(!creds.redacted_target().contains("abc123"));
        // 无 key 的退化地址：server 整串，key 空 → 不可用。
        let degenerate = mate_stream_credentials("rtmp://", "LS");
        assert!(!degenerate.is_usable());
    }

    #[tokio::test]
    async fn buckets_are_isolated_for_same_key() {
        // 同一个字符串作为 profile key 与 mate key：互不互斥，各自独立。
        let creds = mate_stream_credentials("rtmp://h/live/k", "LS-iso");
        try_reserve(
            SubjectKind::Profile,
            "same-id",
            StreamMode::Realtime,
            &creds,
            None,
        )
        .await
        .expect("profile 桶预留");
        try_reserve(
            SubjectKind::Mate,
            "same-id",
            StreamMode::Realtime,
            &creds,
            Some("LS-iso"),
        )
        .await
        .expect("mate 桶预留（不因 profile 桶同名而互斥）");

        let profile_view = snapshot_of(SubjectKind::Profile, "same-id").await;
        let mate_view = snapshot_of(SubjectKind::Mate, "same-id").await;
        assert_eq!(profile_view.subject_kind, SubjectKind::Profile);
        assert_eq!(mate_view.subject_kind, SubjectKind::Mate);
        assert!(profile_view.live_stream_id.is_none());
        assert_eq!(mate_view.live_stream_id.as_deref(), Some("LS-iso"));
    }

    #[tokio::test]
    async fn mate_fetch_credentials_returns_rtmp_and_reports_missing_login() {
        let (_dir, mut driver) =
            crate::driver::business_tests::fixture(multizen_core::ChromixSettings::default());
        let (endpoints, paths, server) = spawn_live_mate_mock(
            serde_json::json!({"result": 1, "liveStreamId": "LS-77", "pushRtmpUrl": "rtmp://push.ksapisrv.com/live/abc123"}),
            serde_json::json!({"result": 1}),
        )
        .await;
        driver.set_live_mate_endpoints(endpoints);

        let account = driver.mate_account_add(Some("伴侣取流")).await.unwrap();
        // 未登录 → 凭证不完整（不发任何请求）。
        let err = driver
            .fetch_mate_stream_credentials(&account.id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("未登录或凭证不完整"), "{err}");
        assert!(paths.lock().unwrap().is_empty());

        // 登录后取流：mock liveMate 三步 → RTMP 拆分 + liveStreamId。
        log_in_mate(&driver, &account.id).await;
        let creds = driver
            .fetch_mate_stream_credentials(&account.id)
            .await
            .unwrap();
        assert!(!creds.placeholder);
        assert!(creds.is_usable());
        assert_eq!(creds.rtmp_server, "rtmp://push.ksapisrv.com/live");
        assert_eq!(creds.stream_key, "abc123");
        assert_eq!(creds.live_stream_id, "LS-77");
        let hit = paths.lock().unwrap().clone();
        assert!(hit.iter().any(|p| p.ends_with("/authStatus")), "{hit:?}");
        assert!(hit.iter().any(|p| p.ends_with("/prePush")), "{hit:?}");
        assert!(hit.iter().any(|p| p.ends_with("/startPush")), "{hit:?}");

        server.abort();
        driver.shutdown().await;
    }

    #[tokio::test]
    async fn mate_fetch_credentials_unknown_account_is_not_found() {
        let (_dir, driver) =
            crate::driver::business_tests::fixture(multizen_core::ChromixSettings::default());
        let err = driver
            .fetch_mate_stream_credentials("absent")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("不存在"), "{err}");
        driver.shutdown().await;
    }

    #[tokio::test]
    async fn mate_stream_start_and_stop_state_flow() {
        let (_dir, mut driver) =
            crate::driver::business_tests::fixture(multizen_core::ChromixSettings::default());
        let account = driver.mate_account_add(Some("伴侣启停")).await.unwrap();
        log_in_mate(&driver, &account.id).await;
        let (endpoints, _paths, server) = spawn_live_mate_mock(
            serde_json::json!({"result": 1, "liveStreamId": "LS-77", "pushRtmpUrl": "rtmp://push.ksapisrv.com/live/abc123"}),
            serde_json::json!({"result": 1}),
        )
        .await;
        driver.set_live_mate_endpoints(endpoints);

        // 取流（mock liveMate）→ 得 RTMP。
        let creds = driver
            .fetch_mate_stream_credentials(&account.id)
            .await
            .unwrap();
        assert_eq!(creds.live_stream_id, "LS-77");

        // 启推流：用 mock 命令驱动**真实**状态机（不启真实 ffmpeg）。
        let (name, args) = mock_ffmpeg();
        let started = driver
            .spawn_push(
                None,
                SubjectKind::Mate,
                &account.id,
                StreamMode::Realtime,
                Path::new(name),
                args,
                creds.clone(),
                Some(&creds.live_stream_id),
            )
            .await
            .expect("mate 推流启动");
        assert_eq!(started.status, StreamingStatus::Streaming);
        assert_eq!(started.subject_kind, SubjectKind::Mate);
        assert_eq!(started.mode, Some(StreamMode::Realtime));
        assert_eq!(started.live_stream_id.as_deref(), Some("LS-77"));
        assert_eq!(
            started.target.as_deref(),
            Some("rtmp://push.ksapisrv.com/live/***")
        );
        assert!(!started.placeholder_credentials);

        // 桶隔离：mate 桶在推流，profile 桶同名 key 仍 Idle。
        let profile_view = driver.live_launch_status(&account.id).await;
        assert_eq!(profile_view.status, StreamingStatus::Idle);
        assert_eq!(profile_view.subject_kind, SubjectKind::Profile);

        // 停止：走 stop_push（与 live_launch_stop_mate_stream 内部同一路径）。
        // stop_push 在状态离开 active（→ Stopping）即返回；supervisor 随后落 Stopped。
        let stopped = stop_push(SubjectKind::Mate, &account.id, None)
            .await
            .expect("mate 推流停止");
        assert!(!stopped.status.is_active(), "{:?}", stopped.status);
        assert!(matches!(
            stopped.status,
            StreamingStatus::Stopping | StreamingStatus::Stopped
        ));
        assert_eq!(stopped.live_stream_id.as_deref(), Some("LS-77"));

        // 等 supervisor 收尾到 Stopped（mock 进程被显式 kill）。
        let mut settled = stopped;
        for _ in 0..100 {
            settled = snapshot_of(SubjectKind::Mate, &account.id).await;
            if settled.status == StreamingStatus::Stopped {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(settled.status, StreamingStatus::Stopped);

        // liveStreamId 回收：take 一次即清空（关播只做一次）。
        let taken = take_mate_live_stream_id(&account.id).await;
        assert_eq!(taken.as_deref(), Some("LS-77"));
        assert!(take_mate_live_stream_id(&account.id).await.is_none());

        server.abort();
        driver.shutdown().await;
    }
}
