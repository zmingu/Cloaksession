//! jieger 直播间监控（调度中枢）。
//!
//! # 范围
//!
//! - 本模块只做一件事：周期性确认指定直播间是否开播，开播瞬间联动触发
//!   下游任务（scene-play / shop-product-script / sub-account），下播时按
//!   配置批量退出。不实现任何下游任务本身，只调用本模块定义的触发接口。
//! - 开播检测走浏览器：用监控 profile 的会话开临时 tab 导航到
//!   `live_room_url`，settle 后提取 [`LiveRoomStatusSnapshot`] 做关键词分类。
//! - 下游触发失败只记录到 `last_*_result`，不中断监控循环。
//! - 关键词表是 jieger 经验值，真号校准时更新（见各 `*_KEYWORDS` 注释）。
//!
//! # 状态机
//!
//! - `status`（推送给前端）：Idle → Checking → Offline / Live → Triggering →
//!   Triggered；检测失败 → Error；`INVALID` 房间 → Error 并停机（fatal）。
//! - `triggered_for_current_live` 保证同一场直播只触发一次；Live → Offline
//!   时复位，下次开播可再次触发。
//! - 轮询快慢自动切换：开播中 / 已触发用 [`FAST_POLL_MS`]，离线用
//!   [`SLOW_POLL_MS`]。

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use cdp_driver::TaskCancel;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

use super::TauriBrowserDriver;

/// 状态变化推送事件名。前端 `listen("live-room-monitor-state-changed")`。
pub const LIVE_ROOM_MONITOR_STATE_CHANGED: &str = "live-room-monitor-state-changed";

/// 开播中 / 触发后快轮询间隔（临近开播不错过触发点）。
pub const FAST_POLL_MS: u64 = 5_000;
/// 离线慢轮询间隔。
pub const SLOW_POLL_MS: u64 = 60_000;
/// 单次检测导航超时。
pub const CHECK_NAVIGATION_TIMEOUT: Duration = Duration::from_secs(20);
/// 导航后 settle 等待（等播放器/文案渲染）。
pub const CHECK_SETTLE: Duration = Duration::from_millis(800);
/// TaskPage 租约等待 / JS 读取等待。
const LEASE_WAIT: Duration = Duration::from_secs(10);
const EVAL_WAIT: Duration = Duration::from_secs(10);

/// 监控对外状态（推送前端）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiveRoomMonitorStatus {
    Idle,
    Checking,
    Offline,
    Live,
    Triggering,
    Triggered,
    Error,
}

/// 直播间本身的开播状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiveRoomLiveStatus {
    Unknown,
    Offline,
    Live,
}

/// 页面关键词分类结果（内部流转，不直接推送）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomPageClass {
    /// 房间不存在/已删除等——fatal，停机。
    Invalid,
    /// 暂未开播/已下播等。
    Offline,
    /// 直播中（关键词 + 播放器/观看人数佐证）。
    Live,
    /// 访问受限（错误代码 22 等）——非 fatal，降级慢轮询，保持原状态。
    Limited,
    /// 无明确信号——保持原状态，慢轮询。
    Unknown,
}

/// 房间不存在类关键词（命中即 fatal）。
/// jieger 经验值，真号校准时更新。
pub const INVALID_ROOM_KEYWORDS: &[&str] = &[
    "404",
    "页面不存在",
    "房间不存在",
    "直播间不存在",
    "主播不存在",
    "账号不存在",
    "用户不存在",
    "已被删除",
    "参数错误",
    "page not found",
];

/// 未开播类关键词。
/// jieger 经验值，真号校准时更新。
pub const OFFLINE_ROOM_KEYWORDS: &[&str] = &[
    "暂未开播",
    "还没开播",
    "尚未开播",
    "还未开播",
    "未开播",
    "已下播",
    "下播",
    "直播已结束",
    "本场直播已结束",
    "主播休息",
    "休息中",
    "不在直播",
    "未在直播",
    "直播结束",
    "开播提醒",
    "预约直播",
    "明天再来",
    "敬请期待",
    "精彩回放",
    "回放",
];

/// 访问受限类关键词（命中即降级慢轮询，不改触发状态）。
/// jieger 经验值，真号校准时更新。
pub const ACCESS_LIMITED_KEYWORDS: &[&str] = &[
    "错误代码22",
    "错误码22",
    "error code 22",
    "error22",
    "访问受限",
    "操作太频繁",
    "请求过于频繁",
    "系统繁忙",
    "网络异常",
    "网络出错",
    "加载失败",
    "请稍后重试",
    "稍后再试",
    "验证码",
    "滑块验证",
    "安全验证",
];

/// 开播类关键词（需播放器/观看人数佐证才判 Live，见 [`classify_snapshot`]）。
/// jieger 经验值，真号校准时更新。
pub const LIVE_ROOM_KEYWORDS: &[&str] = &[
    "直播中",
    "正在直播",
    "直播进行中",
    "人气",
    "在线人数",
    "观看人数",
    "正在热播",
];

/// 观看人数标记（前面带数字才算，如 `1.2万人在看`）。
/// regex 会新增依赖，这里用子串 + 数字前瞻实现，等价可校准。
pub const VIEWER_COUNT_MARKERS: &[&str] = &[
    "人观看",
    "人在看",
    "人看过",
    "观看人数",
    "在线人数",
    "在线观看",
    "围观",
];

/// 单次检测提取的页面快照。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveRoomStatusSnapshot {
    pub title: String,
    pub href: String,
    pub text: String,
    pub input_visible: bool,
    pub media_visible: bool,
}

/// 监控启动配置（IPC 输入，camelCase）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorConfig {
    pub live_room_url: String,
    pub scene_id: Option<i64>,
    pub group_id: Option<String>,
    pub product_script_id: Option<i64>,
    pub product_script_account_id: Option<String>,
    #[serde(default)]
    pub auto_exit_sub_accounts: bool,
}

/// 监控完整状态（IPC 输出 / 推送 payload，camelCase）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRoomMonitorState {
    pub enabled: bool,
    /// 执行检测用的浏览器 profile（开临时 tab，不污染用户页面）。
    pub profile_id: Option<String>,
    pub live_room_url: Option<String>,
    pub scene_id: Option<i64>,
    pub group_id: Option<String>,
    pub product_script_id: Option<i64>,
    pub product_script_account_id: Option<String>,
    pub auto_exit_sub_accounts: bool,
    pub status: LiveRoomMonitorStatus,
    pub live_status: LiveRoomLiveStatus,
    pub triggered_for_current_live: bool,
    pub entering_rooms: bool,
    pub exiting_rooms: bool,
    pub last_checked_at: Option<i64>,
    pub next_check_at: Option<i64>,
    pub last_triggered_at: Option<i64>,
    pub last_enter_all_result: Option<EnterAllResult>,
    pub last_exit_all_result: Option<ExitAllResult>,
    pub last_product_script_result: Option<ProductScriptResult>,
    pub error: Option<String>,
}

impl Default for LiveRoomMonitorState {
    fn default() -> Self {
        Self {
            enabled: false,
            profile_id: None,
            live_room_url: None,
            scene_id: None,
            group_id: None,
            product_script_id: None,
            product_script_account_id: None,
            auto_exit_sub_accounts: false,
            status: LiveRoomMonitorStatus::Idle,
            live_status: LiveRoomLiveStatus::Unknown,
            triggered_for_current_live: false,
            entering_rooms: false,
            exiting_rooms: false,
            last_checked_at: None,
            next_check_at: None,
            last_triggered_at: None,
            last_enter_all_result: None,
            last_exit_all_result: None,
            last_product_script_result: None,
            error: None,
        }
    }
}

/// 下游批量触发结果（进入/退出/货盘脚本共用形状，具名别名见下）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerBatchResult {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub message: Option<String>,
    pub error: Option<String>,
    pub at: String,
}

impl TriggerBatchResult {
    fn now() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// 下游未接入 / 被配置跳过时的占位记录（不算失败）。
    pub fn skipped(message: impl Into<String>) -> Self {
        Self {
            attempted: 0,
            succeeded: 0,
            failed: 0,
            message: Some(message.into()),
            error: None,
            at: Self::now(),
        }
    }

    pub fn ok(attempted: usize, succeeded: usize, message: impl Into<String>) -> Self {
        Self {
            failed: attempted.saturating_sub(succeeded),
            attempted,
            succeeded,
            message: Some(message.into()),
            error: None,
            at: Self::now(),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            attempted: 0,
            succeeded: 0,
            failed: 0,
            message: None,
            error: Some(error.into()),
            at: Self::now(),
        }
    }
}

/// 批量进入直播间结果（sub-account `enter_live_room` 聚合）。
pub type EnterAllResult = TriggerBatchResult;
/// 批量退出/关闭会话结果（sub-account `exit_live_room`/`close_session` 聚合）。
pub type ExitAllResult = TriggerBatchResult;
/// 货盘脚本触发结果（shop-product-script `play_product_script`）。
pub type ProductScriptResult = TriggerBatchResult;

/// 开播触发上下文（下游任务按需取用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerContext {
    pub scene_id: Option<i64>,
    pub group_id: Option<String>,
    pub product_script_id: Option<i64>,
    pub product_script_account_id: Option<String>,
}

impl TriggerContext {
    fn from_state(state: &LiveRoomMonitorState) -> Self {
        Self {
            scene_id: state.scene_id,
            group_id: state.group_id.clone(),
            product_script_id: state.product_script_id,
            product_script_account_id: state.product_script_account_id.clone(),
        }
    }
}

/// 开播联动结果（进入 + 货盘脚本各自记录，互不掩盖）。
pub struct LiveStartOutcome {
    pub enter: EnterAllResult,
    pub product_script: ProductScriptResult,
}

/// 下播联动结果。
pub struct LiveEndOutcome {
    pub exit: ExitAllResult,
}

/// 下游触发接口契约：本任务定义，scene-play / shop-product-script /
/// sub-account 各自实现后经 [`set_live_room_trigger`] 注入。
/// 返回值是聚合记录（不是 `Result`），失败只记入 `last_*_result`，
/// 绝不中断监控循环。
#[async_trait]
pub trait LiveRoomTrigger: Send + Sync + 'static {
    async fn on_live_start(&self, ctx: TriggerContext) -> LiveStartOutcome;
    async fn on_live_end(&self, ctx: TriggerContext, auto_exit: bool) -> LiveEndOutcome;
}

/// 默认触发器：下游任务尚未接入时占位（记录跳过，不报错）。
struct NoopTrigger;

#[async_trait]
impl LiveRoomTrigger for NoopTrigger {
    async fn on_live_start(&self, _ctx: TriggerContext) -> LiveStartOutcome {
        LiveStartOutcome {
            enter: TriggerBatchResult::skipped(
                "下游任务（scene-play/shop-product-script/sub-account）尚未接入，已跳过进入",
            ),
            product_script: TriggerBatchResult::skipped("货盘脚本任务尚未接入，已跳过"),
        }
    }

    async fn on_live_end(&self, _ctx: TriggerContext, auto_exit: bool) -> LiveEndOutcome {
        LiveEndOutcome {
            exit: TriggerBatchResult::skipped(if auto_exit {
                "下游 sub-account 任务尚未接入，已跳过批量退出"
            } else {
                "autoExitSubAccounts 关闭，已跳过批量退出"
            }),
        }
    }
}

static TRIGGER: LazyLock<RwLock<Arc<dyn LiveRoomTrigger>>> =
    LazyLock::new(|| RwLock::new(Arc::new(NoopTrigger)));

/// 下游任务接入时调用：替换全局触发实现。
/// 当前无调用方（scene-play / shop-product-script / sub-account 落地后注入），
/// 保留为本任务定义的触发契约入口。
#[allow(dead_code)]
pub async fn set_live_room_trigger(trigger: Arc<dyn LiveRoomTrigger>) {
    *TRIGGER.write().await = trigger;
}

/// 过渡判定：`detected == Live && !triggered_for_current_live` 才触发。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorTransition {
    pub detected: LiveRoomLiveStatus,
    pub triggered_for_current_live: bool,
}

impl MonitorTransition {
    pub fn should_trigger(self) -> bool {
        self.detected == LiveRoomLiveStatus::Live && !self.triggered_for_current_live
    }

    /// 同一场直播是否刚结束（Live → Offline），用于复位触发位 + 下播联动。
    /// 中间经过 Limited/Unknown 不算结束，避免受限抖动造成重复触发。
    pub fn live_ended(previous: LiveRoomLiveStatus, detected: LiveRoomLiveStatus) -> bool {
        previous == LiveRoomLiveStatus::Live && detected == LiveRoomLiveStatus::Offline
    }
}

/// 快慢轮询：开播中 / 已触发用快轮询，其余慢轮询。
pub fn next_poll_ms(live: LiveRoomLiveStatus, triggered_for_current_live: bool) -> u64 {
    if live == LiveRoomLiveStatus::Live || triggered_for_current_live {
        FAST_POLL_MS
    } else {
        SLOW_POLL_MS
    }
}

/// 直播间链接合法性（轻量）：非空 + http(s) 开头。
/// 平台归属（快手等）由 ks-platform-primitives 侧校验，本模块不重复实现。
pub fn is_live_room_url(url: &str) -> bool {
    let url = url.trim();
    !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://"))
}

/// 文本中是否有“数字 + 观看人数标记”（如 `1.2万人在看` / `在线人数 128`）。
/// 标记前后都允许出现数字（`在线人数 128` 数字在后）。
pub fn has_viewer_count(text: &str) -> bool {
    for marker in VIEWER_COUNT_MARKERS {
        let mut start = 0;
        while let Some(pos) = text[start..].find(marker) {
            let abs = start + pos;
            // 标记往前 12 个字符内有数字（如 `1.2万`），或往后 6 个字符内
            // 有数字（如 `在线人数 128`），即算。
            let before: String = text[..abs].chars().rev().take(12).collect();
            let after: String = text[abs + marker.len()..].chars().take(6).collect();
            if before.chars().any(|c| c.is_ascii_digit())
                || after.chars().any(|c| c.is_ascii_digit())
            {
                return true;
            }
            start = abs + marker.len();
        }
    }
    false
}

/// 快照 → 页面分类。顺序：Invalid（fatal 优先）> Live（需佐证）> Offline >
/// Limited > Unknown。大小写不敏感（中文不受影响）。
pub fn classify_snapshot(snap: &LiveRoomStatusSnapshot) -> RoomPageClass {
    let hay = format!("{}\n{}", snap.title, snap.text).to_lowercase();
    let contains_any = |kws: &[&str]| kws.iter().any(|k| hay.contains(&k.to_lowercase()));
    if contains_any(INVALID_ROOM_KEYWORDS) {
        return RoomPageClass::Invalid;
    }
    let live_keyword = contains_any(LIVE_ROOM_KEYWORDS);
    let viewers = has_viewer_count(&snap.text);
    // 开播关键词需要播放器/观看人数佐证；两者皆有时关键词可缺省
    //（纯英文/符号标题的直播间）。
    if (live_keyword && (snap.media_visible || viewers)) || (snap.media_visible && viewers) {
        return RoomPageClass::Live;
    }
    if contains_any(OFFLINE_ROOM_KEYWORDS) {
        return RoomPageClass::Offline;
    }
    if contains_any(ACCESS_LIMITED_KEYWORDS) {
        return RoomPageClass::Limited;
    }
    RoomPageClass::Unknown
}

// 页面快照提取 JS：标题 + href + 正文（截断 8000 字）+ 输入框/播放器存在性。
const SNAPSHOT_EXTRACTOR_JS: &str = r#"(() => {
  const text = (document.body && (document.body.innerText || "")) || "";
  const has = (sel) => {
    try { return !!document.querySelector(sel); } catch (e) { return false; }
  };
  return {
    title: document.title || "",
    href: location.href || "",
    text: text.slice(0, 8000),
    inputVisible: has("input, textarea, [contenteditable]"),
    mediaVisible: has("video, .live-player, .player-video")
  };
})()"#;

/// 单个 profile 的监控槽：状态 / 取消句柄 / 检测互斥各自独立。
/// `checking` 必须**每槽独立**，否则 A 槽检测会阻塞 B 槽的检测。
/// `checking` 用 `Arc` 包裹，便于循环在不持有槽读锁的情况下克隆后在锁外 await。
pub struct MonitorSlot {
    state: LiveRoomMonitorState,
    cancel: Option<TaskCancel>,
    /// `checking` 互斥：上轮检测未完成时跳过本轮（同 jieger）。
    checking: Arc<Mutex<()>>,
}

impl MonitorSlot {
    fn new(state: LiveRoomMonitorState, cancel: Option<TaskCancel>) -> Self {
        Self {
            state,
            cancel,
            checking: Arc::new(Mutex::new(())),
        }
    }
}

/// 监控运行时：按 profile 分槽，支持多账号同时监控。
struct MonitorRuntime {
    slots: RwLock<HashMap<String, MonitorSlot>>,
    /// 单次检测桩：默认走真实浏览器检测；测试注入离线桩，避免真实网络。
    probe: RwLock<Option<MonitorProbe>>,
}

impl MonitorRuntime {
    fn new() -> Self {
        Self {
            slots: RwLock::new(HashMap::new()),
            probe: RwLock::new(None),
        }
    }

    /// 所有槽快照（仅测试用；按 profileId 稳定排序，便于断言）。
    #[cfg(test)]
    async fn list(&self) -> Vec<LiveRoomMonitorState> {
        let slots = self.slots.read().await;
        let mut states: Vec<LiveRoomMonitorState> =
            slots.values().map(|slot| slot.state.clone()).collect();
        states.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        states
    }

    /// 单槽快照（仅测试用）；未知 profile → `default()`（`profile_id: None`，
    /// 行为与改动前的“未启动”一致）。
    #[cfg(test)]
    async fn state(&self, profile_id: &str) -> LiveRoomMonitorState {
        self.slots
            .read()
            .await
            .get(profile_id)
            .map(|slot| slot.state.clone())
            .unwrap_or_default()
    }
}

static MONITOR: LazyLock<MonitorRuntime> = LazyLock::new(MonitorRuntime::new);

/// 状态推送出口：生产环境推 Tauri 事件；测试可注入内存 sink。
pub type StateSink = Arc<dyn Fn(&LiveRoomMonitorState) + Send + Sync>;

/// 默认出口：emit `LIVE_ROOM_MONITOR_STATE_CHANGED`（payload 含 `profileId`，
/// 前端据此分发到对应账号）。
fn default_sink<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> StateSink {
    let app = app.clone();
    Arc::new(move |state: &LiveRoomMonitorState| {
        if let Err(e) = app.emit(LIVE_ROOM_MONITOR_STATE_CHANGED, state) {
            tracing::warn!(error = %e, "live-room-monitor: 状态推送失败");
        }
    })
}

/// 经出口推送一次状态。
fn emit_state(sink: &StateSink, state: &LiveRoomMonitorState) {
    sink(state);
}

/// 单次检测桩的返回 future（`Box<dyn Future>` 便于注入不同实现）。
pub type ProbeFuture =
    Pin<Box<dyn std::future::Future<Output = Result<RoomPageClass, String>> + Send + 'static>>;

/// 单次检测桩：给定 `(profileId, url)` 返回页面分类。
/// 测试注入离线桩，避免真实网络；生产不注入，走
/// [`TauriBrowserDriver::check_once`] 真实浏览器检测。
pub type MonitorProbe = Arc<dyn Fn(&str, &str) -> ProbeFuture + Send + Sync>;

/// 下游任务/测试接入时调用：注入 / 清除单次检测桩（`None` 恢复真实检测）。
#[allow(dead_code)]
pub async fn set_monitor_probe(probe: Option<MonitorProbe>) {
    *MONITOR.probe.write().await = probe;
}

/// 同步读单槽快照（给同步 IPC 命令用；低频调用，可接受阻塞读）。
/// 未知 profile → 默认快照（`profileId: null`，与“未启动”一致）。
pub fn current_state_blocking(profile_id: &str) -> LiveRoomMonitorState {
    MONITOR
        .slots
        .blocking_read()
        .get(profile_id)
        .map(|slot| slot.state.clone())
        .unwrap_or_default()
}

/// 同步读所有槽快照（给同步 IPC 命令用；供前端一次拉全）。
pub fn list_states_blocking() -> Vec<LiveRoomMonitorState> {
    let slots = MONITOR.slots.blocking_read();
    let mut states: Vec<LiveRoomMonitorState> =
        slots.values().map(|slot| slot.state.clone()).collect();
    states.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
    states
}

/// 把一次检测结论合入状态（含联动触发）。
/// `emit` 在 Triggering/entering 等中间态同步推送，调用方收尾再推一次。
/// 下游触发失败只记入 `last_*_result`，状态仍按触发成功流转（不卡循环）。
pub async fn apply_live_status(
    trigger: &dyn LiveRoomTrigger,
    state: &mut LiveRoomMonitorState,
    detected: LiveRoomLiveStatus,
    now_ms: i64,
    emit: impl Fn(&LiveRoomMonitorState),
) {
    match detected {
        LiveRoomLiveStatus::Live => {
            state.live_status = LiveRoomLiveStatus::Live;
            let transition = MonitorTransition {
                detected: LiveRoomLiveStatus::Live,
                triggered_for_current_live: state.triggered_for_current_live,
            };
            if transition.should_trigger() {
                state.status = LiveRoomMonitorStatus::Triggering;
                state.entering_rooms = true;
                emit(state);
                let outcome = trigger.on_live_start(TriggerContext::from_state(state)).await;
                state.last_enter_all_result = Some(outcome.enter);
                state.last_product_script_result = Some(outcome.product_script);
                state.entering_rooms = false;
                state.triggered_for_current_live = true;
                state.last_triggered_at = Some(now_ms);
                state.error = None;
                state.status = LiveRoomMonitorStatus::Triggered;
            } else {
                state.status = LiveRoomMonitorStatus::Triggered;
            }
        }
        LiveRoomLiveStatus::Offline => {
            // 同一场直播内即使中间经过 Limited/Unknown（live_status 非 Live
            // 但触发位还在），下播仍要联动 + 复位，避免下次开播无法触发。
            let previous = state.live_status;
            let in_live_session = MonitorTransition::live_ended(previous, LiveRoomLiveStatus::Offline)
                || state.triggered_for_current_live;
            state.live_status = LiveRoomLiveStatus::Offline;
            if in_live_session {
                state.status = LiveRoomMonitorStatus::Offline;
                state.exiting_rooms = true;
                emit(state);
                let outcome = trigger
                    .on_live_end(
                        TriggerContext::from_state(state),
                        state.auto_exit_sub_accounts,
                    )
                    .await;
                state.last_exit_all_result = Some(outcome.exit);
                state.exiting_rooms = false;
                state.triggered_for_current_live = false;
                state.error = None;
            }
            state.status = LiveRoomMonitorStatus::Offline;
        }
        LiveRoomLiveStatus::Unknown => {
            // 无明确信号：保持原状态/触发位，只刷新时间戳（慢轮询）。
        }
    }
    state.last_checked_at = Some(now_ms);
    state.next_check_at = Some(
        now_ms + next_poll_ms(state.live_status, state.triggered_for_current_live) as i64,
    );
}

impl TauriBrowserDriver {
    /// 指定 profile 的监控快照（未启动过 → Idle 默认）。
    pub async fn live_room_monitor_state(&self, profile_id: &str) -> LiveRoomMonitorState {
        MONITOR
            .slots
            .read()
            .await
            .get(profile_id)
            .map(|slot| slot.state.clone())
            .unwrap_or_default()
    }

    /// 启动监控：校验配置 → 落该 profile 的槽 → 起 tokio 循环。
    /// 同一 profile 重复启动是**幂等替换**：先取消旧循环再起新的，
    /// 不影响其他 profile 的槽（支持多账号同时监控）。
    pub async fn start_live_room_monitor<R: tauri::Runtime>(
        self: &Arc<Self>,
        app: &tauri::AppHandle<R>,
        profile_id: &str,
        config: MonitorConfig,
    ) -> std::result::Result<LiveRoomMonitorState, String> {
        if !is_live_room_url(&config.live_room_url) {
            return Err("直播间链接无效，须以 http(s):// 开头".into());
        }
        if profile_id.trim().is_empty() {
            return Err("请先选择执行检测的浏览器环境".into());
        }
        let cancel = TaskCancel::new();
        let state = LiveRoomMonitorState {
            enabled: true,
            profile_id: Some(profile_id.to_string()),
            live_room_url: Some(config.live_room_url.clone()),
            scene_id: config.scene_id,
            group_id: config.group_id.clone(),
            product_script_id: config.product_script_id,
            product_script_account_id: config.product_script_account_id.clone(),
            auto_exit_sub_accounts: config.auto_exit_sub_accounts,
            status: LiveRoomMonitorStatus::Checking,
            ..LiveRoomMonitorState::default()
        };
        let sink = default_sink(app);
        // 快照检测桩（测试可注入离线桩；生产为 None → 走真实检测）。
        let probe = MONITOR.probe.read().await.clone();
        // 落槽：同 profile 先取消旧任务再替换（幂等）；其他槽不动。
        let snapshot = {
            let mut slots = MONITOR.slots.write().await;
            if let Some(old) = slots.get(profile_id).and_then(|slot| slot.cancel.as_ref()) {
                old.cancel();
            }
            slots.insert(
                profile_id.to_string(),
                MonitorSlot::new(state.clone(), Some(cancel.clone())),
            );
            state
        };
        emit_state(&sink, &snapshot);
        tracing::info!(
            profile = %profile_id,
            url = %config.live_room_url,
            "live-room-monitor: 启动"
        );
        tauri::async_runtime::spawn(monitor_loop(
            Arc::clone(self),
            profile_id.to_string(),
            cancel,
            sink,
            probe,
        ));
        Ok(snapshot)
    }

    /// 停止指定 profile 的监控：取消循环 → 从槽中移除（幂等）。
    /// 只影响该 profile，其他槽继续运行；未启动的 profile 返回默认快照。
    pub async fn stop_live_room_monitor<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        profile_id: &str,
    ) -> std::result::Result<LiveRoomMonitorState, String> {
        let mut slots = MONITOR.slots.write().await;
        let removed = match slots.remove(profile_id) {
            Some(slot) => {
                if let Some(cancel) = slot.cancel.as_ref() {
                    cancel.cancel();
                }
                slot.state
            }
            // 幂等：未启动过该 profile → 默认快照（与改动前一致）。
            None => LiveRoomMonitorState::default(),
        };
        drop(slots);
        let sink = default_sink(app);
        emit_state(&sink, &removed);
        tracing::info!(profile = %profile_id, "live-room-monitor: 已停止");
        Ok(removed)
    }

    /// 单次检测：临时 tab 导航 + settle + 快照提取 + 分类。
    /// 临时 tab 无论成败尽力关闭，不污染用户页面（同 live-launch 模式）。
    async fn check_once(
        &self,
        profile_id: &str,
        url: &str,
    ) -> std::result::Result<RoomPageClass, String> {
        let session = self
            .require_session(profile_id)
            .await
            .map_err(|e| format!("监控环境未运行，请先启动浏览器：{e}"))?;
        let target = session
            .new_page(url)
            .await
            .map_err(|e| format!("监控临时 tab 创建失败：{e}"))?;
        let outcome = self.probe_target(&session, &target, url).await;
        if let Err(e) = session.close_page(&target).await {
            tracing::debug!(profile = %profile_id, error = %e, "live-room-monitor: 临时 tab 关闭失败");
        }
        outcome
    }

    async fn probe_target(
        &self,
        session: &cdp_driver::session::BrowserSession,
        target: &str,
        url: &str,
    ) -> std::result::Result<RoomPageClass, String> {
        let cancel = TaskCancel::new();
        let mut task = session
            .task_page(target, cancel, LEASE_WAIT)
            .await
            .map_err(|e| format!("监控页面租约失败：{e}"))?;
        task.navigate(url, CHECK_NAVIGATION_TIMEOUT)
            .await
            .map_err(|e| format!("直播间页面导航失败：{e}"))?;
        tokio::time::sleep(CHECK_SETTLE).await;
        let value = task
            .evaluate(SNAPSHOT_EXTRACTOR_JS, EVAL_WAIT)
            .await
            .map_err(|e| format!("直播间快照读取失败：{e}"))?;
        let snapshot: LiveRoomStatusSnapshot =
            serde_json::from_value(value).map_err(|e| format!("直播间快照解析失败：{e}"))?;
        Ok(classify_snapshot(&snapshot))
    }
}

/// 该槽当前应有的轮询间隔（槽不存在 → 慢轮询兜底）。
async fn current_poll_ms(profile_id: &str) -> u64 {
    let slots = MONITOR.slots.read().await;
    match slots.get(profile_id) {
        Some(slot) => next_poll_ms(slot.state.live_status, slot.state.triggered_for_current_live),
        None => SLOW_POLL_MS,
    }
}

/// 单次检测并落回该 profile 的槽。所有状态读写都限定在本槽，
/// 绝不触碰其他 profile 的槽（多账号并发安全）。
/// 检测优先用注入桩 `probe`，否则走 `driver` 的真实浏览器检测。
async fn monitor_tick(
    driver: Option<&TauriBrowserDriver>,
    probe: Option<&MonitorProbe>,
    sink: &StateSink,
    profile_id: &str,
    cancel: &TaskCancel,
) {
    let (url, prev_status) = {
        let slots = MONITOR.slots.read().await;
        let Some(slot) = slots.get(profile_id) else {
            return;
        };
        if !slot.state.enabled {
            return;
        }
        (slot.state.live_room_url.clone(), slot.state.status)
    };
    let Some(url) = url else {
        let snapshot = {
            let mut slots = MONITOR.slots.write().await;
            let Some(slot) = slots.get_mut(profile_id) else {
                return;
            };
            slot.state.status = LiveRoomMonitorStatus::Error;
            slot.state.error = Some("未配置直播间链接".into());
            slot.state.clone()
        };
        emit_state(sink, &snapshot);
        return;
    };
    {
        let mut slots = MONITOR.slots.write().await;
        let Some(slot) = slots.get_mut(profile_id) else {
            return;
        };
        if !slot.state.enabled {
            return;
        }
        slot.state.status = LiveRoomMonitorStatus::Checking;
    }
    let outcome = match probe {
        Some(probe) => probe(profile_id, &url).await,
        None => match driver {
            Some(driver) => driver.check_once(profile_id, &url).await,
            None => Err("监控检测未接入".into()),
        },
    };
    if cancel.is_cancelled() {
        return;
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    match outcome {
        Err(message) => {
            // 检测链路失败（环境关闭/导航超时等）：记 Error 但不清触发位，
            // 下轮继续，不丢同一场直播的触发状态。
            let snapshot = {
                let mut slots = MONITOR.slots.write().await;
                let Some(slot) = slots.get_mut(profile_id) else {
                    return;
                };
                slot.state.status = LiveRoomMonitorStatus::Error;
                slot.state.error = Some(message);
                slot.state.last_checked_at = Some(now_ms);
                slot.state.next_check_at = Some(now_ms + SLOW_POLL_MS as i64);
                slot.state.clone()
            };
            emit_state(sink, &snapshot);
        }
        Ok(RoomPageClass::Invalid) => {
            // fatal：房间不存在，停机（保留配置供用户修正后重启）。
            // 只停本槽并取消本槽循环，其他 profile 不受影响。
            let (snapshot, own_cancel) = {
                let mut slots = MONITOR.slots.write().await;
                let Some(slot) = slots.get_mut(profile_id) else {
                    return;
                };
                slot.state.enabled = false;
                slot.state.status = LiveRoomMonitorStatus::Error;
                slot.state.error = Some("直播间不存在或已删除，监控已停止".into());
                slot.state.last_checked_at = Some(now_ms);
                (slot.state.clone(), slot.cancel.clone())
            };
            if let Some(c) = own_cancel {
                c.cancel();
            }
            cancel.cancel();
            emit_state(sink, &snapshot);
        }
        Ok(RoomPageClass::Limited) | Ok(RoomPageClass::Unknown) => {
            // 非明确信号：恢复 Checking 之前的状态，只刷新时间戳。
            let snapshot = {
                let mut slots = MONITOR.slots.write().await;
                let Some(slot) = slots.get_mut(profile_id) else {
                    return;
                };
                if slot.state.status == LiveRoomMonitorStatus::Checking {
                    slot.state.status = prev_status;
                }
                slot.state.last_checked_at = Some(now_ms);
                slot.state.next_check_at = Some(
                    now_ms
                        + next_poll_ms(
                            slot.state.live_status,
                            slot.state.triggered_for_current_live,
                        ) as i64,
                );
                slot.state.clone()
            };
            emit_state(sink, &snapshot);
        }
        Ok(RoomPageClass::Offline) => {
            let trigger = TRIGGER.read().await.clone();
            let snapshot = {
                let mut slots = MONITOR.slots.write().await;
                let Some(slot) = slots.get_mut(profile_id) else {
                    return;
                };
                if !slot.state.enabled {
                    return;
                }
                let app_emit = |s: &LiveRoomMonitorState| sink(s);
                apply_live_status(
                    trigger.as_ref(),
                    &mut slot.state,
                    LiveRoomLiveStatus::Offline,
                    now_ms,
                    app_emit,
                )
                .await;
                slot.state.clone()
            };
            emit_state(sink, &snapshot);
        }
        Ok(RoomPageClass::Live) => {
            let trigger = TRIGGER.read().await.clone();
            let snapshot = {
                let mut slots = MONITOR.slots.write().await;
                let Some(slot) = slots.get_mut(profile_id) else {
                    return;
                };
                if !slot.state.enabled {
                    return;
                }
                let app_emit = |s: &LiveRoomMonitorState| sink(s);
                apply_live_status(
                    trigger.as_ref(),
                    &mut slot.state,
                    LiveRoomLiveStatus::Live,
                    now_ms,
                    app_emit,
                )
                .await;
                slot.state.clone()
            };
            emit_state(sink, &snapshot);
        }
    }
}

/// 单个 profile 的监控循环：state / cancel / checking 全部绑定到本槽，
/// 与其他 profile 的循环互不阻塞。
async fn monitor_loop(
    driver: Arc<TauriBrowserDriver>,
    profile_id: String,
    cancel: TaskCancel,
    sink: StateSink,
    probe: Option<MonitorProbe>,
) {
    loop {
        if cancel.is_cancelled() {
            break;
        }
        {
            let slots = MONITOR.slots.read().await;
            match slots.get(&profile_id) {
                Some(slot) if slot.state.enabled => {}
                _ => break,
            }
        }
        // checking 互斥：取本槽的独立互斥锁，上轮未完成则跳过本轮，不堆积。
        // 用 Arc 克隆后在锁外 await，避免持槽读锁跨 await。
        let checking = {
            let slots = MONITOR.slots.read().await;
            match slots.get(&profile_id) {
                Some(slot) => Arc::clone(&slot.checking),
                None => break,
            }
        };
        match checking.try_lock() {
            Ok(_guard) => {
                monitor_tick(
                    Some(&driver),
                    probe.as_ref(),
                    &sink,
                    &profile_id,
                    &cancel,
                )
                .await
            }
            Err(_) => tracing::debug!(
                profile = %profile_id,
                "live-room-monitor: 上轮检测未完成，跳过本轮"
            ),
        }
        if cancel.is_cancelled() {
            break;
        }
        {
            let slots = MONITOR.slots.read().await;
            match slots.get(&profile_id) {
                Some(slot) if slot.state.enabled => {}
                _ => break,
            }
        }
        let delay_ms = current_poll_ms(&profile_id).await;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
        }
    }
    tracing::info!(profile = %profile_id, "live-room-monitor: 循环退出");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn snapshot(title: &str, text: &str, input_visible: bool, media_visible: bool) -> LiveRoomStatusSnapshot {
        LiveRoomStatusSnapshot {
            title: title.into(),
            href: "https://live.kuaishou.com/u/test".into(),
            text: text.into(),
            input_visible,
            media_visible,
        }
    }

    #[test]
    fn invalid_room_keywords_are_fatal() {
        let snap = snapshot("404", "页面不存在，请检查链接", false, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Invalid);
        let snap = snapshot("快手直播", "该直播间不存在", false, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Invalid);
    }

    #[test]
    fn offline_room_keywords_classify_offline() {
        let snap = snapshot("某某的直播间", "主播暂未开播，点我开播提醒", true, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Offline);
        let snap = snapshot("某某的直播间", "本场直播已结束，看看精彩回放", false, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Offline);
    }

    #[test]
    fn live_needs_keyword_plus_evidence() {
        // 关键词 + 播放器 → Live
        let snap = snapshot("某某正在直播", "直播中，欢迎大家", true, true);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Live);
        // 关键词 + 观看人数 → Live（无 video 标签也行）
        let snap = snapshot("某某的直播间", "3.2万人在看，直播中", true, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Live);
        // 播放器 + 观看人数 → Live（标题无关键词）
        let snap = snapshot("某某的直播间", "1.5万人观看", false, true);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Live);
        // 只有关键词、无任何佐证 → 不是 Live（可能是静态残留文案）
        let snap = snapshot("某某的直播间", "直播中", false, false);
        assert_ne!(classify_snapshot(&snap), RoomPageClass::Live);
    }

    #[test]
    fn access_limited_downgrades_without_fatal() {
        let snap = snapshot("快手直播", "错误代码22，访问受限", false, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Limited);
        let snap = snapshot("", "系统繁忙，请稍后重试", false, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Limited);
    }

    #[test]
    fn empty_page_is_unknown() {
        let snap = snapshot("", "", false, false);
        assert_eq!(classify_snapshot(&snap), RoomPageClass::Unknown);
    }

    #[test]
    fn viewer_count_requires_digits() {
        assert!(has_viewer_count("3.2万人在看"));
        assert!(has_viewer_count("在线人数 128"));
        assert!(!has_viewer_count("很多人在看"));
        assert!(!has_viewer_count(""));
    }

    #[test]
    fn transition_should_trigger_only_once_per_live() {
        let t = MonitorTransition {
            detected: LiveRoomLiveStatus::Live,
            triggered_for_current_live: false,
        };
        assert!(t.should_trigger());
        let t = MonitorTransition {
            detected: LiveRoomLiveStatus::Live,
            triggered_for_current_live: true,
        };
        assert!(!t.should_trigger());
        let t = MonitorTransition {
            detected: LiveRoomLiveStatus::Offline,
            triggered_for_current_live: false,
        };
        assert!(!t.should_trigger());
        let t = MonitorTransition {
            detected: LiveRoomLiveStatus::Unknown,
            triggered_for_current_live: false,
        };
        assert!(!t.should_trigger());
    }

    #[test]
    fn transition_live_ended_detection() {
        assert!(MonitorTransition::live_ended(
            LiveRoomLiveStatus::Live,
            LiveRoomLiveStatus::Offline
        ));
        // 中间经过 Unknown/Limited 不算结束，避免受限抖动重复触发。
        assert!(!MonitorTransition::live_ended(
            LiveRoomLiveStatus::Live,
            LiveRoomLiveStatus::Unknown
        ));
        assert!(!MonitorTransition::live_ended(
            LiveRoomLiveStatus::Unknown,
            LiveRoomLiveStatus::Offline
        ));
    }

    #[test]
    fn poll_interval_switches_fast_slow() {
        assert_eq!(
            next_poll_ms(LiveRoomLiveStatus::Live, false),
            FAST_POLL_MS
        );
        assert_eq!(
            next_poll_ms(LiveRoomLiveStatus::Offline, true),
            FAST_POLL_MS
        );
        assert_eq!(
            next_poll_ms(LiveRoomLiveStatus::Offline, false),
            SLOW_POLL_MS
        );
        assert_eq!(
            next_poll_ms(LiveRoomLiveStatus::Unknown, false),
            SLOW_POLL_MS
        );
    }

    #[test]
    fn live_room_url_validation() {
        assert!(is_live_room_url("https://live.kuaishou.com/u/xxx"));
        assert!(is_live_room_url("http://localhost:8080/room"));
        assert!(!is_live_room_url(""));
        assert!(!is_live_room_url("   "));
        assert!(!is_live_room_url("live.kuaishou.com/u/xxx"));
    }

    struct MockTrigger {
        starts: AtomicUsize,
        ends: AtomicUsize,
        fail: bool,
    }

    impl MockTrigger {
        fn new(fail: bool) -> Self {
            Self {
                starts: AtomicUsize::new(0),
                ends: AtomicUsize::new(0),
                fail,
            }
        }
    }

    #[async_trait]
    impl LiveRoomTrigger for MockTrigger {
        async fn on_live_start(&self, _ctx: TriggerContext) -> LiveStartOutcome {
            self.starts.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                LiveStartOutcome {
                    enter: TriggerBatchResult::failed("enter boom"),
                    product_script: TriggerBatchResult::failed("script boom"),
                }
            } else {
                LiveStartOutcome {
                    enter: TriggerBatchResult::ok(3, 3, "3/3 进入"),
                    product_script: TriggerBatchResult::ok(1, 1, "货盘脚本已触发"),
                }
            }
        }

        async fn on_live_end(&self, _ctx: TriggerContext, _auto_exit: bool) -> LiveEndOutcome {
            self.ends.fetch_add(1, Ordering::SeqCst);
            LiveEndOutcome {
                exit: TriggerBatchResult::ok(3, 3, "3/3 退出"),
            }
        }
    }

    fn live_state() -> LiveRoomMonitorState {
        LiveRoomMonitorState {
            enabled: true,
            live_room_url: Some("https://live.kuaishou.com/u/test".into()),
            ..LiveRoomMonitorState::default()
        }
    }

    #[tokio::test]
    async fn mock_trigger_fires_once_per_live_session() {
        let trigger = MockTrigger::new(false);
        let mut state = live_state();

        // 首次 Live → 触发一次，落 Triggered。
        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Live, 1000, |_| {}).await;
        assert_eq!(trigger.starts.load(Ordering::SeqCst), 1);
        assert_eq!(state.status, LiveRoomMonitorStatus::Triggered);
        assert!(state.triggered_for_current_live);
        assert!(!state.entering_rooms);
        assert_eq!(state.last_triggered_at, Some(1000));
        assert!(state.last_enter_all_result.is_some());
        assert!(state.last_product_script_result.is_some());

        // 同一场直播内再次 Live → 不重复触发。
        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Live, 2000, |_| {}).await;
        assert_eq!(trigger.starts.load(Ordering::SeqCst), 1);

        // Live → Offline → 下播联动一次，触发位复位。
        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Offline, 3000, |_| {}).await;
        assert_eq!(trigger.ends.load(Ordering::SeqCst), 1);
        assert_eq!(state.status, LiveRoomMonitorStatus::Offline);
        assert!(!state.triggered_for_current_live);
        assert!(!state.exiting_rooms);
        assert!(state.last_exit_all_result.is_some());

        // 再次开播 → 再次触发。
        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Live, 4000, |_| {}).await;
        assert_eq!(trigger.starts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn trigger_failure_is_recorded_without_breaking_loop() {
        let trigger = MockTrigger::new(true);
        let mut state = live_state();

        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Live, 1000, |_| {}).await;
        // 失败只记录，不抛错、不卡状态机：仍落 Triggered，触发位照常。
        assert_eq!(state.status, LiveRoomMonitorStatus::Triggered);
        assert!(state.triggered_for_current_live);
        let enter = state.last_enter_all_result.as_ref().unwrap();
        assert_eq!(enter.error.as_deref(), Some("enter boom"));
        // 循环可继续：下播联动照常。
        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Offline, 2000, |_| {}).await;
        assert_eq!(state.status, LiveRoomMonitorStatus::Offline);
        assert_eq!(trigger.ends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unknown_keeps_state_and_updates_timestamps() {
        let trigger = MockTrigger::new(false);
        let mut state = live_state();
        state.status = LiveRoomMonitorStatus::Offline;
        state.live_status = LiveRoomLiveStatus::Offline;

        apply_live_status(&trigger, &mut state, LiveRoomLiveStatus::Unknown, 5000, |_| {}).await;
        assert_eq!(state.status, LiveRoomMonitorStatus::Offline);
        assert_eq!(state.live_status, LiveRoomLiveStatus::Offline);
        assert_eq!(trigger.starts.load(Ordering::SeqCst), 0);
        assert_eq!(trigger.ends.load(Ordering::SeqCst), 0);
        assert_eq!(state.last_checked_at, Some(5000));
        assert!(state.next_check_at.is_some());
    }

    // --- 多槽（按 profile）监控测试：注入检测桩，不触真实网络 ---

    /// 多槽测试共享全局 MONITOR，串行化避免槽/桩互相干扰。
    static TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    /// 返回固定分类的检测桩（离线，不触网络）。
    fn probe_returning(class: RoomPageClass) -> MonitorProbe {
        Arc::new(move |_profile: &str, _url: &str| {
            Box::pin(async move { Ok(class) }) as ProbeFuture
        })
    }

    fn mon_config(url: &str) -> MonitorConfig {
        MonitorConfig {
            live_room_url: url.into(),
            scene_id: None,
            group_id: None,
            product_script_id: None,
            product_script_account_id: None,
            auto_exit_sub_accounts: false,
        }
    }

    /// 离线可用的 driver（不启动浏览器、不触网络）。
    fn monitor_driver() -> (TempDir, Arc<TauriBrowserDriver>) {
        use multizen_core::BrowserEngine;
        let dir = TempDir::new().unwrap();
        let driver = TauriBrowserDriver::start(
            dir.path().join("p.db"),
            dir.path().join("profiles"),
            dir.path().join("extensions"),
            Arc::new(crate::registry::ProfileRegistry::new()),
            BrowserEngine::Chromix,
            std::path::PathBuf::new(),
            None,
        )
        .unwrap();
        (dir, Arc::new(driver))
    }

    /// 清空所有槽 + 注入离线检测桩（测试起始统一调用）。
    async fn reset_monitor(class: RoomPageClass) {
        MONITOR.slots.write().await.clear();
        set_monitor_probe(Some(probe_returning(class))).await;
    }

    /// 手工插入一个“在跑”的槽（用于直接驱动 monitor_loop 的测试）。
    async fn insert_slot(profile_id: &str, url: &str) {
        let mut slots = MONITOR.slots.write().await;
        slots.insert(
            profile_id.to_string(),
            MonitorSlot::new(
                LiveRoomMonitorState {
                    enabled: true,
                    profile_id: Some(profile_id.to_string()),
                    live_room_url: Some(url.to_string()),
                    status: LiveRoomMonitorStatus::Checking,
                    ..LiveRoomMonitorState::default()
                },
                Some(TaskCancel::new()),
            ),
        );
    }

    /// 等该槽完成至少一轮检测（last_checked_at 有值）。
    async fn wait_checked(profile_id: &str, timeout_ms: u64) {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            if MONITOR.state(profile_id).await.last_checked_at.is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("profile {profile_id} 未在 {timeout_ms}ms 内完成一次检测");
    }

    /// 两个不同 profile 同时 start → 两槽各自独立、都在跑、profileId 正确。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn multi_slot_start_keeps_profiles_independent() {
        let _guard = TEST_LOCK.lock().await;
        reset_monitor(RoomPageClass::Offline).await;
        let (_dir, driver) = monitor_driver();
        let app = tauri::test::mock_app().handle().clone();

        let a = driver
            .start_live_room_monitor(&app, "profileA", mon_config("https://live.kuaishou.com/u/a"))
            .await
            .unwrap();
        let b = driver
            .start_live_room_monitor(&app, "profileB", mon_config("https://live.kuaishou.com/u/b"))
            .await
            .unwrap();
        assert_eq!(a.profile_id.as_deref(), Some("profileA"));
        assert_eq!(b.profile_id.as_deref(), Some("profileB"));
        assert!(a.enabled && b.enabled);

        // 两个槽都在，互不覆盖。
        let states = MONITOR.list().await;
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].profile_id.as_deref(), Some("profileA"));
        assert_eq!(states[1].profile_id.as_deref(), Some("profileB"));

        // 两槽并发各自完成检测。
        wait_checked("profileA", 5000).await;
        wait_checked("profileB", 5000).await;
        let sa = MONITOR.state("profileA").await;
        let sb = MONITOR.state("profileB").await;
        assert_eq!(sa.profile_id.as_deref(), Some("profileA"));
        assert_eq!(sb.profile_id.as_deref(), Some("profileB"));
        assert_eq!(sa.live_status, LiveRoomLiveStatus::Offline);
        assert_eq!(sb.live_status, LiveRoomLiveStatus::Offline);

        driver
            .stop_live_room_monitor(&app, "profileA")
            .await
            .unwrap();
        driver
            .stop_live_room_monitor(&app, "profileB")
            .await
            .unwrap();
        set_monitor_probe(None).await;
    }

    /// stop(profileA) 后：A 槽为空（default），B 槽仍在跑。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_one_profile_leaves_others_running() {
        let _guard = TEST_LOCK.lock().await;
        reset_monitor(RoomPageClass::Offline).await;
        let (_dir, driver) = monitor_driver();
        let app = tauri::test::mock_app().handle().clone();

        driver
            .start_live_room_monitor(&app, "profileA", mon_config("https://live.kuaishou.com/u/a"))
            .await
            .unwrap();
        driver
            .start_live_room_monitor(&app, "profileB", mon_config("https://live.kuaishou.com/u/b"))
            .await
            .unwrap();
        wait_checked("profileA", 5000).await;
        wait_checked("profileB", 5000).await;

        let stopped = driver
            .stop_live_room_monitor(&app, "profileA")
            .await
            .unwrap();
        assert_eq!(stopped.profile_id.as_deref(), Some("profileA"));

        // A 槽已移除 → get 返回默认（profileId: null）。
        let a = MONITOR.state("profileA").await;
        assert!(a.profile_id.is_none());
        assert!(!a.enabled);
        assert_eq!(a.status, LiveRoomMonitorStatus::Idle);

        // B 槽仍在跑，未受影响。
        let b = MONITOR.state("profileB").await;
        assert_eq!(b.profile_id.as_deref(), Some("profileB"));
        assert!(b.enabled);
        assert_eq!(MONITOR.list().await.len(), 1);

        driver
            .stop_live_room_monitor(&app, "profileB")
            .await
            .unwrap();
        set_monitor_probe(None).await;
    }

    /// list_* 返回全部槽（按 profileId 排序）；未知 profile 的 get 返回默认。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_returns_all_slots_and_get_unknown_is_default() {
        let _guard = TEST_LOCK.lock().await;
        reset_monitor(RoomPageClass::Offline).await;
        let (_dir, driver) = monitor_driver();
        let app = tauri::test::mock_app().handle().clone();

        // 未启动任何槽 → list 空、get 默认（行为与“未启动”一致）。
        assert!(MONITOR.list().await.is_empty());
        let none = MONITOR.state("ghost").await;
        assert!(none.profile_id.is_none());
        assert!(!none.enabled);
        assert_eq!(none.status, LiveRoomMonitorStatus::Idle);

        driver
            .start_live_room_monitor(&app, "profileA", mon_config("https://live.kuaishou.com/u/a"))
            .await
            .unwrap();
        driver
            .start_live_room_monitor(&app, "profileB", mon_config("https://live.kuaishou.com/u/b"))
            .await
            .unwrap();

        let list = MONITOR.list().await;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].profile_id.as_deref(), Some("profileA"));
        assert_eq!(list[1].profile_id.as_deref(), Some("profileB"));

        driver
            .stop_live_room_monitor(&app, "profileA")
            .await
            .unwrap();
        driver
            .stop_live_room_monitor(&app, "profileB")
            .await
            .unwrap();
        set_monitor_probe(None).await;
    }

    /// 同一 profile 重复 start 为替换：旧任务被取消、槽只有一个、新循环照跑。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restart_same_profile_replaces_without_leaking() {
        let _guard = TEST_LOCK.lock().await;
        reset_monitor(RoomPageClass::Offline).await;
        let (_dir, driver) = monitor_driver();
        let app = tauri::test::mock_app().handle().clone();

        driver
            .start_live_room_monitor(&app, "profileA", mon_config("https://live.kuaishou.com/u/a1"))
            .await
            .unwrap();
        wait_checked("profileA", 5000).await;
        let first = MONITOR.state("profileA").await;
        assert_eq!(
            first.live_room_url.as_deref(),
            Some("https://live.kuaishou.com/u/a1")
        );
        let t1 = first.last_checked_at.unwrap();

        // 同 profile 再启动（新链接）→ 幂等替换，槽仍只有一个。
        let second = driver
            .start_live_room_monitor(&app, "profileA", mon_config("https://live.kuaishou.com/u/a2"))
            .await
            .unwrap();
        assert_eq!(
            second.live_room_url.as_deref(),
            Some("https://live.kuaishou.com/u/a2")
        );
        assert_eq!(MONITOR.list().await.len(), 1);
        let replaced = MONITOR.state("profileA").await;
        assert_eq!(
            replaced.live_room_url.as_deref(),
            Some("https://live.kuaishou.com/u/a2")
        );
        assert!(replaced.enabled);

        // 新循环确实在跑：last_checked_at 前进。
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let now = MONITOR.state("profileA").await.last_checked_at.unwrap_or(0);
            if now > t1 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "替换后新循环未运行（last_checked_at 未前进）"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        driver
            .stop_live_room_monitor(&app, "profileA")
            .await
            .unwrap();
        set_monitor_probe(None).await;
    }

    /// 每槽 checking 互斥独立：A 槽检测阻塞时，B 槽仍能推进。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn per_slot_checking_is_independent() {
        let _guard = TEST_LOCK.lock().await;
        MONITOR.slots.write().await.clear();
        let (_dir, driver) = monitor_driver();
        let app = tauri::test::mock_app().handle().clone();

        // A 槽检测阻塞（等一个信号量许可）；B 槽正常返回。
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let gate_a = Arc::clone(&gate);
        let probe: MonitorProbe = Arc::new(move |profile: &str, _url: &str| {
            let gate = Arc::clone(&gate_a);
            let is_a = profile == "profileA";
            Box::pin(async move {
                if is_a {
                    let _permit = gate.acquire().await.unwrap();
                }
                Ok(RoomPageClass::Offline)
            }) as ProbeFuture
        });
        set_monitor_probe(Some(probe)).await;

        driver
            .start_live_room_monitor(&app, "profileA", mon_config("https://live.kuaishou.com/u/a"))
            .await
            .unwrap();
        driver
            .start_live_room_monitor(&app, "profileB", mon_config("https://live.kuaishou.com/u/b"))
            .await
            .unwrap();

        // A 卡在检测中：B 不受影响，照常完成检测。
        wait_checked("profileB", 3000).await;
        assert!(MONITOR.state("profileA").await.last_checked_at.is_none());
        assert!(MONITOR.state("profileB").await.last_checked_at.is_some());

        // 释放 A → A 也完成。
        gate.add_permits(1);
        wait_checked("profileA", 3000).await;

        driver
            .stop_live_room_monitor(&app, "profileA")
            .await
            .unwrap();
        driver
            .stop_live_room_monitor(&app, "profileB")
            .await
            .unwrap();
        set_monitor_probe(None).await;
    }

    /// 事件推送 payload 含 profileId：每槽推送都能定位到对应账号。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn state_changes_carry_profile_id() {
        let _guard = TEST_LOCK.lock().await;
        MONITOR.slots.write().await.clear();
        let (_dir, driver) = monitor_driver();

        // 捕获推送出口：记录 (profileId, status)。
        let captured: Arc<std::sync::Mutex<Vec<(Option<String>, LiveRoomMonitorStatus)>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);
        let sink: StateSink = Arc::new(move |s: &LiveRoomMonitorState| {
            captured_clone
                .lock()
                .unwrap()
                .push((s.profile_id.clone(), s.status));
        });

        // 手工插槽 + 直接驱动 monitor_loop（注入桩 + 捕获出口）。
        insert_slot("profileA", "https://live.kuaishou.com/u/a").await;
        insert_slot("profileB", "https://live.kuaishou.com/u/b").await;
        let cancel_a = TaskCancel::new();
        let cancel_b = TaskCancel::new();
        let handle_a = tokio::spawn(monitor_loop(
            Arc::clone(&driver),
            "profileA".into(),
            cancel_a.clone(),
            Arc::clone(&sink),
            Some(probe_returning(RoomPageClass::Offline)),
        ));
        let handle_b = tokio::spawn(monitor_loop(
            Arc::clone(&driver),
            "profileB".into(),
            cancel_b.clone(),
            Arc::clone(&sink),
            Some(probe_returning(RoomPageClass::Offline)),
        ));

        wait_checked("profileA", 3000).await;
        wait_checked("profileB", 3000).await;
        cancel_a.cancel();
        cancel_b.cancel();
        let _ = handle_a.await;
        let _ = handle_b.await;

        let events = captured.lock().unwrap().clone();
        assert!(events
            .iter()
            .any(|(p, _)| p.as_deref() == Some("profileA")));
        assert!(events
            .iter()
            .any(|(p, _)| p.as_deref() == Some("profileB")));
        // 每个推送都带 profileId（前端可据此分发到对应账号）。
        assert!(events.iter().all(|(p, _)| p.is_some()));
    }
}
