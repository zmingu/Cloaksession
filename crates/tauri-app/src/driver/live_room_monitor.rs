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

struct MonitorRuntime {
    state: RwLock<LiveRoomMonitorState>,
    cancel: RwLock<Option<TaskCancel>>,
    /// `checking` 互斥：上轮检测未完成时跳过本轮（同 jieger）。
    checking: Mutex<()>,
}

static MONITOR: LazyLock<MonitorRuntime> = LazyLock::new(|| MonitorRuntime {
    state: RwLock::new(LiveRoomMonitorState::default()),
    cancel: RwLock::new(None),
    checking: Mutex::new(()),
});

fn emit_state(app: &tauri::AppHandle, state: &LiveRoomMonitorState) {
    if let Err(e) = app.emit(LIVE_ROOM_MONITOR_STATE_CHANGED, state) {
        tracing::warn!(error = %e, "live-room-monitor: 状态推送失败");
    }
}

/// 同步读当前快照（给同步 IPC 命令用；低频调用，可接受阻塞读）。
pub fn current_state_blocking() -> LiveRoomMonitorState {
    MONITOR.state.blocking_read().clone()
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
    /// 当前监控快照（未启动过 → Idle 默认）。
    pub async fn live_room_monitor_state(&self) -> LiveRoomMonitorState {
        MONITOR.state.read().await.clone()
    }

    /// 启动监控：校验配置 → 落状态 → 起 tokio 循环（`&Arc<Self>` 起循环，
    /// 同 identity monitor 模式）。重复启动直接拒绝。
    pub fn start_live_room_monitor(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        profile_id: &str,
        config: MonitorConfig,
    ) -> std::result::Result<LiveRoomMonitorState, String> {
        if !is_live_room_url(&config.live_room_url) {
            return Err("直播间链接无效，须以 http(s):// 开头".into());
        }
        if profile_id.trim().is_empty() {
            return Err("请先选择执行检测的浏览器环境".into());
        }
        // 同步检查运行态：已在运行则拒绝（tokio RwLock 用 blocking 写锁，
        // 启动路径低频，可接受）。
        {
            let mut state = MONITOR.state.blocking_write();
            if state.enabled {
                return Err("直播间监控已在运行，请先停止".into());
            }
            *state = LiveRoomMonitorState {
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
            // enabled 必须在 struct-update 之后重新置 true（default 为 false）。
            state.enabled = true;
            state.status = LiveRoomMonitorStatus::Checking;
        }
        let cancel = TaskCancel::new();
        MONITOR.cancel.blocking_write().replace(cancel.clone());
        let snapshot = MONITOR.state.blocking_read().clone();
        emit_state(app, &snapshot);
        tracing::info!(
            profile = %profile_id,
            url = %config.live_room_url,
            "live-room-monitor: 启动"
        );
        tauri::async_runtime::spawn(monitor_loop(Arc::clone(self), app.clone(), cancel));
        Ok(snapshot)
    }

    /// 停止监控：取消循环 → 落 Idle（幂等，未运行也返回当前快照）。
    pub async fn stop_live_room_monitor(
        &self,
        app: &tauri::AppHandle,
    ) -> std::result::Result<LiveRoomMonitorState, String> {
        if let Some(cancel) = MONITOR.cancel.write().await.take() {
            cancel.cancel();
        }
        let snapshot = {
            let mut state = MONITOR.state.write().await;
            state.enabled = false;
            state.status = LiveRoomMonitorStatus::Idle;
            state.entering_rooms = false;
            state.exiting_rooms = false;
            state.clone()
        };
        emit_state(app, &snapshot);
        tracing::info!("live-room-monitor: 已停止");
        Ok(snapshot)
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

async fn current_poll_ms() -> u64 {
    let state = MONITOR.state.read().await;
    next_poll_ms(state.live_status, state.triggered_for_current_live)
}

async fn monitor_tick(
    driver: &TauriBrowserDriver,
    app: &tauri::AppHandle,
    cancel: &TaskCancel,
) {
    let (profile_id, url, prev_status) = {
        let state = MONITOR.state.read().await;
        if !state.enabled {
            return;
        }
        (
            state.profile_id.clone().unwrap_or_default(),
            state.live_room_url.clone(),
            state.status,
        )
    };
    let Some(url) = url else {
        let mut state = MONITOR.state.write().await;
        state.status = LiveRoomMonitorStatus::Error;
        state.error = Some("未配置直播间链接".into());
        let snapshot = state.clone();
        drop(state);
        emit_state(app, &snapshot);
        return;
    };
    {
        let mut state = MONITOR.state.write().await;
        if !state.enabled {
            return;
        }
        state.status = LiveRoomMonitorStatus::Checking;
    }
    let outcome = driver.check_once(&profile_id, &url).await;
    if cancel.is_cancelled() {
        return;
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    match outcome {
        Err(message) => {
            // 检测链路失败（环境关闭/导航超时等）：记 Error 但不清触发位，
            // 下轮继续，不丢同一场直播的触发状态。
            let snapshot = {
                let mut state = MONITOR.state.write().await;
                state.status = LiveRoomMonitorStatus::Error;
                state.error = Some(message);
                state.last_checked_at = Some(now_ms);
                state.next_check_at = Some(now_ms + SLOW_POLL_MS as i64);
                state.clone()
            };
            emit_state(app, &snapshot);
        }
        Ok(RoomPageClass::Invalid) => {
            // fatal：房间不存在，停机（保留配置供用户修正后重启）。
            let snapshot = {
                let mut state = MONITOR.state.write().await;
                state.enabled = false;
                state.status = LiveRoomMonitorStatus::Error;
                state.error = Some("直播间不存在或已删除，监控已停止".into());
                state.last_checked_at = Some(now_ms);
                state.clone()
            };
            MONITOR.cancel.write().await.take().map(|c| c.cancel());
            cancel.cancel();
            emit_state(app, &snapshot);
        }
        Ok(RoomPageClass::Limited) | Ok(RoomPageClass::Unknown) => {
            // 非明确信号：恢复 Checking 之前的状态，只刷新时间戳。
            let snapshot = {
                let mut state = MONITOR.state.write().await;
                if state.status == LiveRoomMonitorStatus::Checking {
                    state.status = prev_status;
                }
                state.last_checked_at = Some(now_ms);
                state.next_check_at = Some(
                    now_ms + next_poll_ms(state.live_status, state.triggered_for_current_live) as i64,
                );
                state.clone()
            };
            emit_state(app, &snapshot);
        }
        Ok(RoomPageClass::Offline) => {
            let trigger = TRIGGER.read().await.clone();
            let snapshot = {
                let mut state = MONITOR.state.write().await;
                if !state.enabled {
                    return;
                }
                let app_emit = |s: &LiveRoomMonitorState| emit_state(app, s);
                apply_live_status(
                    trigger.as_ref(),
                    &mut state,
                    LiveRoomLiveStatus::Offline,
                    now_ms,
                    app_emit,
                )
                .await;
                state.clone()
            };
            emit_state(app, &snapshot);
        }
        Ok(RoomPageClass::Live) => {
            let trigger = TRIGGER.read().await.clone();
            let snapshot = {
                let mut state = MONITOR.state.write().await;
                if !state.enabled {
                    return;
                }
                let app_emit = |s: &LiveRoomMonitorState| emit_state(app, s);
                apply_live_status(
                    trigger.as_ref(),
                    &mut state,
                    LiveRoomLiveStatus::Live,
                    now_ms,
                    app_emit,
                )
                .await;
                state.clone()
            };
            emit_state(app, &snapshot);
        }
    }
}

async fn monitor_loop(
    driver: Arc<TauriBrowserDriver>,
    app: tauri::AppHandle,
    cancel: TaskCancel,
) {
    loop {
        if cancel.is_cancelled() {
            break;
        }
        {
            let enabled = MONITOR.state.read().await.enabled;
            if !enabled {
                break;
            }
        }
        // checking 互斥：上轮未完成则跳过本轮，不堆积。
        match MONITOR.checking.try_lock() {
            Ok(_guard) => monitor_tick(&driver, &app, &cancel).await,
            Err(_) => tracing::debug!("live-room-monitor: 上轮检测未完成，跳过本轮"),
        }
        if cancel.is_cancelled() {
            break;
        }
        {
            let enabled = MONITOR.state.read().await.enabled;
            if !enabled {
                break;
            }
        }
        let delay_ms = current_poll_ms().await;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
        }
    }
    tracing::info!("live-room-monitor: 循环退出");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
}
