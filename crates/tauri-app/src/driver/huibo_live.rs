//! huibo-live — 跟播/回播迁移 (jieger `tasks/huiboLive`, 51 行最小模块).
//!
//! jieger 源文件对照:
//! - `electron/main/tasks/huiboLive/index.ts` — `getHuiboVideoList` / `startHuiboLive`
//! - `electron/main/ipc/handlers/huiboLiveHandler.ts` — IPC (本文件对应 `commands/huibo_live.rs`)
//! - `electron/main/platforms/kuaishou/huiboActions.ts` — `fetchVideoList` / `startHuiboLiveFlow`
//! - `electron/main/platforms/kuaishou/huiboSelectors.ts` — `HUIBO_SELECTORS`
//!
//! # 以 jieger 源码为准的纠正 (design.md 初稿勘误)
//!
//! - `HuiboVideo` 真实字段 = jieger `src/types/entities.ts` 的 10 个字段
//!   (`id`/`name`/`uploadTime`/`segments`/`size`/`duration`/`status`/
//!   `usageCount`/`expiryTime`/`goodsCount`)。**没有** `thumbnail`/`title`,
//!   `duration` 是字符串 (如 `"1小时41分"`),**不是** `i64`。
//! - 列表 URL: `https://zs.kwaixiaodian.com/page/record-live/upload`
//!   (jieger `HUIBO_VIDEO_LIST_URL`)。
//! - `fetchVideoList` **不是纯读**: `replayId` 只能从"去使用"按钮点击后的 URL
//!   (`replayId=(\d+)`) 提取, 每次点击都会导航离开列表页再 `goBack`。
//!   本移植逐行复刻该副作用 (按行索引重新查询, 不持有跨导航的过时引用),
//!   点击失败的行 `id` 留空并继续, 不中断整表。
//! - `startHuiboLiveFlow` 尾部的固定 3 秒等待**不是成功证明**:
//!   `start_huibo_live_flow` 只负责把流程点到"去开播", 成功与否一律由随后
//!   的 `get_shop_live_state` (中控页状态复核) 判定, 顶层 `huibo_start_live`
//!   返回的 `ShopLiveState` 才是验收依据。
//!
//! # 与 Cloaksession 现状的对接说明
//!
//! - jieger `connect` (ensure 会话连接) 对应 `ks-platform-primitives` 的
//!   `ensure_auth`, 该任务尚未落地。本任务以 `require_session` (profile 已
//!   launch 且有活跃 `BrowserSession`) 为"已连接"等价条件; `ensure_auth`
//!   落地后在此处统一替换为登录态门禁, 调用点已集中在 `ensure_connected`。
//! - jieger `getShopLiveState` 来自 `tasks/liveControl`; 若 `live-launch`
//!   任务落地了统一的 `ShopLiveState`, 本模块类型应收敛到彼处, 此处实现仅
//!   作最小可用复刻 (中控页可见性判定 + 直播间链接提取)。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::Duration;

use cdp_driver::{TaskCancel, TaskPage, TaskResult};
use multizen_core::{MultizenError, Result};
use serde::{Deserialize, Serialize};

use super::TauriBrowserDriver;

/// jieger `HUIBO_VIDEO_LIST_URL` — 自传视频管理列表页。
pub const HUIBO_VIDEO_LIST_URL: &str = "https://zs.kwaixiaodian.com/page/record-live/upload";
/// 开播后复核用的小店中控页 (jieger `KUAISHOU_CONFIG.liveControlUrl`)。
pub const LIVE_CONTROL_URL: &str = "https://zs.kwaixiaodian.com/page/helper";

const TABLE_SELECTOR: &str = "table";
const NAV_TIMEOUT: Duration = Duration::from_secs(20);
const OP_TIMEOUT: Duration = Duration::from_secs(10);
const CLICK_TIMEOUT: Duration = Duration::from_secs(8);
const URL_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const STEP_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// jieger `HuiboVideo['status']`: `processing | success | failed | live`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HuiboVideoStatus {
    Processing,
    Success,
    Failed,
    Live,
}

/// jieger `src/types/entities.ts::HuiboVideo` — 逐字段等价, wire 为 camelCase。
///
/// 注意: `duration` 是展示字符串 (如 `"1小时41分"`), `id` 即 `replayId`
/// (只能经"去使用"点击从 URL 提取, 见模块文档)。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HuiboVideo {
    /// replayId ("去使用"点击后 URL 中的 `replayId=(\d+)`; 未能提取时为空串)
    pub id: String,
    /// 视频名称
    pub name: String,
    /// 上传时间 `YYYY.MM.DD HH:mm`
    pub upload_time: String,
    /// 片段数量
    pub segments: u32,
    /// 总大小 (如 `5.8GB`)
    pub size: String,
    /// 总时长展示串 (如 `1小时41分`) — 字符串, 不是秒数
    pub duration: String,
    pub status: HuiboVideoStatus,
    /// 回放可用次数
    pub usage_count: u32,
    /// 回放有效期 `YYYY-MM-DD HH:mm:ss`
    pub expiry_time: String,
    /// 挂车商品数
    pub goods_count: u32,
}

/// jieger `tasks/liveControl::ShopLiveStatus`: `unknown | offline | live`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShopLiveStatus {
    Unknown,
    Offline,
    Live,
}

/// jieger `tasks/liveControl::ShopLiveState`。
///
/// `profile_id` 即 Cloaksession 侧等价的 jieger `accountId`
/// (wire 名保持 `profileId`, 遵循本仓库 IPC camelCase 约定)。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopLiveState {
    pub profile_id: String,
    pub status: ShopLiveStatus,
    pub live_room_url: Option<String>,
    /// 毫秒 epoch (jieger `Date.now()`)
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 进行中的跟播任务取消句柄 (`profile_id` → cancel)。
/// IPC `cancel_huibo_task` 经此取消 `TaskPage` 协同 lease (取消只停止本地
/// 等待/后续 CDP 派发, 不撤回已送达浏览器的动作 — 与 `TaskPage` 语义一致)。
static HUIBO_CANCEL: LazyLock<StdMutex<HashMap<String, TaskCancel>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

fn register_cancel(profile_id: &str) -> TaskCancel {
    let cancel = TaskCancel::new();
    HUIBO_CANCEL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(profile_id.to_string(), cancel.clone());
    cancel
}

fn unregister_cancel(profile_id: &str) {
    HUIBO_CANCEL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(profile_id);
}

fn task_error(error: cdp_driver::TaskError) -> MultizenError {
    MultizenError::Cdp(format!("huibo-live: {error}"))
}

// ---------------------------------------------------------------------------
// 纯解析 (离线可测)
// ---------------------------------------------------------------------------

/// 从任意 URL 中提取 `replayId=(\d+)` (jieger: 点击"去使用"后 `page.url()` 匹配)。
pub fn extract_replay_id_from_url(url: &str) -> Option<String> {
    let marker = "replayId=";
    let start = url.find(marker)? + marker.len();
    let digits: String = url[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

/// jieger `isKuaishouControlPage`: URL 命中 `zs.kwaixiaodian.com` 即视为中控域。
pub fn is_huibo_control_url(url: &str) -> bool {
    url.contains("zs.kwaixiaodian.com")
}

/// 解析"片段数量：N / 总大小：X / 总时长：Y"信息串 (jieger `fetchVideoList` 内联正则)。
/// 缺失项回退为 (1, "", "")。
pub fn parse_video_info(info: &str) -> (u32, String, String) {
    fn after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        text.find(key).map(|i| &text[i + key.len()..])
    }
    let segments = after(info, "片段数量：")
        .and_then(|rest| {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u32>().ok()
        })
        .unwrap_or(1);
    // 总大小/总时长取值到下一个空白或 `|` 为止。
    fn token_after(text: &str, key: &str) -> String {
        after(text, key)
            .map(|rest| {
                rest.split(|c: char| c.is_whitespace() || c == '|')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default()
    }
    let size = token_after(info, "总大小：");
    let duration = token_after(info, "总时长：");
    (segments, size, duration)
}

/// 解析"回放可用次数N次 / 回放有效期至 YYYY-MM-DD HH:mm:ss"诊断串。
/// 缺失项回退为 (0, "")。
pub fn parse_video_diagnosis(diagnosis: &str) -> (u32, String) {
    let usage_count = diagnosis
        .find("回放可用次数")
        .and_then(|i| {
            let rest = &diagnosis[i + "回放可用次数".len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u32>().ok()
        })
        .unwrap_or(0);
    let expiry_time = diagnosis
        .find("回放有效期至")
        .map(|i| diagnosis[i + "回放有效期至".len()..].trim().to_string())
        .unwrap_or_default();
    (usage_count, expiry_time)
}

/// jieger 状态判定: 有"直播中"按钮 → live; 有"去使用"且状态含"处理成功" →
/// success; 状态含"失败" → failed; 其余 processing。
pub fn classify_video_status(
    has_live_button: bool,
    has_use_button: bool,
    status_text: &str,
) -> HuiboVideoStatus {
    if has_live_button {
        HuiboVideoStatus::Live
    } else if has_use_button && status_text.contains("处理成功") {
        HuiboVideoStatus::Success
    } else if status_text.contains("失败") {
        HuiboVideoStatus::Failed
    } else {
        HuiboVideoStatus::Processing
    }
}

/// 表格行转 `HuiboVideo` (`replay_id` 由调用方经点击副作用填入)。
/// jieger 列约定: td[1]=上传时间, td[2]=名称+信息串, td[3]=状态, td[4]=诊断,
/// td[5]=挂车商品数。
pub fn parse_video_row(
    cells: &[String],
    buttons: &[String],
    replay_id: String,
) -> Option<HuiboVideo> {
    if cells.len() < 5 {
        return None;
    }
    let upload_time = cells[0].trim().to_string();
    let name = cells[1].lines().next().unwrap_or("").trim().to_string();
    let (segments, size, duration) = parse_video_info(&cells[1]);
    let status_text = cells[2].trim();
    let (usage_count, expiry_time) = parse_video_diagnosis(&cells[3]);
    let goods_count = cells[4].trim().parse::<u32>().unwrap_or(0);
    let has_live_button = buttons.iter().any(|b| b.contains("直播中"));
    let has_use_button = buttons.iter().any(|b| b.contains("去使用"));
    Some(HuiboVideo {
        id: replay_id,
        name,
        upload_time,
        segments,
        size,
        duration,
        status: classify_video_status(has_live_button, has_use_button, status_text),
        usage_count,
        expiry_time,
        goods_count,
    })
}

// ---------------------------------------------------------------------------
// 页面 JS (Playwright `:has-text` 在 CDP `querySelector` 不可用, 一律用 JS
// 文本匹配; 行级操作按索引重新查询, 不持有跨导航的引用)
// ---------------------------------------------------------------------------

/// 一次性 dump 全表行 (cells 文本 + 行内按钮文本), 避免逐格 CDP 往返。
const ROW_DUMP_JS: &str = r#"(() => {
  const rows = Array.from(document.querySelectorAll('table tbody tr'));
  return rows.map((tr) => ({
    cells: Array.from(tr.querySelectorAll('td')).map((td) => td.innerText || ''),
    buttons: Array.from(tr.querySelectorAll('button')).map((b) => (b.innerText || '').trim()),
  }));
})()"#;

/// 点击第 `index` 行的"去使用"按钮 (返回是否点到)。
fn click_use_button_js(index: usize) -> String {
    format!(
        r#"(() => {{
  const rows = document.querySelectorAll('table tbody tr');
  const tr = rows[{index}];
  if (!tr) return false;
  const btn = Array.from(tr.querySelectorAll('button'))
    .find((b) => (b.innerText || '').includes('去使用'));
  if (!btn) return false;
  btn.click();
  return true;
}})()"#
    )
}

/// 按可见文本点击全页第一个匹配按钮 (下一步 / 去开播)。
fn click_button_by_text_js(text: &str) -> String {
    let want = serde_json::to_string(text).expect("serializing a string cannot fail");
    format!(
        r#"(() => {{
  const want = {want};
  const btn = Array.from(document.querySelectorAll('button'))
    .find((b) => (b.innerText || '').includes(want));
  if (!btn) return false;
  btn.click();
  return true;
}})()"#
    )
}

const CURRENT_URL_JS: &str = "location.href";

/// 中控页直播状态一次采样 (选择器来自 jieger `KS_SELECTORS.liveStatus`,
/// 文本按钮改 JS 匹配; 直播间链接取首个命中的 kuaishou live/www URL)。
const LIVE_STATE_JS: &str = r#"(() => {
  const vis = (el) => {
    if (!el) return false;
    const st = getComputedStyle(el);
    const r = el.getBoundingClientRect();
    return st.display !== 'none' && st.visibility !== 'hidden' &&
      st.visibility !== 'collapse' && r.width > 0 && r.height > 0;
  };
  const anyVis = (sel) => Array.from(document.querySelectorAll(sel)).some(vis);
  const btnVis = (want, e2e) => Array.from(document.querySelectorAll('button'))
    .some((el) => vis(el) && ((el.innerText || '').includes(want)));
  const attrs = Array.from(
    document.querySelectorAll('a[href],[data-href],[data-url],[data-link],[data-share-url]'))
    .flatMap((el) => [el.getAttribute('href'), el.getAttribute('data-href'),
      el.getAttribute('data-url'), el.getAttribute('data-link'),
      el.getAttribute('data-share-url')]).filter(Boolean);
  const text = ((document.body && document.body.innerText) || '') + '\n' + attrs.join('\n');
  const m = text.match(/https?:\/\/(?:live|www)\.kuaishou\.com\/[^\s"'<>\\]+/) ||
    text.match(/(?:live|www)\.kuaishou\.com\/[^\s"'<>\\]+/);
  return {
    living: anyVis('[data-e2e="live-status-on"], .status-live'),
    notLiving: anyVis('[data-e2e="live-status-off"], .status-offline'),
    startBtn: btnVis('开始直播') || anyVis('[data-e2e="start-live"]'),
    stopBtn: btnVis('结束直播') || anyVis('[data-e2e="stop-live"]'),
    liveRoom: m ? m[0] : null,
  };
})()"#;

#[derive(Deserialize)]
struct LiveStateSample {
    living: bool,
    #[serde(default)]
    #[allow(dead_code)]
    not_living: bool,
    #[serde(default)]
    start_btn: bool,
    #[serde(default)]
    stop_btn: bool,
    #[serde(default)]
    live_room: Option<String>,
}

fn classify_live_status(sample: &LiveStateSample) -> ShopLiveStatus {
    // jieger `getShopLiveState` 判定顺序: living/stop → live; start/notLiving → offline.
    if sample.living || sample.stop_btn {
        ShopLiveStatus::Live
    } else if sample.start_btn || sample.not_living {
        ShopLiveStatus::Offline
    } else {
        ShopLiveStatus::Unknown
    }
}

// ---------------------------------------------------------------------------
// TaskPage 流程
// ---------------------------------------------------------------------------

async fn current_url(page: &mut TaskPage<'_>) -> TaskResult<String> {
    let value = page.evaluate(CURRENT_URL_JS, OP_TIMEOUT).await?;
    Ok(value.as_str().unwrap_or("").to_string())
}

async fn navigate_to_video_list(page: &mut TaskPage<'_>) -> TaskResult<()> {
    page.navigate(HUIBO_VIDEO_LIST_URL, NAV_TIMEOUT).await?;
    page.wait_for_selector(
        TABLE_SELECTOR,
        cdp_driver::SelectorState::Attached,
        OP_TIMEOUT,
        POLL_INTERVAL,
    )
    .await
}

/// 轮询当前 URL 直到包含 `marker`, 返回完整 URL (超时即错, 不伪造成功)。
async fn wait_for_url(
    page: &mut TaskPage<'_>,
    marker: &str,
    timeout: Duration,
) -> TaskResult<String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let url = current_url(page).await?;
        if url.contains(marker) {
            return Ok(url);
        }
        if std::time::Instant::now() >= deadline {
            return Err(cdp_driver::TaskError::TimedOut);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[derive(Deserialize)]
struct RowDump {
    cells: Vec<String>,
    buttons: Vec<String>,
}

async fn dump_rows(page: &mut TaskPage<'_>) -> TaskResult<Vec<RowDump>> {
    let value = page.evaluate(ROW_DUMP_JS, OP_TIMEOUT).await?;
    serde_json::from_value(value)
        .map_err(|e| cdp_driver::TaskError::Driver(MultizenError::Cdp(format!("row dump: {e}"))))
}

/// jieger `fetchVideoList`: 导航到列表页 → 逐行解析 → 有"去使用"按钮的行
/// 点击进配置页从 URL 取 `replayId` 再返回列表页。
///
/// 点击导航是**预期副作用** (replayId 别无来源), 每行按索引重新定位,
/// 单行失败记 warn 语义 (id 留空) 并继续。
pub async fn fetch_video_list(page: &mut TaskPage<'_>) -> TaskResult<Vec<HuiboVideo>> {
    navigate_to_video_list(page).await?;
    let rows = dump_rows(page).await?;
    let mut videos = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let has_use_button = row.buttons.iter().any(|b| b.contains("去使用"));
        let mut replay_id = String::new();
        if has_use_button {
            let clicked = page
                .evaluate(&click_use_button_js(index), CLICK_TIMEOUT)
                .await?
                .as_bool()
                .unwrap_or(false);
            if clicked {
                match wait_for_url(page, "replayId=", URL_WAIT_TIMEOUT).await {
                    Ok(url) => {
                        replay_id = extract_replay_id_from_url(&url).unwrap_or_default();
                    }
                    Err(_) => {
                        // 取不到 replayId: 本行 id 留空, 返回列表继续下一行。
                    }
                }
                // 返回列表页 (jieger `page.goBack`), 等表格重新挂载。
                page.navigate(HUIBO_VIDEO_LIST_URL, NAV_TIMEOUT).await?;
                page.wait_for_selector(
                    TABLE_SELECTOR,
                    cdp_driver::SelectorState::Attached,
                    OP_TIMEOUT,
                    POLL_INTERVAL,
                )
                .await?;
            }
        }
        if let Some(video) = parse_video_row(&row.cells, &row.buttons, replay_id) {
            videos.push(video);
        }
        // cells 不足 5 列的行直接跳过 (jieger `try/catch` 逐行 warn 语义)。
    }
    Ok(videos)
}

/// jieger `startHuiboLiveFlow`: 列表页找 `replayId` 匹配行点"去使用" →
/// step=1 点"下一步" → step=2 点"去开播" → 固定等待后返回。
///
/// 尾部固定等待**不是成功证明** — 调用方必须随后用 `get_shop_live_state`
/// 复核中控页真实状态 (见 `huibo_start_live`)。
pub async fn start_huibo_live_flow(page: &mut TaskPage<'_>, replay_id: &str) -> TaskResult<()> {
    navigate_to_video_list(page).await?;
    let rows = dump_rows(page).await?;
    let mut found = false;
    for (index, row) in rows.iter().enumerate() {
        if !row.buttons.iter().any(|b| b.contains("去使用")) {
            continue;
        }
        let clicked = page
            .evaluate(&click_use_button_js(index), CLICK_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if !clicked {
            continue;
        }
        let url = wait_for_url(page, "replayId=", URL_WAIT_TIMEOUT).await?;
        if extract_replay_id_from_url(&url).as_deref() == Some(replay_id) {
            found = true;
            break;
        }
        // 非目标视频: 返回列表继续找 (jieger `goBack` 语义)。
        page.navigate(HUIBO_VIDEO_LIST_URL, NAV_TIMEOUT).await?;
        page.wait_for_selector(
            TABLE_SELECTOR,
            cdp_driver::SelectorState::Attached,
            OP_TIMEOUT,
            POLL_INTERVAL,
        )
        .await?;
    }
    if !found {
        return Err(cdp_driver::TaskError::Driver(MultizenError::Cdp(format!(
            "未找到replayId为{replay_id}的视频"
        ))));
    }
    // step=1 配置页点"下一步"。
    wait_for_url(page, "step=1", STEP_WAIT_TIMEOUT).await?;
    let next_clicked = page
        .evaluate(&click_button_by_text_js("下一步"), CLICK_TIMEOUT)
        .await?
        .as_bool()
        .unwrap_or(false);
    if !next_clicked {
        return Err(cdp_driver::TaskError::Driver(MultizenError::Cdp(
            "配置页未找到“下一步”按钮".into(),
        )));
    }
    // step=2 确认页点"去开播"。
    wait_for_url(page, "step=2", STEP_WAIT_TIMEOUT).await?;
    let start_clicked = page
        .evaluate(&click_button_by_text_js("去开播"), CLICK_TIMEOUT)
        .await?
        .as_bool()
        .unwrap_or(false);
    if !start_clicked {
        return Err(cdp_driver::TaskError::Driver(MultizenError::Cdp(
            "确认页未找到“去开播”按钮".into(),
        )));
    }
    // jieger `waitForTimeout(3000)`: 仅给页面跳转留时间, 不证明开播成功。
    tokio::time::sleep(Duration::from_secs(3)).await;
    Ok(())
}

/// jieger `platforms/kuaishou/liveControl::getShopLiveState` 最小复刻:
/// 中控页采样可见性 → live/offline/unknown, 附带直播间链接提取。
/// 非中控域 URL 返回 `unknown` (jieger 同语义, 不自动导航触发登录)。
pub async fn get_shop_live_state(
    page: &mut TaskPage<'_>,
    profile_id: &str,
) -> TaskResult<ShopLiveState> {
    let url = current_url(page).await?;
    if !is_huibo_control_url(&url) {
        return Ok(ShopLiveState {
            profile_id: profile_id.to_string(),
            status: ShopLiveStatus::Unknown,
            live_room_url: None,
            updated_at: chrono::Utc::now().timestamp_millis(),
            error: Some("页面未连接到快手中控".into()),
        });
    }
    let value = page.evaluate(LIVE_STATE_JS, OP_TIMEOUT).await?;
    let sample: LiveStateSample = serde_json::from_value(value).map_err(|e| {
        cdp_driver::TaskError::Driver(MultizenError::Cdp(format!("live state: {e}")))
    })?;
    Ok(ShopLiveState {
        profile_id: profile_id.to_string(),
        status: classify_live_status(&sample),
        live_room_url: sample.live_room.clone(),
        updated_at: chrono::Utc::now().timestamp_millis(),
        error: None,
    })
}

// ---------------------------------------------------------------------------
// Driver 方法 (TaskPage lease + cancel)
// ---------------------------------------------------------------------------

impl TauriBrowserDriver {
    /// jieger `connect` 的现阶段等价: profile 必须已 launch 且有活跃会话。
    /// `ks-platform-primitives::ensure_auth` (登录态门禁) 落地后在此替换。
    async fn ensure_connected(
        &self,
        profile_id: &str,
    ) -> Result<std::sync::Arc<cdp_driver::session::BrowserSession>> {
        self.require_session(profile_id).await.map_err(|e| {
            MultizenError::Launch(format!(
                "profile `{profile_id}` 未连接, 请先启动浏览器后再试: {e}"
            ))
        })
    }

    /// 在独立后台页上执行跟播闭包: 建页 → lease → 执行 → 关页 → 注销 cancel。
    async fn with_huibo_page<T>(
        &self,
        profile_id: &str,
        start_url: &str,
        f: impl AsyncFnOnce(&mut TaskPage<'_>) -> TaskResult<T>,
    ) -> Result<T> {
        let session = self.ensure_connected(profile_id).await?;
        let bound = session
            .new_bound_page(start_url)
            .await
            .map_err(|e| MultizenError::Cdp(format!("huibo-live: 新建页面失败: {e}")))?;
        let target = bound.target_id().to_string();
        drop(bound);
        let cancel = register_cancel(profile_id);
        let result = async {
            let mut task = session
                .task_page(&target, cancel.clone(), Duration::from_secs(10))
                .await
                .map_err(task_error)?;
            f(&mut task).await.map_err(task_error)
        }
        .await;
        unregister_cancel(profile_id);
        // 关页尽力而为, 不覆盖业务结果。
        session.close_page(&target).await.ok();
        result
    }

    /// jieger `getHuiboVideoList(accountId)`: ensure 连接 → 取自传视频列表。
    pub async fn huibo_video_list(&self, profile_id: &str) -> Result<Vec<HuiboVideo>> {
        self.with_huibo_page(profile_id, HUIBO_VIDEO_LIST_URL, async |page| {
            fetch_video_list(page).await
        })
        .await
    }

    /// jieger `startHuiboLive({accountId, replayId})`: 开播流程 →
    /// 中控页复核状态并返回 (3 秒等待本身不是成功证明, 以此状态为准)。
    pub async fn huibo_start_live(
        &self,
        profile_id: &str,
        replay_id: &str,
    ) -> Result<ShopLiveState> {
        if replay_id.trim().is_empty() {
            return Err(MultizenError::Config("replayId 不能为空".into()));
        }
        let owned_replay = replay_id.to_string();
        let owned_profile = profile_id.to_string();
        self.with_huibo_page(profile_id, HUIBO_VIDEO_LIST_URL, async |page| {
            start_huibo_live_flow(page, &owned_replay).await?;
            // 复核: 切到中控页再读真实状态 (jieger 在 session 主页读状态;
            // 此处任务页是独立后台页, 需先导航到中控域, 否则必为 unknown)。
            page.navigate(LIVE_CONTROL_URL, NAV_TIMEOUT).await?;
            get_shop_live_state(page, &owned_profile).await
        })
        .await
    }

    /// jieger `getShopLiveState(accountId)` (跟播侧复用; `live-launch`
    /// 若提供统一实现, 此处应收敛到彼处)。
    pub async fn huibo_shop_live_state(&self, profile_id: &str) -> Result<ShopLiveState> {
        let owned_profile = profile_id.to_string();
        self.with_huibo_page(profile_id, LIVE_CONTROL_URL, async |page| {
            page.navigate(LIVE_CONTROL_URL, NAV_TIMEOUT).await?;
            get_shop_live_state(page, &owned_profile).await
        })
        .await
    }

    /// 取消该 profile 进行中的跟播任务 (协同取消语义见模块文档)。
    pub fn huibo_cancel(&self, profile_id: &str) -> bool {
        HUIBO_CANCEL
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(profile_id)
            .map(|cancel| {
                cancel.cancel();
                true
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info() -> &'static str {
        "测试视频 片段数量：3 总大小：5.8GB | 总时长：1小时41分"
    }

    #[test]
    fn huibo_video_wire_shape_matches_jieger_entities() {
        // 真实字段断言: 有这 10 个, 无 thumbnail/title, duration 为字符串。
        let video = HuiboVideo {
            id: "12345".into(),
            name: "测试".into(),
            upload_time: "2026.10.01 12:00".into(),
            segments: 3,
            size: "5.8GB".into(),
            duration: "1小时41分".into(),
            status: HuiboVideoStatus::Success,
            usage_count: 5,
            expiry_time: "2026-12-31 23:59:59".into(),
            goods_count: 12,
        };
        let wire = serde_json::to_value(&video).unwrap();
        for key in [
            "id",
            "name",
            "uploadTime",
            "segments",
            "size",
            "duration",
            "status",
            "usageCount",
            "expiryTime",
            "goodsCount",
        ] {
            assert!(wire.get(key).is_some(), "missing wire field {key}");
        }
        assert!(wire.get("thumbnail").is_none());
        assert!(wire.get("title").is_none());
        assert!(wire.get("duration").unwrap().is_string());
        let back: HuiboVideo = serde_json::from_value(wire).unwrap();
        assert_eq!(back, video);
    }

    #[test]
    fn replay_id_extraction_matches_jieger_click_url() {
        assert_eq!(
            extract_replay_id_from_url(
                "https://zs.kwaixiaodian.com/page/record-live/config?replayId=98765&step=1"
            )
            .as_deref(),
            Some("98765")
        );
        assert!(extract_replay_id_from_url("https://zs.kwaixiaodian.com/page/helper").is_none());
        assert!(extract_replay_id_from_url("https://x/?replayId=").is_none());
    }

    #[test]
    fn video_info_parsing_matches_jieger_regex() {
        let (segments, size, duration) = parse_video_info(sample_info());
        assert_eq!(segments, 3);
        assert_eq!(size, "5.8GB");
        assert_eq!(duration, "1小时41分");
        // 缺失项回退。
        assert_eq!(
            parse_video_info("裸名称"),
            (1, String::new(), String::new())
        );
    }

    #[test]
    fn video_diagnosis_parsing() {
        let (count, expiry) =
            parse_video_diagnosis("诊断通过 回放可用次数5次 回放有效期至 2026-12-31 23:59:59");
        assert_eq!(count, 5);
        assert_eq!(expiry, "2026-12-31 23:59:59");
        assert_eq!(parse_video_diagnosis("诊断中"), (0, String::new()));
    }

    #[test]
    fn video_status_classification_matrix() {
        use HuiboVideoStatus as S;
        assert_eq!(classify_video_status(true, false, "直播中"), S::Live);
        assert_eq!(classify_video_status(false, true, "处理成功"), S::Success);
        assert_eq!(
            classify_video_status(false, true, "处理成功 有直播中?"),
            S::Success
        );
        assert_eq!(classify_video_status(false, false, "处理失败"), S::Failed);
        assert_eq!(classify_video_status(false, false, "处理中"), S::Processing);
        assert_eq!(classify_video_status(false, true, "处理中"), S::Processing);
    }

    #[test]
    fn video_row_parsing_uses_jieger_column_contract() {
        let cells = vec![
            "2026.10.01 12:00".to_string(),
            "带货回放\n片段数量：2 总大小：1.2GB | 总时长：30分钟".to_string(),
            "处理成功".to_string(),
            "回放可用次数3次 回放有效期至 2026-11-01 00:00:00".to_string(),
            "7".to_string(),
        ];
        let video = parse_video_row(&cells, &["去使用".to_string()], "4242".to_string()).unwrap();
        assert_eq!(video.id, "4242");
        assert_eq!(video.name, "带货回放");
        assert_eq!(video.segments, 2);
        assert_eq!(video.duration, "30分钟");
        assert_eq!(video.status, HuiboVideoStatus::Success);
        assert_eq!(video.goods_count, 7);
        // 列不足直接跳过。
        assert!(parse_video_row(&cells[..2], &[], String::new()).is_none());
    }

    #[test]
    fn shop_live_state_wire_shape() {
        let state = ShopLiveState {
            profile_id: "p1".into(),
            status: ShopLiveStatus::Live,
            live_room_url: Some("https://live.kuaishou.com/u/1".into()),
            updated_at: 1_700_000_000_000,
            error: None,
        };
        let wire = serde_json::to_value(&state).unwrap();
        assert_eq!(wire["profileId"], "p1");
        assert_eq!(wire["status"], "live");
        assert!(wire.get("error").is_none());
    }

    #[test]
    fn live_state_classification_follows_jieger_order() {
        let live = LiveStateSample {
            living: true,
            not_living: false,
            start_btn: false,
            stop_btn: false,
            live_room: None,
        };
        assert_eq!(classify_live_status(&live), ShopLiveStatus::Live);
        let offline = LiveStateSample {
            living: false,
            not_living: true,
            start_btn: false,
            stop_btn: false,
            live_room: None,
        };
        assert_eq!(classify_live_status(&offline), ShopLiveStatus::Offline);
        let unknown = LiveStateSample {
            living: false,
            not_living: false,
            start_btn: false,
            stop_btn: false,
            live_room: None,
        };
        assert_eq!(classify_live_status(&unknown), ShopLiveStatus::Unknown);
    }

    #[test]
    fn control_url_check_is_domain_level_like_jieger() {
        assert!(is_huibo_control_url(
            "https://zs.kwaixiaodian.com/page/record-live/upload"
        ));
        assert!(!is_huibo_control_url("https://login.kwaixiaodian.com/"));
    }
}
