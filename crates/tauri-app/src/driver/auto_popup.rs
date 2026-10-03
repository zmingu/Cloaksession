//! jieger 自动弹窗 / 自动弹品 (`autoPopUp`) 的 Rust 移植.
//!
//! 源码对照（路径相对 `F:/jieger`，实现在场不等于线上验证通过）：
//! - `electron/main/tasks/autoPopUp/index.ts` — 队列、间隔调度、重试、启停
//! - `electron/main/tasks/autoPopUp/goodsKnowledge.ts` — 商品扫描与差异
//! - `electron/main/tasks/autoPopUp/shortcutManager.ts` — 快捷键注册表
//! - `electron/main/tasks/retry.ts` — `runWithRetry`
//! - `electron/main/platforms/kuaishou/goodsList.ts` — 商品列表读写
//! - `electron/main/platforms/kuaishou/selectors.ts` — 选择器表
//!
//! # 约束（来自任务要求，原样执行）
//!
//! - `explainGoods` 只在中控页、以主播身份执行：启动与单次讲解前检查业务
//!   scope（金牛 / 非小店绑定直接拒绝，见 [`kuaishou_identity_allowed`]），
//!   每次动作前只绑定中控台 origin 的页面 target，不存在则拒绝而不是任选一页。
//! - 选择器沿用 jieger 占位骨架，**必须用真实中控台页面实地校准后才能线上用**
//!  （见 [`selectors`] 上的警告）。
//! - 快捷键需要用户明确授权：本模块只维护“账号 → 快捷键 → 商品”的注册表与
//!   触发回调路径，OS 级全局注册（`tauri_plugin_global_shortcut`）是有意的
//!   未来接线点，未经用户授权不得静默注册（见 [`register_shortcuts`]）。
//! - 重试有上限：次数钳制在 1..=10（默认 3），延迟钳制在 0..=60_000ms
//!  （默认 1_000ms），取消后不再发起新动作。
//!
//! # 线程与运行时归属
//!
//! 本文件经 `crate::auto_popup`（`#[path]` 指向本文件）挂入 crate，
//! **不改动 `driver.rs`**（并行工作流归属约束）。只使用
//! `TauriBrowserDriver` 的公开 API（`registry()`、业务状态查询），
//! 不触碰 launcher 线程私有状态。运行态放在模块级静态表里，
//! 定时循环持有 `Arc<ProfileRegistry>`（`'static`），因此不需要 `Arc<Self>`。

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use cdp_driver::{TaskCancel, TaskPage};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::driver::TauriBrowserDriver;
use crate::registry::ProfileRegistry;

// ---------------------------------------------------------------------------
// 选择器（占位骨架，来自 jieger `selectors.ts` 的 goods 段）
// ---------------------------------------------------------------------------

/// 快手商品列表选择器。
///
/// ⚠️ 真号校准警告：以下选择器是 jieger 留下的占位骨架，
/// 必须基于真实快手中控台页面实地测绘（Playwright Inspector / DevTools）
/// 校准后才能用于线上讲解。测绘命令：
/// `npx playwright codegen https://live.kuaishou.com/shop/live`。
pub mod selectors {
    /// 商品列表容器。
    pub const GOODS_LIST: &str = "[data-e2e=\"goods-list\"], .goods-list";
    /// 单个商品 item。
    pub const GOODS_ITEM: &str = "[data-e2e=\"goods-item\"], .goods-item";
    /// 商品序号。
    pub const ITEM_SERIAL: &str = ".serial-no, [data-e2e=\"goods-serial\"]";
    /// 商品标题。
    pub const ITEM_TITLE: &str = ".title, [data-e2e=\"goods-title\"]";
    /// 商品价格。
    pub const ITEM_PRICE: &str = ".price, [data-e2e=\"goods-price\"]";
    /// 弹品 / 讲解按钮（item 作用域内查找）。
    pub const EXPLAIN_BUTTON: &str = "button:has-text(\"讲解\"), [data-e2e=\"goods-explain\"]";
    /// 取消讲解按钮（页面作用域内查找）。
    pub const CANCEL_EXPLAIN_BUTTON: &str =
        "button:has-text(\"取消讲解\"), [data-e2e=\"goods-cancel-explain\"]";
}

// ---------------------------------------------------------------------------
// 中控页 origin 门禁
// ---------------------------------------------------------------------------

/// 被视作“中控页”的 https host 白名单（jieger `getSession(accountId).page`
/// 即已连中控台的页面；此处显式校验 origin，不存在则拒绝）。
///
/// 同选择器一样，真号联调时按实际中控台域名校准。
pub const CONSOLE_HOSTS: &[&str] = &["live.kuaishou.com", "s.kwaixiaodian.com"];

/// 长表保护：拒绝超长 / 含 userinfo 的 URL（与 identity 的 `shop_url` 同级防御）。
pub fn is_console_url(raw: &str) -> bool {
    if raw.len() > 8192
        || raw.split_once("://").is_some_and(|(_, v)| {
            v.split(['/', '?', '#'])
                .next()
                .is_some_and(|v| v.contains('@'))
        })
    {
        return false;
    }
    reqwest::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some_and(|h| CONSOLE_HOSTS.contains(&h))
            && u.port_or_known_default() == Some(443)
            && u.username().is_empty()
            && u.password().is_none()
    })
}

// ---------------------------------------------------------------------------
// 配置与执行队列（对 `autoPopUp/index.ts`）
// ---------------------------------------------------------------------------

/// 增强商品队列项：单个商品可重复多次、可覆盖独立间隔。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPopUpGoodsItem {
    pub id: String,
    #[serde(default)]
    pub repeat_count: Option<u32>,
    #[serde(default)]
    pub interval: Option<[f64; 2]>,
}

/// 失败重试配置（上限见模块文档）。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPopUpRetryConfig {
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,
}

/// 自动弹品配置。`goods_items` 优先，缺席时回退到 `goods_ids`
///（兼容 jieger 旧配置）。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPopUpConfig {
    #[serde(default)]
    pub goods_ids: Option<Vec<String>>,
    pub interval: [f64; 2],
    #[serde(default)]
    pub per_goods_interval: Option<HashMap<String, [f64; 2]>>,
    #[serde(default)]
    pub goods_items: Option<Vec<AutoPopUpGoodsItem>>,
    #[serde(default)]
    pub random: bool,
    #[serde(default)]
    pub retry: Option<AutoPopUpRetryConfig>,
}

/// 配置热更新补丁：`Some` 字段覆盖运行中配置，`None` 保持不变。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPopUpConfigPatch {
    #[serde(default)]
    pub goods_ids: Option<Vec<String>>,
    #[serde(default)]
    pub interval: Option<[f64; 2]>,
    #[serde(default)]
    pub per_goods_interval: Option<HashMap<String, [f64; 2]>>,
    #[serde(default)]
    pub goods_items: Option<Vec<AutoPopUpGoodsItem>>,
    #[serde(default)]
    pub random: Option<bool>,
    #[serde(default)]
    pub retry: Option<AutoPopUpRetryConfig>,
}

/// 展开后的单个待讲解项（已应用重复次数与 per-goods 间隔）。
#[derive(Clone, Debug)]
pub struct QueueItem {
    pub goods_id: String,
    pub interval: [f64; 2],
}

/// 间隔合法：两端有限、`min >= 1`、`max >= min`（单位：秒）。
pub fn is_valid_interval(range: Option<[f64; 2]>) -> bool {
    match range {
        Some([min, max]) => min.is_finite() && max.is_finite() && min >= 1.0 && max >= min,
        None => false,
    }
}

/// 重复次数钳制到 1..=1000（对 `index.ts::clampRepeatCount`）。
pub fn clamp_repeat_count(value: Option<u32>) -> u32 {
    value.map(|n| n.clamp(1, 1000)).unwrap_or(1)
}

fn resolve_item_interval(
    config: &AutoPopUpConfig,
    goods_id: &str,
    item_interval: Option<[f64; 2]>,
) -> [f64; 2] {
    if is_valid_interval(item_interval) {
        return item_interval.unwrap_or(config.interval);
    }
    if let Some(overrides) = config.per_goods_interval.as_ref() {
        if let Some(range) = overrides.get(goods_id) {
            if is_valid_interval(Some(*range)) {
                return *range;
            }
        }
    }
    config.interval
}

/// 构造执行队列：`goods_items` 非空时优先展开（重复 + 独立间隔），
/// 否则回退到 `goods_ids` + 全局/per-goods 间隔。
pub fn build_execution_queue(config: &AutoPopUpConfig) -> Vec<QueueItem> {
    let mut queue = Vec::new();
    let items: Vec<&AutoPopUpGoodsItem> = config
        .goods_items
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|item| !item.id.trim().is_empty())
        .collect();
    if !items.is_empty() {
        for item in items {
            let goods_id = item.id.trim().to_string();
            let repeat = clamp_repeat_count(item.repeat_count) as usize;
            let interval = resolve_item_interval(config, &goods_id, item.interval);
            for _ in 0..repeat {
                queue.push(QueueItem {
                    goods_id: goods_id.clone(),
                    interval,
                });
            }
        }
        return queue;
    }
    if let Some(ids) = config.goods_ids.as_deref() {
        for goods_id in ids.iter().map(|id| id.trim()).filter(|id| !id.is_empty()) {
            queue.push(QueueItem {
                goods_id: goods_id.to_string(),
                interval: resolve_item_interval(config, goods_id, None),
            });
        }
    }
    queue
}

/// 校验配置：全局间隔合法且队列非空。
pub fn validate_config(config: &AutoPopUpConfig) -> Result<(), String> {
    if !is_valid_interval(Some(config.interval)) {
        return Err("间隔范围无效（需要 [min, max]，min >= 1 且 max >= min，单位秒）".into());
    }
    if build_execution_queue(config).is_empty() {
        return Err("必须提供至少一个商品序号".into());
    }
    Ok(())
}

/// 从 `[min, max]`（秒）随机取一个等待时长（对 `index.ts::pickInterval`）。
pub fn pick_interval(range: [f64; 2]) -> Duration {
    let secs = rand::thread_rng().gen_range(range[0]..=range[1]);
    Duration::from_millis((secs * 1000.0).max(0.0) as u64)
}

// ---------------------------------------------------------------------------
// 重试（对 `tasks/retry.ts::runWithRetry`）
// ---------------------------------------------------------------------------

/// 重试次数归一化：缺省 3，钳制 1..=10。
pub fn normalize_max_retries(value: Option<u32>) -> u32 {
    value.map(|n| n.clamp(1, 10)).unwrap_or(3)
}

/// 重试延迟归一化：缺省 1_000ms，钳制 0..=60_000ms。
pub fn normalize_retry_delay_ms(value: Option<u64>) -> u64 {
    value.map(|n| n.min(60_000)).unwrap_or(1000)
}

#[derive(Clone, Debug)]
pub struct RetryOutcome<T> {
    pub ok: bool,
    pub value: Option<T>,
    pub error: Option<String>,
    pub attempts: usize,
}

/// 带上限的重试。`should_continue` 返回 false（任务已停止 / 已取消）时
/// 立刻退出且不再发起新动作；延迟等待按 100ms 切片，可被中断。
///
/// `operation` 返回的 future 不得借用闭包环境（调用方如需借用，
/// 见 [`explain_with_retry`] 的页内循环写法）。
pub async fn run_with_retry<T, F, Fut>(
    mut operation: F,
    max_retries: Option<u32>,
    retry_delay_ms: Option<u64>,
    should_continue: Option<&(dyn Fn() -> bool + Sync)>,
) -> RetryOutcome<T>
where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let max = normalize_max_retries(max_retries) as usize;
    let delay = normalize_retry_delay_ms(retry_delay_ms);
    let continued = || should_continue.is_none_or(|f| f());
    let mut last_error: Option<String> = None;

    for attempt in 1..=max {
        if !continued() {
            return RetryOutcome {
                ok: false,
                value: None,
                error: Some("任务已停止".into()),
                attempts: attempt - 1,
            };
        }
        match operation(attempt).await {
            Ok(value) => {
                return RetryOutcome {
                    ok: true,
                    value: Some(value),
                    error: None,
                    attempts: attempt,
                };
            }
            Err(error) => {
                last_error = Some(error);
                if attempt >= max {
                    break;
                }
                if !sleep_retry_delay(delay, &continued).await {
                    return RetryOutcome {
                        ok: false,
                        value: None,
                        error: Some("任务已停止".into()),
                        attempts: attempt,
                    };
                }
            }
        }
    }
    RetryOutcome {
        ok: false,
        value: None,
        error: last_error.or_else(|| Some("重试执行失败".into())),
        attempts: max,
    }
}

/// 重试延迟等待（100ms 切片，可被 `continued() == false` 中断）。
/// 返回 false 表示等待期间任务已停止。
async fn sleep_retry_delay(delay_ms: u64, continued: &(dyn Fn() -> bool + Sync)) -> bool {
    if delay_ms == 0 {
        return continued();
    }
    let deadline = Instant::now() + Duration::from_millis(delay_ms);
    while Instant::now() < deadline {
        if !continued() {
            return false;
        }
        let rest = deadline.saturating_duration_since(Instant::now());
        tokio::time::sleep(rest.min(Duration::from_millis(100))).await;
    }
    continued()
}

// ---------------------------------------------------------------------------
// 商品读写（对 `platforms/kuaishou/goodsList.ts`）
// ---------------------------------------------------------------------------

/// 商品行：序号 / 标题 / 价格（空字符串归一为 `None`）。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GoodsInfo {
    pub serial: String,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "empty_as_none")]
    pub price: Option<String>,
}

fn empty_as_none<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(d)?.unwrap_or_default();
    let value = value.trim();
    Ok((!value.is_empty()).then(|| value.to_string()))
}

#[derive(Deserialize)]
struct ActionResult {
    ok: bool,
    error: Option<String>,
}

const EVAL_TIMEOUT: Duration = Duration::from_secs(8);
const CLICK_TIMEOUT: Duration = Duration::from_secs(5);
const VISIBLE_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(150);
const PAGE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

fn js_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

/// `fetchGoodsList`：一次 `evaluate` 提取全部商品行（serial/title/price）。
pub async fn fetch_goods_list(page: &mut TaskPage<'_>) -> Result<Vec<GoodsInfo>, String> {
    let item = js_string(selectors::GOODS_ITEM);
    let serial = js_string(selectors::ITEM_SERIAL);
    let title = js_string(selectors::ITEM_TITLE);
    let price = js_string(selectors::ITEM_PRICE);
    let expression = format!(
        r#"(() => {{
        const text = (root, sel) => (root.querySelector(sel)?.textContent ?? "").trim();
        return Array.from(document.querySelectorAll({item}))
            .map((el) => ({{ serial: text(el, {serial}), title: text(el, {title}), price: text(el, {price}) }}))
            .filter((g) => g.serial);
    }})()"#
    );
    let value = page
        .evaluate(&expression, EVAL_TIMEOUT)
        .await
        .map_err(|e| format!("读取商品列表失败：{e}"))?;
    serde_json::from_value(value).map_err(|e| format!("商品列表结构无效：{e}"))
}

/// 可见性探针 JS：按关键字找 item，可见则 `scrollIntoView` 并返回 true。
fn visible_probe_expression(keyword: &str) -> String {
    let item = js_string(selectors::GOODS_ITEM);
    let keyword = js_string(keyword);
    format!(
        r#"(() => {{
        const keyword = {keyword};
        const item = Array.from(document.querySelectorAll({item}))
            .find((el) => (el.textContent ?? "").includes(keyword));
        if (!item || !item.isConnected) return false;
        const style = getComputedStyle(item);
        const rect = item.getBoundingClientRect();
        const visible = style.display !== "none" && style.visibility !== "hidden"
            && style.visibility !== "collapse" && rect.width > 0 && rect.height > 0;
        if (!visible) return false;
        item.scrollIntoView({{ block: "center" }});
        return true;
    }})()"#
    )
}

/// `waitForGoodsVisible`：150ms 轮询 + `scrollIntoView`，默认 15s 超时。
/// 超时返回 `false`（不对调用方报错，由 `explain_goods` 转为可读错误）。
pub async fn wait_for_goods_visible(
    page: &mut TaskPage<'_>,
    keyword: &str,
    timeout: Duration,
    cancel: &TaskCancel,
) -> bool {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return false;
    }
    let expression = visible_probe_expression(keyword);
    let deadline = Instant::now() + timeout.max(Duration::from_secs(1));
    loop {
        if cancel.is_cancelled() {
            return false;
        }
        // 页面刷新 / 列表重绘时忽略单次失败，重试（对 jieger 的 catch 重试）。
        if let Ok(value) = page.evaluate(&expression, EVAL_TIMEOUT).await {
            if value.as_bool() == Some(true) {
                return true;
            }
        }
        if Instant::now() >= deadline || cancel.is_cancelled() {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// `explainGoods`：等可见 → 点击 item 作用域内的讲解按钮。
pub async fn explain_goods(
    page: &mut TaskPage<'_>,
    serial: &str,
    cancel: &TaskCancel,
) -> Result<(), String> {
    let serial = serial.trim();
    if serial.is_empty() {
        return Err("商品序号为空".into());
    }
    if !wait_for_goods_visible(page, serial, VISIBLE_TIMEOUT, cancel).await {
        if cancel.is_cancelled() {
            return Err("任务已停止".into());
        }
        return Err(format!("商品不可见或等待超时：{serial}"));
    }
    let item = js_string(selectors::GOODS_ITEM);
    let keyword = js_string(serial);
    // `:has-text` 非原生 CSS，先用 `data-e2e` 做原生回退，再试原始选择器。
    let fallback = js_string("[data-e2e=\"goods-explain\"]");
    let button = js_string(selectors::EXPLAIN_BUTTON);
    let expression = format!(
        r#"(() => {{
        const item = Array.from(document.querySelectorAll({item}))
            .find((el) => (el.textContent ?? "").includes({keyword}));
        if (!item || !item.isConnected) return {{ ok: false, error: "goods_not_found" }};
        let btn = null;
        try {{ btn = item.querySelector({fallback}); }} catch (_) {{}}
        if (!btn) {{ try {{ btn = item.querySelector({button}); }} catch (_) {{}} }}
        if (!btn || !btn.isConnected) return {{ ok: false, error: "explain_button_missing" }};
        btn.scrollIntoView({{ block: "center" }});
        btn.click();
        return {{ ok: true }};
    }})()"#
    );
    let value = page
        .evaluate(&expression, CLICK_TIMEOUT)
        .await
        .map_err(|e| format!("讲解点击失败：{e}"))?;
    let result: ActionResult =
        serde_json::from_value(value).map_err(|e| format!("讲解返回结构无效：{e}"))?;
    if result.ok {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "讲解点击失败".into()))
    }
}

/// 带重试的讲解。`run_with_retry` 的闭包不能借用 `&mut TaskPage`
///（返回 future 的借用无法用泛型表达），因此页操作在此处内联同一套
/// 上限 / 中断语义，而不是再包一层泛型闭包。
pub async fn explain_with_retry(
    page: &mut TaskPage<'_>,
    serial: &str,
    cancel: &TaskCancel,
    max_retries: Option<u32>,
    retry_delay_ms: Option<u64>,
) -> RetryOutcome<()> {
    let max = normalize_max_retries(max_retries) as usize;
    let delay = normalize_retry_delay_ms(retry_delay_ms);
    let mut last_error: Option<String> = None;
    for attempt in 1..=max {
        if cancel.is_cancelled() {
            return stopped_outcome(attempt - 1);
        }
        match explain_goods(page, serial, cancel).await {
            Ok(()) => {
                return RetryOutcome {
                    ok: true,
                    value: Some(()),
                    error: None,
                    attempts: attempt,
                };
            }
            Err(error) => {
                last_error = Some(error);
                if attempt >= max {
                    break;
                }
                if !sleep_retry_delay(delay, &|| !cancel.is_cancelled()).await {
                    return stopped_outcome(attempt);
                }
            }
        }
    }
    RetryOutcome {
        ok: false,
        value: None,
        error: last_error.or_else(|| Some("重试执行失败".into())),
        attempts: max,
    }
}

fn stopped_outcome<T>(attempts: usize) -> RetryOutcome<T> {
    RetryOutcome {
        ok: false,
        value: None,
        error: Some("任务已停止".into()),
        attempts,
    }
}

/// `cancelExplainGoods`：点击取消讲解按钮。
pub async fn cancel_explain_goods(page: &mut TaskPage<'_>) -> Result<(), String> {
    let fallback = js_string("[data-e2e=\"goods-cancel-explain\"]");
    let button = js_string(selectors::CANCEL_EXPLAIN_BUTTON);
    let expression = format!(
        r#"(() => {{
        let btn = null;
        try {{ btn = document.querySelector({fallback}); }} catch (_) {{}}
        if (!btn) {{ try {{ btn = document.querySelector({button}); }} catch (_) {{}} }}
        if (!btn || !btn.isConnected) return {{ ok: false, error: "cancel_button_missing" }};
        btn.click();
        return {{ ok: true }};
    }})()"#
    );
    let value = page
        .evaluate(&expression, CLICK_TIMEOUT)
        .await
        .map_err(|e| format!("取消讲解失败：{e}"))?;
    let result: ActionResult =
        serde_json::from_value(value).map_err(|e| format!("取消讲解返回结构无效：{e}"))?;
    if result.ok {
        Ok(())
    } else {
        Err(result.error.unwrap_or_else(|| "取消讲解失败".into()))
    }
}

// ---------------------------------------------------------------------------
// 商品知识扫描（对 `goodsKnowledge.ts` 的扫描与差异部分）
// ---------------------------------------------------------------------------

/// 已入库的商品知识（标题 / 价格）。写入闭环（jieger 的 `applyCandidates`）
/// 归 `shop-product-script` 所有，本模块只产出候选与差异。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodsKnowledge {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub price: Option<String>,
}

/// 商品知识来源。由 `shop-product-script` 实现（DB 归属在该任务）；
/// 本模块的命令层在来源就绪前使用 [`EmptyKnowledgeSource`]。
pub trait ProductScriptSource: Send + Sync {
    fn goods_knowledge(&self, account_id: &str, goods_id: &str) -> Option<GoodsKnowledge>;
}

/// 尚无知识库实现时的来源：永远返回 `None`（只产候选、无差异）。
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyKnowledgeSource;

impl ProductScriptSource for EmptyKnowledgeSource {
    fn goods_knowledge(&self, _account_id: &str, _goods_id: &str) -> Option<GoodsKnowledge> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KnowledgeField {
    Title,
    Price,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanDiff {
    pub goods_id: String,
    pub field: KnowledgeField,
    pub current: Option<String>,
    pub candidate: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned_count: usize,
    pub diffs: Vec<ScanDiff>,
    pub candidates: HashMap<String, GoodsKnowledge>,
}

/// `scanGoodsKnowledge`：以页面列表为候选，与来源中的存量逐项比对产出差异。
pub fn scan_goods_knowledge<S: ProductScriptSource + ?Sized>(
    source: &S,
    account_id: &str,
    goods: &[GoodsInfo],
) -> ScanReport {
    let mut diffs = Vec::new();
    let mut candidates = HashMap::new();
    for item in goods.iter().filter(|g| !g.serial.trim().is_empty()) {
        let candidate = GoodsKnowledge {
            title: item.title.clone(),
            price: item.price.clone(),
        };
        candidates.insert(item.serial.clone(), candidate.clone());
        let Some(existing) = source.goods_knowledge(account_id, &item.serial) else {
            continue;
        };
        if let Some(title) = candidate.title.as_deref() {
            if existing.title.as_deref() != Some(title) {
                diffs.push(ScanDiff {
                    goods_id: item.serial.clone(),
                    field: KnowledgeField::Title,
                    current: existing.title.clone(),
                    candidate: Some(title.to_string()),
                });
            }
        }
        if let Some(price) = candidate.price.as_deref() {
            if existing.price.as_deref() != Some(price) {
                diffs.push(ScanDiff {
                    goods_id: item.serial.clone(),
                    field: KnowledgeField::Price,
                    current: existing.price.clone(),
                    candidate: Some(price.to_string()),
                });
            }
        }
    }
    ScanReport {
        scanned_count: candidates.len(),
        diffs,
        candidates,
    }
}

// ---------------------------------------------------------------------------
// 快捷键注册表（对 `shortcutManager.ts`）
// ---------------------------------------------------------------------------

/// 快捷键配置：accelerator → 商品序号（如 `"CommandOrControl+1" → "001"`）。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShortcutConfig {
    #[serde(default)]
    pub bindings: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShortcutFailure {
    pub accelerator: String,
    pub error: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRegisterResult {
    pub ok: bool,
    pub registered: Vec<String>,
    pub failed: Vec<ShortcutFailure>,
}

fn shortcut_table() -> &'static StdMutex<HashMap<String, HashMap<String, String>>> {
    static TABLE: OnceLock<StdMutex<HashMap<String, HashMap<String, String>>>> = OnceLock::new();
    TABLE.get_or_init(|| StdMutex::new(HashMap::new()))
}

/// 注册快捷键（jieger `registerShortcuts` 语义）。
///
/// 只记录“账号 → 快捷键 → 商品”映射并做占用校验，不做 OS 级全局注册：
/// 全局快捷键需要用户明确授权，由 `tauri_plugin_global_shortcut` 在应用层
/// 接线（有意的未来工作点，不在本模块静默完成）。触发路径见
/// [`TauriBrowserDriver::auto_popup_trigger_shortcut`]
///（应用层快捷键回调 / 手动触发共用）。
pub fn register_shortcuts(profile_id: &str, config: ShortcutConfig) -> ShortcutRegisterResult {
    unregister_shortcuts(profile_id);
    let mut registered = Vec::new();
    let mut failed = Vec::new();
    {
        let mut table = shortcut_table().lock().unwrap();
        for (accelerator, goods_id) in &config.bindings {
            if accelerator.trim().is_empty() || goods_id.trim().is_empty() {
                continue;
            }
            let occupied = table
                .iter()
                .any(|(owner, bindings)| owner != profile_id && bindings.contains_key(accelerator));
            if occupied {
                failed.push(ShortcutFailure {
                    accelerator: accelerator.clone(),
                    error: "快捷键可能已被其他账号或其他应用占用".into(),
                });
                continue;
            }
            table
                .entry(profile_id.to_string())
                .or_default()
                .insert(accelerator.clone(), goods_id.clone());
            registered.push(accelerator.clone());
        }
    }
    ShortcutRegisterResult {
        ok: !registered.is_empty(),
        registered,
        failed,
    }
}

/// 注销该账号的全部快捷键（jieger `unregisterShortcuts`）。
pub fn unregister_shortcuts(profile_id: &str) {
    shortcut_table().lock().unwrap().remove(profile_id);
}

/// 快捷键触发：返回绑定的商品序号（调用方随后执行讲解）。
/// OS 快捷键回调与手动触发共用此入口。
pub fn lookup_shortcut(profile_id: &str, accelerator: &str) -> Option<String> {
    shortcut_table()
        .lock()
        .unwrap()
        .get(profile_id)
        .and_then(|bindings| bindings.get(accelerator))
        .cloned()
}

// ---------------------------------------------------------------------------
// 运行态与广播（对 `runningTasks` + `broadcast(CHANNELS…)`）
// ---------------------------------------------------------------------------

/// 运行状态快照（命令层返回 + 事件广播载荷）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPopUpStatus {
    pub profile_id: String,
    pub running: bool,
    pub queue_len: usize,
    pub last_goods_id: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

/// 广播事件种类。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopupEventKind {
    Started,
    Stopped,
    Explained,
    ExplainFailed,
    ConfigUpdated,
    ShortcutTriggered,
}

/// 广播事件（tokio channel + Tauri `auto-popup:event` 共用结构）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPopUpEvent {
    pub profile_id: String,
    pub kind: PopupEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goods_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

struct RunningTask {
    cancel: TaskCancel,
    config: AutoPopUpConfig,
    queue: Vec<QueueItem>,
    index: usize,
    random: bool,
    last_goods_id: Option<String>,
    last_error: Option<String>,
}

fn task_table() -> &'static StdMutex<HashMap<String, RunningTask>> {
    static TABLE: OnceLock<StdMutex<HashMap<String, RunningTask>>> = OnceLock::new();
    TABLE.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn popup_bus() -> &'static tokio::sync::broadcast::Sender<AutoPopUpEvent> {
    static BUS: OnceLock<tokio::sync::broadcast::Sender<AutoPopUpEvent>> = OnceLock::new();
    BUS.get_or_init(|| tokio::sync::broadcast::channel(128).0)
}

/// 订阅运行事件。应用层（`lib.rs` 桥接任务）用它转发为 Tauri 事件。
pub fn subscribe() -> tokio::sync::broadcast::Receiver<AutoPopUpEvent> {
    popup_bus().subscribe()
}

fn emit(event: AutoPopUpEvent) {
    let _ = popup_bus().send(event);
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn status_of(profile_id: &str, task: Option<&RunningTask>) -> AutoPopUpStatus {
    match task {
        Some(task) => AutoPopUpStatus {
            profile_id: profile_id.to_string(),
            running: true,
            queue_len: task.queue.len(),
            last_goods_id: task.last_goods_id.clone(),
            last_error: task.last_error.clone(),
            updated_at: now_rfc3339(),
        },
        None => AutoPopUpStatus {
            profile_id: profile_id.to_string(),
            running: false,
            queue_len: 0,
            last_goods_id: None,
            last_error: None,
            updated_at: now_rfc3339(),
        },
    }
}

impl TauriBrowserDriver {
    /// 取运行环境的会话；不自动启动（与 `require_session` 同契约）。
    async fn popup_session(
        &self,
        profile_id: &str,
    ) -> Result<Arc<cdp_driver::session::BrowserSession>, String> {
        self.registry().get(profile_id).await.ok_or_else(|| {
            format!("环境 `{profile_id}` 未运行；请先启动对应环境再使用自动弹品")
        })
    }

    /// 主播身份门禁：金牛 scope / 非小店业务绑定直接拒绝。
    async fn require_console_scope(&self, profile_id: &str) -> Result<(), String> {
        let state = self
            .business_accounts_profile_state(profile_id)
            .await
            .map_err(|e| e.to_string())?;
        if multizen_core::kuaishou_identity_allowed(&state) {
            Ok(())
        } else {
            Err("当前环境非主播小店业务（金牛或非小店绑定），已拒绝自动弹品".into())
        }
    }

    /// 在会话中定位中控页并加任务锁。不存在中控页则拒绝，不任选页面。
    async fn console_task_page<'a>(
        session: &'a Arc<cdp_driver::session::BrowserSession>,
        cancel: &TaskCancel,
    ) -> Result<TaskPage<'a>, String> {
        let pages = session
            .browser
            .pages()
            .await
            .map_err(|e| format!("列出浏览器页面失败：{e}"))?;
        let mut target: Option<String> = None;
        for page in pages {
            let url = page
                .url()
                .await
                .map_err(|e| format!("读取页面地址失败：{e}"))?;
            if url.as_ref().is_some_and(|u| is_console_url(u.as_str())) {
                target = Some(page.target_id().as_ref().to_owned());
                break;
            }
        }
        let Some(target) = target else {
            return Err("未找到中控页；请先在该环境打开中控台后再使用自动弹品".into());
        };
        session
            .task_page(&target, cancel.clone(), PAGE_LOCK_TIMEOUT)
            .await
            .map_err(|e| format!("中控页繁忙、已关闭或等待超时：{e}"))
    }

    /// 启动自动弹品（对 `autoPopUp::start`）。已在运行则先停旧任务再启动。
    pub async fn auto_popup_start(
        &self,
        profile_id: &str,
        config: AutoPopUpConfig,
    ) -> Result<AutoPopUpStatus, String> {
        validate_config(&config)?;
        self.require_console_scope(profile_id).await?;
        // 启动前确认会话存在（不自动启动环境）。
        self.popup_session(profile_id).await?;
        self.auto_popup_stop_inner(profile_id, "restart").await;

        let queue = build_execution_queue(&config);
        let first_delay = pick_interval(config.interval);
        let random = config.random;
        let cancel = TaskCancel::new();
        task_table().lock().unwrap().insert(
            profile_id.to_string(),
            RunningTask {
                cancel: cancel.clone(),
                config,
                queue,
                index: 0,
                random,
                last_goods_id: None,
                last_error: None,
            },
        );

        let loop_profile = profile_id.to_string();
        let registry = self.registry().clone();
        tokio::spawn(async move {
            Self::popup_loop(loop_profile, registry, cancel, first_delay).await;
        });

        emit(AutoPopUpEvent {
            profile_id: profile_id.to_string(),
            kind: PopupEventKind::Started,
            goods_id: None,
            reason: None,
        });
        let queue_len = task_table()
            .lock()
            .unwrap()
            .get(profile_id)
            .map(|t| t.queue.len())
            .unwrap_or(0);
        tracing::info!(profile = %profile_id, queue = queue_len, "auto-popup started");
        Ok(self.auto_popup_status(profile_id))
    }

    /// 定时循环：首 tick 延迟全局间隔，之后每个商品按其自身间隔调度。
    /// `random` 为真时随机选，否则顺序轮换（对 `tick` + `pickQueueItem`）。
    /// 队列 / 配置逐 tick 从共享表重读，`update_config` 热更新即时生效。
    async fn popup_loop(
        profile_id: String,
        registry: Arc<ProfileRegistry>,
        cancel: TaskCancel,
        first_delay: Duration,
    ) {
        Self::sleep_interruptible(first_delay, &cancel).await;
        loop {
            if cancel.is_cancelled() {
                break;
            }
            // 从共享表取本轮 item（热更新重建的队列即时生效）。
            let picked = {
                let mut table = task_table().lock().unwrap();
                let Some(task) = table.get_mut(&profile_id) else {
                    break;
                };
                if task.cancel.is_cancelled() || task.queue.is_empty() {
                    break;
                }
                let item = if task.random {
                    let i = rand::thread_rng().gen_range(0..task.queue.len());
                    task.queue[i].clone()
                } else {
                    let item = task.queue[task.index % task.queue.len()].clone();
                    task.index = task.index.wrapping_add(1);
                    item
                };
                let retry = task.config.retry.clone();
                (item, retry)
            };
            let (item, retry) = picked;
            if cancel.is_cancelled() {
                break;
            }
            let Some(session) = registry.get(&profile_id).await else {
                tracing::warn!(profile = %profile_id, "auto-popup: session lost, stopping");
                Self::finish_popup(&profile_id, "session_lost");
                break;
            };
            let max_retries = normalize_max_retries(retry.as_ref().and_then(|r| r.max_retries));
            let retry_delay =
                normalize_retry_delay_ms(retry.as_ref().and_then(|r| r.retry_delay_ms));
            let outcome = match Self::console_task_page(&session, &cancel).await {
                Ok(mut task_page) => {
                    explain_with_retry(
                        &mut task_page,
                        &item.goods_id,
                        &cancel,
                        Some(max_retries),
                        Some(retry_delay),
                    )
                    .await
                }
                Err(error) => RetryOutcome {
                    ok: false,
                    value: None,
                    error: Some(error),
                    attempts: 0,
                },
            };
            {
                let mut table = task_table().lock().unwrap();
                let Some(task) = table.get_mut(&profile_id) else {
                    break;
                };
                if task.cancel.is_cancelled() || cancel.is_cancelled() {
                    break;
                }
                if outcome.ok {
                    task.last_goods_id = Some(item.goods_id.clone());
                    task.last_error = None;
                    tracing::info!(profile = %profile_id, goods = %item.goods_id, attempts = outcome.attempts, "auto-popup explained");
                } else {
                    task.last_error = outcome.error.clone();
                    tracing::warn!(profile = %profile_id, goods = %item.goods_id, error = ?outcome.error, "auto-popup explain failed");
                }
            }
            emit(AutoPopUpEvent {
                profile_id: profile_id.clone(),
                kind: if outcome.ok {
                    PopupEventKind::Explained
                } else {
                    PopupEventKind::ExplainFailed
                },
                goods_id: Some(item.goods_id.clone()),
                reason: outcome.error.clone(),
            });
            // 按该商品自身的间隔调度下一轮。
            Self::sleep_interruptible(pick_interval(item.interval), &cancel).await;
        }
    }

    /// 可中断的等待：100ms 切片检查取消。
    async fn sleep_interruptible(duration: Duration, cancel: &TaskCancel) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            if cancel.is_cancelled() {
                return;
            }
            let rest = deadline.saturating_duration_since(Instant::now());
            tokio::time::sleep(rest.min(Duration::from_millis(100))).await;
        }
    }

    /// 循环内终止：移除运行态 + 注销快捷键 + 广播。
    fn finish_popup(profile_id: &str, reason: &str) {
        task_table().lock().unwrap().remove(profile_id);
        unregister_shortcuts(profile_id);
        emit(AutoPopUpEvent {
            profile_id: profile_id.to_string(),
            kind: PopupEventKind::Stopped,
            goods_id: None,
            reason: Some(reason.to_string()),
        });
        tracing::info!(profile = %profile_id, %reason, "auto-popup stopped");
    }

    async fn auto_popup_stop_inner(&self, profile_id: &str, reason: &str) {
        if let Some(task) = task_table().lock().unwrap().remove(profile_id) {
            task.cancel.cancel();
        }
        // 停止后尝试取消正在进行的讲解（全新 lease，不受已取消 token 影响）。
        if let Ok(session) = self.popup_session(profile_id).await {
            let cleanup = TaskCancel::new();
            if let Ok(mut task_page) = Self::console_task_page(&session, &cleanup).await {
                let _ = cancel_explain_goods(&mut task_page).await;
            }
        }
        unregister_shortcuts(profile_id);
        emit(AutoPopUpEvent {
            profile_id: profile_id.to_string(),
            kind: PopupEventKind::Stopped,
            goods_id: None,
            reason: Some(reason.to_string()),
        });
        tracing::info!(profile = %profile_id, %reason, "auto-popup stopped");
    }

    /// 停止自动弹品（对 `autoPopUp::stop`）。未运行也返回成功。
    pub async fn auto_popup_stop(
        &self,
        profile_id: &str,
        reason: &str,
    ) -> Result<AutoPopUpStatus, String> {
        self.auto_popup_stop_inner(profile_id, reason).await;
        Ok(self.auto_popup_status(profile_id))
    }

    /// 当前状态快照（同步读表）。
    pub fn auto_popup_status(&self, profile_id: &str) -> AutoPopUpStatus {
        let table = task_table().lock().unwrap();
        status_of(profile_id, table.get(profile_id))
    }

    /// 是否正在运行（同步）。
    pub fn auto_popup_is_running(&self, profile_id: &str) -> bool {
        task_table().lock().unwrap().contains_key(profile_id)
    }

    /// 热更新运行中配置并重建队列（对 `autoPopUp::updateConfig`）。
    /// `patch` 中 `Some` 字段覆盖，`None` 保持；队列重建后序号归零。
    pub async fn auto_popup_update_config(
        &self,
        profile_id: &str,
        patch: AutoPopUpConfigPatch,
    ) -> Result<AutoPopUpStatus, String> {
        let mut table = task_table().lock().unwrap();
        let task = table
            .get_mut(profile_id)
            .ok_or_else(|| "任务未运行".to_string())?;
        if let Some(goods_ids) = patch.goods_ids {
            task.config.goods_ids = Some(goods_ids);
        }
        if let Some(interval) = patch.interval {
            task.config.interval = interval;
        }
        if let Some(per_goods_interval) = patch.per_goods_interval {
            task.config.per_goods_interval = Some(per_goods_interval);
        }
        if let Some(goods_items) = patch.goods_items {
            task.config.goods_items = Some(goods_items);
        }
        if let Some(random) = patch.random {
            task.config.random = random;
            task.random = random;
        }
        if let Some(retry) = patch.retry {
            task.config.retry = Some(retry);
        }
        validate_config(&task.config)?;
        task.queue = build_execution_queue(&task.config);
        task.index = 0;
        emit(AutoPopUpEvent {
            profile_id: profile_id.to_string(),
            kind: PopupEventKind::ConfigUpdated,
            goods_id: None,
            reason: None,
        });
        let queue_len = task.queue.len();
        tracing::info!(profile = %profile_id, queue = queue_len, "auto-popup config updated");
        Ok(status_of(profile_id, Some(task)))
    }

    /// 一次性读取中控页商品列表（对 `autoPopUp::fetchGoodsIds`，返回全行）。
    pub async fn auto_popup_goods(&self, profile_id: &str) -> Result<Vec<GoodsInfo>, String> {
        self.require_console_scope(profile_id).await?;
        let session = self.popup_session(profile_id).await?;
        let cancel = TaskCancel::new();
        let mut task_page = Self::console_task_page(&session, &cancel).await?;
        fetch_goods_list(&mut task_page).await
    }

    /// 商品知识扫描：读列表 + 与来源比对（来源就绪前用空实现，只产候选）。
    pub async fn auto_popup_scan(&self, profile_id: &str) -> Result<ScanReport, String> {
        let goods = self.auto_popup_goods(profile_id).await?;
        if goods.is_empty() {
            return Err("商品列表为空".into());
        }
        Ok(scan_goods_knowledge(
            &EmptyKnowledgeSource,
            profile_id,
            &goods,
        ))
    }

    /// 单次讲解（快捷键回调与手动触发共用）：门禁 + 中控页 + 重试。
    pub async fn auto_popup_explain_once(
        &self,
        profile_id: &str,
        goods_id: &str,
    ) -> Result<(), String> {
        self.require_console_scope(profile_id).await?;
        let session = self.popup_session(profile_id).await?;
        let cancel = TaskCancel::new();
        let mut task_page = Self::console_task_page(&session, &cancel).await?;
        let outcome = explain_with_retry(&mut task_page, goods_id, &cancel, None, None).await;
        if outcome.ok {
            Ok(())
        } else {
            Err(outcome.error.unwrap_or_else(|| "讲解失败".into()))
        }
    }

    /// 快捷键触发：查表得商品序号并立即讲解一次。
    pub async fn auto_popup_trigger_shortcut(
        &self,
        profile_id: &str,
        accelerator: &str,
    ) -> Result<String, String> {
        let goods_id = lookup_shortcut(profile_id, accelerator)
            .ok_or_else(|| format!("快捷键未注册：{accelerator}"))?;
        let result = self.auto_popup_explain_once(profile_id, &goods_id).await;
        emit(AutoPopUpEvent {
            profile_id: profile_id.to_string(),
            kind: PopupEventKind::ShortcutTriggered,
            goods_id: Some(goods_id.clone()),
            reason: result.as_ref().err().cloned(),
        });
        result.map(|()| goods_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;

    fn config() -> AutoPopUpConfig {
        AutoPopUpConfig {
            goods_ids: Some(vec!["001".into(), "002".into()]),
            interval: [30.0, 60.0],
            per_goods_interval: None,
            goods_items: None,
            random: false,
            retry: None,
        }
    }

    #[test]
    fn queue_falls_back_to_goods_ids_in_order() {
        let queue = build_execution_queue(&config());
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].goods_id, "001");
        assert_eq!(queue[1].goods_id, "002");
        assert_eq!(queue[0].interval, [30.0, 60.0]);
    }

    #[test]
    fn goods_items_take_priority_with_repeat_and_interval() {
        let mut cfg = config();
        cfg.goods_items = Some(vec![
            AutoPopUpGoodsItem {
                id: "A".into(),
                repeat_count: Some(3),
                interval: Some([5.0, 9.0]),
            },
            AutoPopUpGoodsItem {
                id: "  ".into(),
                repeat_count: Some(9),
                interval: None,
            },
        ]);
        let queue = build_execution_queue(&cfg);
        assert_eq!(queue.len(), 3);
        assert!(queue.iter().all(|q| q.goods_id == "A"));
        assert!(queue.iter().all(|q| q.interval == [5.0, 9.0]));
    }

    #[test]
    fn legacy_per_goods_interval_overrides_global() {
        let mut cfg = config();
        cfg.per_goods_interval = Some(HashMap::from([("002".to_string(), [7.0, 8.0])]));
        let queue = build_execution_queue(&cfg);
        assert_eq!(queue[0].interval, [30.0, 60.0]);
        assert_eq!(queue[1].interval, [7.0, 8.0]);
    }

    #[test]
    fn repeat_count_is_clamped() {
        assert_eq!(clamp_repeat_count(None), 1);
        assert_eq!(clamp_repeat_count(Some(0)), 1);
        assert_eq!(clamp_repeat_count(Some(5)), 5);
        assert_eq!(clamp_repeat_count(Some(999_999)), 1000);
    }

    #[test]
    fn config_validation_rejects_bad_interval_and_empty_queue() {
        let mut bad = config();
        bad.interval = [60.0, 30.0];
        assert!(validate_config(&bad).is_err());
        let mut empty = config();
        empty.goods_ids = Some(vec!["  ".into()]);
        empty.goods_items = None;
        assert!(validate_config(&empty).is_err());
        assert!(validate_config(&config()).is_ok());
    }

    #[test]
    fn retry_bounds_match_spec() {
        assert_eq!(normalize_max_retries(None), 3);
        assert_eq!(normalize_max_retries(Some(0)), 1);
        assert_eq!(normalize_max_retries(Some(99)), 10);
        assert_eq!(normalize_retry_delay_ms(None), 1000);
        assert_eq!(normalize_retry_delay_ms(Some(999_999)), 60_000);
    }

    #[tokio::test]
    async fn retry_succeeds_after_failures_with_attempt_count() {
        let mut calls = 0usize;
        let outcome = run_with_retry(
            |_attempt| {
                calls += 1;
                let calls = calls;
                Box::pin(async move {
                    if calls < 3 {
                        Err::<(), String>("boom".into())
                    } else {
                        Ok(())
                    }
                })
                    as Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>
            },
            Some(5),
            Some(0),
            None,
        )
        .await;
        assert!(outcome.ok);
        assert_eq!(outcome.attempts, 3);
    }

    #[tokio::test]
    async fn retry_stops_at_cap_and_reports_last_error() {
        let outcome = run_with_retry(
            |_attempt| {
                Box::pin(async { Err::<(), String>("nope".into()) })
                    as Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>
            },
            Some(3),
            Some(0),
            None,
        )
        .await;
        assert!(!outcome.ok);
        assert_eq!(outcome.attempts, 3);
        assert_eq!(outcome.error.as_deref(), Some("nope"));
    }

    #[tokio::test]
    async fn retry_aborts_when_task_stops() {
        let outcome = run_with_retry(
            |_attempt| {
                Box::pin(async { Err::<(), String>("nope".into()) })
                    as Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>
            },
            Some(10),
            Some(5_000),
            Some(&|| false),
        )
        .await;
        assert!(!outcome.ok);
        assert_eq!(outcome.error.as_deref(), Some("任务已停止"));
    }

    #[test]
    fn scan_reports_title_and_price_diffs() {
        struct Memory(HashMap<String, GoodsKnowledge>);
        impl ProductScriptSource for Memory {
            fn goods_knowledge(&self, _account: &str, goods_id: &str) -> Option<GoodsKnowledge> {
                self.0.get(goods_id).cloned()
            }
        }
        let source = Memory(HashMap::from([(
            "001".to_string(),
            GoodsKnowledge {
                title: Some("旧标题".into()),
                price: Some("9.9".into()),
            },
        )]));
        let goods = vec![
            GoodsInfo {
                serial: "001".into(),
                title: Some("新标题".into()),
                price: Some("9.9".into()),
            },
            GoodsInfo {
                serial: "002".into(),
                title: Some("标题".into()),
                price: None,
            },
        ];
        let report = scan_goods_knowledge(&source, "acc", &goods);
        assert_eq!(report.scanned_count, 2);
        assert_eq!(report.diffs.len(), 1);
        assert_eq!(report.diffs[0].goods_id, "001");
        assert_eq!(report.diffs[0].field, KnowledgeField::Title);
        assert_eq!(report.candidates.len(), 2);
    }

    #[test]
    fn shortcuts_register_occupancy_and_unregister() {
        let profile = "popup-shortcut-test-profile";
        unregister_shortcuts(profile);
        unregister_shortcuts("popup-shortcut-other");
        let first = register_shortcuts(
            profile,
            ShortcutConfig {
                bindings: HashMap::from([("Ctrl+1".to_string(), "001".to_string())]),
            },
        );
        assert!(first.ok);
        let clash = register_shortcuts(
            "popup-shortcut-other",
            ShortcutConfig {
                bindings: HashMap::from([("Ctrl+1".to_string(), "002".to_string())]),
            },
        );
        assert!(!clash.ok);
        assert_eq!(clash.failed.len(), 1);
        assert_eq!(
            lookup_shortcut(profile, "Ctrl+1").as_deref(),
            Some("001")
        );
        unregister_shortcuts(profile);
        unregister_shortcuts("popup-shortcut-other");
        assert_eq!(lookup_shortcut(profile, "Ctrl+1"), None);
    }

    #[test]
    fn console_url_gate_rejects_non_console_origins() {
        assert!(is_console_url("https://live.kuaishou.com/shop/live"));
        assert!(is_console_url("https://s.kwaixiaodian.com/zone/home"));
        assert!(!is_console_url("https://www.kuaishou.com/"));
        assert!(!is_console_url("http://live.kuaishou.com/shop/live"));
        assert!(!is_console_url("https://evil.com/?x=live.kuaishou.com"));
    }

    #[test]
    fn picked_interval_stays_in_range() {
        for _ in 0..50 {
            let wait = pick_interval([30.0, 60.0]);
            assert!(wait >= Duration::from_secs(30));
            assert!(wait <= Duration::from_secs(60));
        }
    }
}
