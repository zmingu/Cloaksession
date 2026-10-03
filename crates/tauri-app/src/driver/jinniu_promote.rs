//! Magnetic Jinniu (磁力金牛) promote operations.
//!
//! Rust port of jieger `electron/main/tasks/jinniuPromote/index.ts` (F1
//! live-user switch + plan-build phases F5/F6/F7) and the shared helpers in
//! `electron/main/platforms/kuaishou/jinniuActions.ts`, with the selector
//! table from `.../jinniuSelectors.ts`.
//!
//! ## Scope gaps (deliberate, not oversights)
//!
//! - F8 (plan management page) / F9 (auto-ROI "加速探索"): jieger only
//!   carries their *selectors* (`manage.*`, `accelerate.*` below, sourced
//!   from kslivego `browser_service.py`); no manage/ROI task functions exist
//!   in `tasks/jinniuPromote/index.ts` yet. This module ports the selector
//!   constants but adds no invented manage/ROI flows.
//! - Playwright `:has-text()` selectors from jieger are kept verbatim as
//!   reference constants, but live code never feeds them to
//!   `querySelector`/chromiumoxide `click` (invalid CSS there). All live
//!   clicks are text-driven `evaluate` scripts: exact text match first,
//!   contains-match fallback — the same policy as jieger's
//!   `_click_button_by_text`.
//! - `homeType=new` is mandatory on the storeCreate URL. Kuaishou naming is
//!   inverted here: `new` pins the *legacy*创编 page (matches jieger's
//!   `platformConfig` home entry). Navigation results are asserted; a page
//!   without `homeType=new` is refused, never "fixed" silently.
//! - Jinniu isolation: every live op first requires the profile's business
//!   scope to be `jinniu` (`require_jinniu_scope`). Xiaodian-shop flows
//!   stay on kuaishou-shop profiles.
//!
//! ## Testability
//!
//! All URL parsing, page-picking priority, live-user text parsing, button
//! text matching and JS-expression building are pure functions covered by
//! the unit tests below. Live CDP bodies are thin leases over `TaskPage`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cdp_driver::{session::BrowserSession, SelectorState, TaskCancel, TaskError, TaskPage};
use multizen_core::{MultizenError, Result};
use serde::{Deserialize, Serialize};

use super::TauriBrowserDriver;

// ── URL constants (KS_JINNIU_URLS) ──────────────────────────────────────────

/// `__accountId__` query parameter: the sub-account context every Jinniu
/// page carries (jieger `accountIdQueryParam`, browser_service.py:528).
pub const ACCOUNT_ID_PARAM: &str = "__accountId__";
/// `homeType` query parameter that pins the创编 page generation.
pub const HOME_TYPE_PARAM: &str = "homeType";
/// Mandatory `homeType` value. Inverted Kuaishou naming: `new` = legacy
/// (旧版)创编 page. Must never be changed to anything else.
pub const HOME_TYPE_LEGACY: &str = "new";
/// storeCreate page host.
pub const STORE_CREATE_HOST: &str = "https://niu.e.kuaishou.com";
/// storeCreate page URL keyword (page.url contains check).
pub const STORE_CREATE_KEYWORD: &str = "storeCreate";
/// Plan-management page URL keyword (:966/989). No URL template exists in
/// jieger, so no builder is provided (see module docs).
pub const STORE_MANAGE_KEYWORD: &str = "storeManage";
/// Pages preferred as `__accountId__` source, in priority order
/// (browser_service.py:467).
pub const ACCOUNT_SOURCE_KEYWORDS: &[&str] = &["storeCreate", "home", "storeManage"];
/// Follow-assistant page URL keyword (:436).
pub const HELPER_KEYWORD: &str = "zs.kwaixiaodian.com/page/helper";

// ── Selector constants (KS_JINNIU_SELECTORS) ────────────────────────────────
// Reference table. Entries containing Playwright-only `:has-text()` are
// marked; live code uses the text-driven JS builders instead.

// Live-user dropdown (:560-595).
pub const SEL_LIVE_USER_CONTAINER: &str = "#authorize-user-select .ant-select";
pub const SEL_VISIBLE_DROPDOWN: &str = ".ant-select-dropdown";
pub const SEL_DROPDOWN_ITEM: &str = ".ant-select-item-option";
pub const SEL_SELECTED_ATTR: &str = "aria-selected";

// storeCreate page (:286-323).
pub const SEL_STORE_READY: &str = ".ad-form-row";
pub const SEL_FORM_ROW: &str = ".ad-form-row";
pub const SEL_ROW_OPTION: &str =
    "label.ant-radio-button-wrapper, label.ant-radio-wrapper, button, [role='button']";
pub const SEL_ROW_INPUT: &str = "input[type='number'], input[type='text'], input:not([type])";
pub const SEL_NET_ROI_CHECKBOX: &str = ".ant-checkbox-input";
pub const SEL_VIDEO_LIBRARY_READY: &str = ".video-library, [class*=\"video-library\"]";
pub const SEL_VIDEO_CARD: &str = ".video-card, [class*=\"VideoCard\"], [data-testid*=\"video\"]";
/// Playwright-only (`:has-text`); reference only — live code clicks 确定/确认 by text.
pub const SEL_VIDEO_CONFIRM_BUTTON: &str = "button:has-text(\"确定\"), button:has-text(\"确认\")";
pub const SEL_AD_COPY_TEXTAREA: &str = "textarea[placeholder*=\"广告\"], textarea[placeholder*=\"文案\"]";
/// Playwright-only (`:has-text`); reference only — live code clicks 立即推广 by text.
pub const SEL_SUBMIT_BUTTON: &str = "button:has-text(\"立即推广\")";

// Manage page (:947-1595). Reference only; no live manage flow exists yet.
pub const SEL_MANAGE_READY: &str = ".plan-list, [class*=\"manage-table\"], .ant-table";
/// Playwright-only; reference only.
pub const SEL_MANAGE_FEEDBACK_DISMISS: &str = "button:has-text(\"我知道了\"), button:has-text(\"好的\")";
pub const SEL_MANAGE_PLAN_ROW: &str = ".plan-row, .ant-table-row";
pub const SEL_MANAGE_PLAN_CELL: &str = "td, .ant-table-cell";
pub const SEL_MANAGE_BALANCE_TEXT: &str = ".balance, [class*=\"Balance\"], [class*=\"account-money\"]";
pub const SEL_MANAGE_BALANCE_TRIGGER: &str = ".balance-trigger, [class*=\"balance-popover\"]";
pub const SEL_MANAGE_SPEND_TEXT: &str = ".spend, [class*=\"Spend\"], [class*=\"cost\"]";
/// Playwright-only; reference only.
pub const SEL_MANAGE_REFRESH_BUTTON: &str = "button:has-text(\"刷新\")";
pub const SEL_MANAGE_SUCCESS_MESSAGE: &str = ".ant-message-success";
pub const SEL_MANAGE_VISIBLE_OVERLAY: &str = ".ant-popover-inner, .ant-tooltip-inner, .edit-overlay";

// Accelerate-explore modal (:1072-1314). Reference only; no live flow exists yet.
pub const SEL_ACCELERATE_TRIGGER: &str = "button:has-text(\"加速探索\")";
pub const SEL_ACCELERATE_TIME_PERIOD: &str = "[class*=\"time-period\"] .ant-select";
pub const SEL_ACCELERATE_BUDGET_INPUT: &str = "input[placeholder*=\"预算\"], input[placeholder*=\"金额\"]";
pub const SEL_ACCELERATE_CONFIRM: &str = "button:has-text(\"确定\")";

// Generic modal tools (:1175-1670).
pub const SEL_MODAL_VISIBLE_ANY: &str = ".ant-modal-wrap, .ant-modal";
/// Modal selectors enumerated for `querySelectorAll` iteration in live JS.
pub const MODAL_SELECTORS: &[&str] = &[".ant-modal-wrap", ".ant-modal", "[class*=\"Modal\"]"];

// Creator-authorize page (/account/authorize). Reference only.
pub const SEL_AUTHORIZE_ADD_BUTTON: &str = "button:has-text(\"新增授权申请\")";
pub const SEL_AUTHORIZE_USER_TRIGGER: &str =
    ".KwaiUserSelect-module__kwaiUserSelect___IaPxG .ant-select-selector";
pub const SEL_AUTHORIZE_DROPDOWN: &str = ".ant-select-dropdown:not(.ant-select-dropdown-hidden)";
pub const SEL_AUTHORIZE_SEARCH_INPUT: &str = ".KwaiUserSelect-module__searchInput___GzpPn input.ant-input";
pub const SEL_AUTHORIZE_USER_ITEM: &str = ".KwaiUserSelect-module__userItem___UF_X1";
pub const SEL_AUTHORIZE_SELECTED_DISPLAY: &str = ".KwaiUserSelect-module__selectedDisplay___nuR5X";
pub const SEL_AUTHORIZE_AGREEMENT: &str =
    ".ant-modal-wrap:not([style*=\"display: none\"]) .ant-checkbox-wrapper";
pub const SEL_AUTHORIZE_CONFIRM: &str =
    ".ant-modal-wrap:not([style*=\"display: none\"]) .ant-btn-primary";

// ── Timeouts (mirror jieger's millisecond budgets) ──────────────────────────

const OPEN_PAGE_TIMEOUT: Duration = Duration::from_secs(15);
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const OP_TIMEOUT: Duration = Duration::from_secs(5);
const VIDEO_LIBRARY_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const UI_SETTLE: Duration = Duration::from_millis(500);

// ── Wire types ──────────────────────────────────────────────────────────────

/// One promotable live user (jieger `LiveUserInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuLiveUser {
    pub uid: String,
    pub display_name: String,
    pub full_text: String,
    pub is_selected: bool,
}

/// `getLiveUsers` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuLiveUsers {
    pub account_id: String,
    pub users: Vec<JinniuLiveUser>,
}

/// Opened (or reused) storeCreate tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCreateTab {
    pub account_id: String,
    pub url: String,
    pub target_id: String,
}

/// phase1 config (jieger `StoreCreatePhase1Config`). All fields optional;
/// [`StoreCreatePhase1Config::resolved`] fills jieger's defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCreatePhase1Config {
    pub enable_net_roi: Option<bool>,
    pub daily_budget: Option<String>,
    pub roi_coefficient: Option<String>,
    pub promote_type: Option<String>,
    pub roi_target_mode: Option<String>,
    pub creative_mode: Option<String>,
}

/// phase1 config with jieger defaults applied (`DEFAULT_PHASE1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPhase1 {
    pub enable_net_roi: bool,
    pub daily_budget: String,
    pub roi_coefficient: String,
    pub promote_type: String,
    pub roi_target_mode: String,
    pub creative_mode: String,
}

impl StoreCreatePhase1Config {
    pub fn resolved(&self) -> ResolvedPhase1 {
        ResolvedPhase1 {
            enable_net_roi: self.enable_net_roi.unwrap_or(false),
            daily_budget: self.daily_budget.clone().unwrap_or_else(|| "1000".into()),
            roi_coefficient: self.roi_coefficient.clone().unwrap_or_else(|| "5".into()),
            promote_type: self.promote_type.clone().unwrap_or_else(|| "长效推广".into()),
            roi_target_mode: self.roi_target_mode.clone().unwrap_or_else(|| "自定义".into()),
            creative_mode: self.creative_mode.clone().unwrap_or_else(|| "程序化创意".into()),
        }
    }
}

/// Minimal page reference for page-picking. Live code builds these from the
/// CDP session; tests construct them directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRef {
    pub target_id: String,
    pub url: String,
}

impl PageRef {
    pub fn new(target_id: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            target_id: target_id.into(),
            url: url.into(),
        }
    }
}

// ── Pure helpers ────────────────────────────────────────────────────────────

/// Extract `__accountId__` from a URL (jieger `extractAccountIdFromUrl`,
/// browser_service.py:526). `None` for invalid URLs, missing or empty param.
pub fn extract_account_id_from_url(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()?
        .query_pairs()
        .find(|(key, _)| key == ACCOUNT_ID_PARAM)
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
}

/// Build the全站推广创编 URL with the sub-account context. `homeType=new`
/// is always forced (legacy创编 page; inverted Kuaishou naming).
pub fn build_store_create_url(account_id: &str) -> String {
    format!(
        "{STORE_CREATE_HOST}/storeCreate?{ACCOUNT_ID_PARAM}={account_id}&{HOME_TYPE_PARAM}={HOME_TYPE_LEGACY}"
    )
}

/// Whether `url` pins the legacy创编 page generation (`homeType=new`).
pub fn home_type_is_new(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .query_pairs()
                .find(|(key, _)| key == HOME_TYPE_PARAM)
                .map(|(_, value)| value == HOME_TYPE_LEGACY)
        })
        .unwrap_or(false)
}

/// Enforce `homeType=new` on a navigated URL. Refuses anything else —
/// callers must not "repair" the URL, the generation pin is load-bearing.
pub fn require_home_type_new(url: &str) -> Result<()> {
    if home_type_is_new(url) {
        Ok(())
    } else {
        Err(MultizenError::Cdp(format!(
            "金牛创编页缺少 {HOME_TYPE_PARAM}={HOME_TYPE_LEGACY}（实际 {url}），已拒绝继续"
        )))
    }
}

/// Find the loaded全站推广创编 page (url contains `storeCreate`).
/// Newest-first: later entries win (mirrors jieger's `[...pages].reverse()`).
/// (browser_service.py:446 `_find_store_create_page`)
pub fn find_store_create_page(pages: &[PageRef]) -> Option<&PageRef> {
    pages
        .iter()
        .rev()
        .find(|page| page.url.contains(STORE_CREATE_KEYWORD))
}

/// Find the `__accountId__` source page. Priority: 首页/创编/管理 pages
/// first, then any other page carrying `__accountId__` (fallback).
/// Newest-first within each tier.
/// (browser_service.py:456 `_get_account_source_page`)
pub fn find_account_source_page(pages: &[PageRef]) -> Option<&PageRef> {
    let mut fallback = None;
    for page in pages.iter().rev() {
        if extract_account_id_from_url(&page.url).is_none() {
            continue;
        }
        if ACCOUNT_SOURCE_KEYWORDS
            .iter()
            .any(|keyword| page.url.contains(keyword))
        {
            return Some(page);
        }
        if fallback.is_none() {
            fallback = Some(page);
        }
    }
    fallback
}

/// Collapse whitespace runs (jieger `.trim().replace(/\s+/g, ' ')`).
fn normalize_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a dropdown row into [`JinniuLiveUser`]
/// (browser_service.py:597 `_parse_live_user`). Supports full/half-width
/// colons; `displayName` requires whitespace before `的直播间` (mirrors the
/// `^(.*?)\s+的直播间` regex); UID-less rows fall back to the full text.
pub fn parse_live_user_text(raw: &str) -> JinniuLiveUser {
    const SELECTED_PREFIX: &str = "[selected] ";
    const UID_MARKER: &str = "达人UID";
    const LIVE_ROOM_SUFFIX: &str = "的直播间";

    let (is_selected, rest) = match raw.strip_prefix(SELECTED_PREFIX) {
        Some(text) => (true, text),
        None => (false, raw),
    };
    let normalized = normalize_text(rest);

    let uid = normalized
        .find(UID_MARKER)
        .and_then(|pos| {
            let after = &normalized[pos + UID_MARKER.len()..];
            let after = after
                .strip_prefix(':')
                .or_else(|| after.strip_prefix('：'))?;
            let digits: String =
                after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() { None } else { Some(digits) }
        })
        .unwrap_or_else(|| normalized.clone());

    let display_name = normalized
        .find(LIVE_ROOM_SUFFIX)
        .and_then(|pos| {
            let before = &normalized[..pos];
            if !before.ends_with(char::is_whitespace) {
                return None;
            }
            let name = before.trim().to_string();
            if name.is_empty() { None } else { Some(name) }
        })
        .unwrap_or_else(|| normalized.clone());

    JinniuLiveUser {
        uid,
        display_name,
        full_text: normalized,
        is_selected,
    }
}

/// Match `text` against a button-text list: exact (normalized) match wins,
/// first contains-match is the fallback, `None` when nothing matches. This
/// is the testable core of text-driven clicking
/// (browser_service.py:728 `_click_button_by_text`).
pub fn match_button_text(buttons: &[String], text: &str) -> Option<usize> {
    let want = normalize_text(text);
    if want.is_empty() {
        return None;
    }
    let mut fallback = None;
    for (index, button) in buttons.iter().enumerate() {
        let candidate = normalize_text(button);
        if candidate == want {
            return Some(index);
        }
        if fallback.is_none() && candidate.contains(&want) {
            fallback = Some(index);
        }
    }
    fallback
}

/// Placeholder ad copy generator (jieger `generateRandomAdCopy`): pure
/// Chinese + digits from the fixed charset, `length` chars. Deterministic
/// per `seed` (LCG) so tests stay stable; live callers seed from time.
pub fn generate_ad_copy(seed: u64, length: usize) -> String {
    const CHARSET: &str = "热卖好物精选优惠抢购上新限时折扣品质推荐爆款体验福利秒杀";
    let chars: Vec<char> = CHARSET.chars().collect();
    debug_assert!(!chars.is_empty());
    let mut state = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    (0..length)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            chars[(state >> 33) as usize % chars.len()]
        })
        .collect()
}

// ── JS expression builders (live `evaluate` payloads) ───────────────────────

/// JSON string literal for safe interpolation into JS expressions.
fn js_str(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a string cannot fail")
}

/// Click a `<button>` by visible text: exact match first, contains fallback
/// (returns bool). Text-driven — never a fixed selector.
pub fn click_button_by_text_js(button_text: &str) -> String {
    format!(
        r#"(() => {{
  const want = {want};
  const norm = (s) => (s || "").trim().replace(/\s+/g, " ");
  const target = norm(want);
  const buttons = Array.from(document.querySelectorAll("button"));
  const visible = (el) => el.offsetParent !== null;
  const el = buttons.find((b) => visible(b) && norm(b.innerText || b.textContent) === target)
    || buttons.find((b) => visible(b) && norm(b.innerText || b.textContent).includes(target));
  if (!el) return false;
  el.scrollIntoView({{ block: "center" }});
  el.click();
  return true;
}})()"#,
        want = js_str(button_text)
    )
}

/// Whether `document.body.innerText` contains `text` (returns bool).
pub fn wait_for_text_js(text: &str) -> String {
    format!(
        r#"(() => {{
  const b = document.body;
  return !!(b && (b.innerText || "").includes({want}));
}})()"#,
        want = js_str(text)
    )
}

/// First visible modal whose text contains `text`; returns its innerText
/// (truncated) or null (browser_service.py:1635 `_find_visible_modal_by_text`).
pub fn find_visible_modal_js(text: &str) -> String {
    let selectors = MODAL_SELECTORS
        .iter()
        .map(|sel| js_str(sel))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"(() => {{
  const selectors = [{selectors}];
  const want = {want};
  for (const sel of selectors) {{
    for (const m of document.querySelectorAll(sel)) {{
      if (m.offsetParent === null) continue;
      const t = m.innerText || "";
      if (t.includes(want)) return t.slice(0, 2000);
    }}
  }}
  return null;
}})()"#,
        want = js_str(text)
    )
}

/// Click by text within `scope_selector` (button/span/div/role=button,
/// exact-first; returns bool) (:1203 `_click_text_in_scope`).
pub fn click_text_in_scope_js(scope_selector: &str, text: &str) -> String {
    format!(
        r#"(() => {{
  const scope = document.querySelector({scope});
  if (!scope) return false;
  const norm = (s) => (s || "").trim().replace(/\s+/g, " ");
  const want = norm({want});
  const cands = Array.from(scope.querySelectorAll("button, span, div, [role='button']"));
  const visible = cands.filter((el) => el.offsetParent !== null);
  const el = visible.find((el) => norm(el.innerText || el.textContent) === want)
    || visible.find((el) => norm(el.innerText || el.textContent).includes(want));
  if (!el) return false;
  el.scrollIntoView({{ block: "center" }});
  el.click();
  return true;
}})()"#,
        scope = js_str(scope_selector),
        want = js_str(text)
    )
}

/// Count visible `.ant-select-dropdown` popups (returns number).
pub const COUNT_VISIBLE_DROPDOWNS_JS: &str = r#"(() => Array.from(document.querySelectorAll(".ant-select-dropdown")).filter((n) => n.offsetParent !== null).length)()"#;

/// Read live-user dropdown options as `[{text, selected}]` (:569).
pub const READ_LIVE_USERS_JS: &str = r#"(() => {
  const drops = Array.from(document.querySelectorAll(".ant-select-dropdown")).filter((n) => n.offsetParent !== null);
  const cur = drops[0];
  if (!cur) return [];
  return Array.from(cur.querySelectorAll(".ant-select-item-option"))
    .map((n) => ({
      text: ((n.innerText || n.textContent) || "").trim().replace(/\s+/g, " "),
      selected: n.getAttribute("aria-selected") === "true",
    }))
    .filter((i) => i.text);
})()"#;

/// Click the dropdown option for `uid` (mimics jieger's mousedown/mouseup/
/// click sequence; returns bool).
pub fn select_live_user_option_js(uid: &str) -> String {
    format!(
        r#"(({uid}) => {{
  const drop = Array.from(document.querySelectorAll(".ant-select-dropdown")).find((n) => n.offsetParent !== null);
  if (!drop) return false;
  const opts = Array.from(drop.querySelectorAll(".ant-select-item-option"));
  const t = opts.find((n) => {{
    const x = ((n.innerText || n.textContent) || "").trim().replace(/\s+/g, " ");
    return x.includes("达人UID：" + {uid}) || x === {uid};
  }});
  if (!t) return false;
  t.dispatchEvent(new MouseEvent("mousedown", {{ bubbles: true }}));
  t.dispatchEvent(new MouseEvent("mouseup", {{ bubbles: true }}));
  t.click();
  return true;
}})({uid})"#,
        uid = js_str(uid)
    )
}

/// Best-effort Escape keydown (antd dropdowns listen for it). Jieger uses
/// `page.keyboard.press('Escape')`; TaskPage has no keyboard API, so this
/// DOM-level dispatch is the equivalent. Failures are ignored by callers.
pub const ESCAPE_JS: &str = r#"(() => { document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); return true; })()"#;

/// Click the option with exact `option_text` inside the `.ad-form-row` whose
/// text contains `row_label` (returns bool; :614 `_click_option_in_row`).
/// This is the JS-fallback half of jieger's implementation, used directly
/// because it is the text-driven, reskin-tolerant path.
pub fn click_option_in_row_js(row_label: &str, option_text: &str) -> String {
    format!(
        r#"(({row}, {opt}) => {{
  const rows = Array.from(document.querySelectorAll(".ad-form-row"));
  const row = rows.find((r) => (r.innerText || "").includes({row}));
  if (!row) return false;
  const norm = (s) => (s || "").trim().replace(/\s+/g, " ");
  const t = Array.from(row.querySelectorAll("label, span, button, div"))
    .find((el) => norm(el.textContent) === norm({opt}));
  if (!t) return false;
  t.dispatchEvent(new MouseEvent("mousedown", {{ bubbles: true }}));
  t.dispatchEvent(new MouseEvent("mouseup", {{ bubbles: true }}));
  t.click();
  return true;
}})({row}, {opt})"#,
        row = js_str(row_label),
        opt = js_str(option_text)
    )
}

/// Fill the row input via the native value setter + input/change events
/// (returns bool; :656 `_fill_input_in_row`). Uses the element's own
/// prototype so it works for both `<input>` and `<textarea>`.
pub fn fill_input_in_row_js(row_label: &str, value: &str) -> String {
    format!(
        r#"(({row}, nextValue) => {{
  const rows = Array.from(document.querySelectorAll(".ad-form-row"));
  const row = rows.find((r) => (r.innerText || "").includes({row}));
  if (!row) return false;
  const input = row.querySelector("input[type='number'], input[type='text'], input:not([type])");
  if (!input) return false;
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), "value")?.set;
  if (!setter) return false;
  input.focus();
  setter.call(input, "");
  input.dispatchEvent(new Event("input", {{ bubbles: true }}));
  setter.call(input, nextValue);
  input.dispatchEvent(new Event("input", {{ bubbles: true }}));
  input.dispatchEvent(new Event("change", {{ bubbles: true }}));
  input.blur();
  return true;
}})({row}, {value})"#,
        row = js_str(row_label),
        value = js_str(value)
    )
}

/// Sync the 净ROI checkbox to `desired`. Returns `"missing"` (row/checkbox
/// absent — caller skips, mirroring jieger's `count > 0` guard), `"toggled"`
/// or `"unchanged"`.
pub fn set_net_roi_js(desired: bool) -> String {
    format!(
        r#"(() => {{
  const rows = Array.from(document.querySelectorAll(".ad-form-row"));
  const row = rows.find((r) => (r.innerText || "").includes("净ROI"));
  if (!row) return "missing";
  const cb = row.querySelector(".ant-checkbox-input");
  if (!cb) return "missing";
  if (!!cb.checked !== {desired}) {{
    cb.click();
    return "toggled";
  }}
  return "unchanged";
}})()"#,
        desired = if desired { "true" } else { "false" }
    )
}

/// Click the first visible element matching `selector` (returns bool).
/// `selector` must be valid CSS (no Playwright `:has-text`).
pub fn click_first_js(selector: &str) -> String {
    format!(
        r#"(() => {{
  const el = Array.from(document.querySelectorAll({sel})).find((n) => n.offsetParent !== null);
  if (!el) return false;
  el.scrollIntoView({{ block: "center" }});
  el.click();
  return true;
}})()"#,
        sel = js_str(selector)
    )
}

/// Count ad-copy textareas (placeholder contains 广告/文案; returns number).
pub const COUNT_AD_COPY_TEXTAREAS_JS: &str = r#"(() => Array.from(document.querySelectorAll("textarea")).filter((t) => {
  const ph = t.getAttribute("placeholder") || "";
  return ph.includes("广告") || ph.includes("文案");
}).length)()"#;

/// Fill the `index`-th ad-copy textarea via the native setter (returns bool).
pub fn fill_ad_copy_js(index: usize, text: &str) -> String {
    format!(
        r#"(({idx}, nextValue) => {{
  const areas = Array.from(document.querySelectorAll("textarea")).filter((t) => {{
    const ph = t.getAttribute("placeholder") || "";
    return ph.includes("广告") || ph.includes("文案");
  }});
  const input = areas[{idx}];
  if (!input) return false;
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(input), "value")?.set;
  if (!setter) return false;
  input.focus();
  setter.call(input, "");
  input.dispatchEvent(new Event("input", {{ bubbles: true }}));
  setter.call(input, nextValue);
  input.dispatchEvent(new Event("input", {{ bubbles: true }}));
  input.dispatchEvent(new Event("change", {{ bubbles: true }}));
  input.blur();
  return true;
}})({idx}, {value})"#,
        idx = index,
        value = js_str(text)
    )
}

/// Dismiss the "取消创编" confirm popup if visible (returns bool; :921).
pub const DISMISS_CANCEL_CONFIRM_JS: &str = r#"(() => {
  const btns = Array.from(document.querySelectorAll(".ant-modal-confirm-btns button"));
  const t = btns.find((b) => b.offsetParent !== null && (b.innerText || "").includes("取消"));
  if (!t) return false;
  t.click();
  return true;
})()"#;

/// State of the 立即推广 submit button: finds the visible exact-text button
/// and clicks it. Returns `"missing"` / `"disabled"` / `"clicked"` (:325).
pub const SUBMIT_BUTTON_JS: &str = r#"(() => {
  const norm = (s) => (s || "").trim().replace(/\s+/g, " ");
  const b = Array.from(document.querySelectorAll("button"))
    .find((x) => x.offsetParent !== null && norm(x.innerText || x.textContent) === "立即推广");
  if (!b) return "missing";
  if (b.disabled) return "disabled";
  b.scrollIntoView({ block: "center" });
  b.click();
  return "clicked";
})()"#;

/// Click 确定 inside the first visible modal (returns bool). Used for the
/// post-submit second confirmation.
pub const SUBMIT_CONFIRM_JS: &str = r#"(() => {
  const selectors = [".ant-modal-wrap", ".ant-modal", "[class*=\"Modal\"]"];
  for (const sel of selectors) {
    const modals = Array.from(document.querySelectorAll(sel)).filter((m) => m.offsetParent !== null);
    for (const m of modals) {
      const btn = Array.from(m.querySelectorAll("button"))
        .find((b) => b.offsetParent !== null && ((b.innerText || b.textContent) || "").trim() === "确定");
      if (btn) { btn.click(); return true; }
    }
  }
  return false;
})()"#;

// ── Live driver ops ─────────────────────────────────────────────────────────

fn task_err(error: TaskError) -> MultizenError {
    MultizenError::Cdp(error.to_string())
}

impl TauriBrowserDriver {
    /// Jinniu isolation gate (mirror of `require_kuaishou_login_scope`,
    /// inverted): Jinniu promote ops only run on `jinniu`-scope profiles.
    pub async fn require_jinniu_scope(&self, profile_id: &str) -> Result<()> {
        let state = self.business_accounts_profile_state(profile_id).await?;
        if state.scope != Some(multizen_core::BusinessProfileScope::Jinniu) {
            return Err(MultizenError::Launch(
                "当前环境不是磁力金牛专用环境，金牛推广操作已拒绝".into(),
            ));
        }
        Ok(())
    }

    /// Scope-gated live session + cancellation token for Jinniu task leases.
    async fn jinniu_session(&self, profile_id: &str) -> Result<(Arc<BrowserSession>, TaskCancel)> {
        self.require_jinniu_scope(profile_id).await?;
        let slot = self.registry.slot(profile_id).await.ok_or_else(|| {
            MultizenError::Mcp(format!(
                "profile `{profile_id}` 当前未运行；请先启动金牛环境"
            ))
        })?;
        let session = slot.session().ok_or_else(|| {
            MultizenError::Mcp(format!(
                "profile `{profile_id}` 会话不可用；请重新启动金牛环境"
            ))
        })?;
        Ok((session, slot.cancel.clone()))
    }

    /// List live tabs as [`PageRef`]s (URL read failures skip the page).
    pub async fn jinniu_promote_pages(&self, profile_id: &str) -> Result<Vec<PageRef>> {
        let (session, _) = self.jinniu_session(profile_id).await?;
        let pages = session
            .browser
            .pages()
            .await
            .map_err(|e| MultizenError::Cdp(format!("pages: {e}")))?;
        let mut out = Vec::new();
        for page in pages {
            let url = page
                .evaluate("location.href")
                .await
                .ok()
                .and_then(|eval| eval.into_value::<serde_json::Value>().ok())
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default();
            out.push(PageRef {
                target_id: page.target_id().as_ref().to_string(),
                url,
            });
        }
        Ok(out)
    }

    /// Acquire a cooperative lease on one target of this profile's session.
    async fn jinniu_task_page<'s>(
        session: &'s Arc<BrowserSession>,
        cancel: TaskCancel,
        target_id: &str,
        timeout: Duration,
    ) -> Result<TaskPage<'s>> {
        session
            .task_page(target_id, cancel, timeout)
            .await
            .map_err(task_err)
    }

    /// Evaluate a JS expression on the leased page.
    async fn jinniu_eval(
        task: &mut TaskPage<'_>,
        expression: &str,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        task.evaluate(expression, timeout).await.map_err(task_err)
    }

    /// Poll until page text appears (browser_service.py:760 `_wait_for_text`).
    async fn jinniu_wait_for_text(
        task: &mut TaskPage<'_>,
        text: &str,
        timeout: Duration,
    ) -> Result<()> {
        let expression = wait_for_text_js(text);
        let deadline = Instant::now() + timeout;
        loop {
            let visible = Self::jinniu_eval(task, &expression, OP_TIMEOUT)
                .await?
                .as_bool()
                .unwrap_or(false);
            if visible {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(MultizenError::Cdp(format!(
                    "等待页面文本超时：{text}"
                )));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Text-driven click helper: exact-first, contains-fallback.
    async fn jinniu_click_button(
        task: &mut TaskPage<'_>,
        button_text: &str,
    ) -> Result<()> {
        let expression = click_button_by_text_js(button_text);
        let clicked = Self::jinniu_eval(task, &expression, OP_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if clicked {
            Ok(())
        } else {
            Err(MultizenError::Cdp(format!(
                "未找到按钮：{button_text}"
            )))
        }
    }

    /// Click an option inside a labeled form row (phase1 building block).
    async fn jinniu_click_option_in_row(
        task: &mut TaskPage<'_>,
        row_label: &str,
        option_text: &str,
    ) -> Result<()> {
        let expression = click_option_in_row_js(row_label, option_text);
        let clicked = Self::jinniu_eval(task, &expression, OP_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if clicked {
            Ok(())
        } else {
            Err(MultizenError::Cdp(format!(
                "未找到“{row_label}”中的选项“{option_text}”"
            )))
        }
    }

    /// Get or open the全站推广创编 page: reuse a loaded storeCreate tab,
    /// else derive `__accountId__` from an account-source page and open a
    /// new tab (with the `homeType=new` assertion + ready wait).
    pub async fn jinniu_open_store_create_tab(&self, profile_id: &str) -> Result<StoreCreateTab> {
        let pages = self.jinniu_promote_pages(profile_id).await?;
        if let Some(found) = find_store_create_page(&pages) {
            let account_id = extract_account_id_from_url(&found.url).ok_or_else(|| {
                MultizenError::Cdp("storeCreate 页面 URL 中缺少 __accountId__".into())
            })?;
            return Ok(StoreCreateTab {
                account_id,
                url: found.url.clone(),
                target_id: found.target_id.clone(),
            });
        }
        let source = find_account_source_page(&pages).ok_or_else(|| {
            MultizenError::Mcp("未找到含 __accountId__ 的页面，请先在金牛后台浏览到首页或推广页".into())
        })?;
        let account_id = extract_account_id_from_url(&source.url)
            .ok_or_else(|| MultizenError::Mcp("未能从 URL 提取 __accountId__".into()))?;
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let target_id = session
            .new_page(&build_store_create_url(&account_id))
            .await
            .map_err(|e| MultizenError::Cdp(format!("open storeCreate: {e}")))?;
        let mut task = Self::jinniu_task_page(&session, cancel, &target_id, OPEN_PAGE_TIMEOUT).await?;
        let current = Self::jinniu_eval(&mut task, "location.href", OP_TIMEOUT)
            .await?
            .as_str()
            .unwrap_or_default()
            .to_string();
        require_home_type_new(&current)?;
        if !current.contains(STORE_CREATE_KEYWORD) {
            return Err(MultizenError::Cdp(format!(
                "新标签页 URL 不含关键字 \"{STORE_CREATE_KEYWORD}\"：{current}"
            )));
        }
        task.wait_for_selector(
            SEL_STORE_READY,
            SelectorState::Visible,
            READY_TIMEOUT,
            POLL_INTERVAL,
        )
        .await
        .map_err(task_err)?;
        tracing::info!(profile = %profile_id, account = %account_id, "jinniu storeCreate tab ready");
        Ok(StoreCreateTab {
            account_id,
            url: current,
            target_id,
        })
    }

    /// Ensure the live-user dropdown is open (:560).
    async fn jinniu_ensure_dropdown_open(task: &mut TaskPage<'_>) -> Result<()> {
        let count = Self::jinniu_eval(task, COUNT_VISIBLE_DROPDOWNS_JS, OP_TIMEOUT)
            .await?
            .as_u64()
            .unwrap_or(0);
        if count > 0 {
            return Ok(());
        }
        task.click(SEL_LIVE_USER_CONTAINER, OP_TIMEOUT)
            .await
            .map_err(task_err)?;
        tokio::time::sleep(UI_SETTLE).await;
        Ok(())
    }

    /// Read dropdown rows as parsed [`JinniuLiveUser`]s (:569 + :597).
    async fn jinniu_read_live_users(task: &mut TaskPage<'_>) -> Result<Vec<JinniuLiveUser>> {
        #[derive(Deserialize)]
        struct RawOption {
            text: String,
            selected: bool,
        }
        let value = Self::jinniu_eval(task, READ_LIVE_USERS_JS, OP_TIMEOUT).await?;
        let raws: Vec<RawOption> = serde_json::from_value(value)
            .map_err(|e| MultizenError::Cdp(format!("live users decode: {e}")))?;
        Ok(raws
            .into_iter()
            .map(|raw| {
                let text = if raw.selected {
                    format!("[selected] {}", raw.text)
                } else {
                    raw.text
                };
                parse_live_user_text(&text)
            })
            .collect())
    }

    /// F1 list: promotable users on the storeCreate page (`getLiveUsers`).
    pub async fn jinniu_live_users(&self, profile_id: &str) -> Result<JinniuLiveUsers> {
        let tab = self.jinniu_open_store_create_tab(profile_id).await?;
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task =
            Self::jinniu_task_page(&session, cancel, &tab.target_id, READY_TIMEOUT).await?;
        Self::jinniu_ensure_dropdown_open(&mut task).await?;
        let users = Self::jinniu_read_live_users(&mut task).await?;
        let _ = Self::jinniu_eval(&mut task, ESCAPE_JS, OP_TIMEOUT).await;
        if users.is_empty() {
            return Err(MultizenError::Mcp("未识别到可选推广用户".into()));
        }
        Ok(JinniuLiveUsers {
            account_id: tab.account_id,
            users,
        })
    }

    /// F1 switch: select the live user with `uid` (`selectLiveUser`).
    /// Already-selected users return immediately without clicking.
    pub async fn jinniu_select_live_user(
        &self,
        profile_id: &str,
        uid: &str,
    ) -> Result<JinniuLiveUser> {
        if uid.trim().is_empty() {
            return Err(MultizenError::Mcp("达人 UID 不能为空".into()));
        }
        let tab = self.jinniu_open_store_create_tab(profile_id).await?;
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task =
            Self::jinniu_task_page(&session, cancel, &tab.target_id, READY_TIMEOUT).await?;
        Self::jinniu_ensure_dropdown_open(&mut task).await?;
        let users = Self::jinniu_read_live_users(&mut task).await?;
        let matched = users.into_iter().find(|user| user.uid == uid);
        let Some(matched) = matched else {
            let _ = Self::jinniu_eval(&mut task, ESCAPE_JS, OP_TIMEOUT).await;
            return Err(MultizenError::Mcp(format!(
                "未找到达人 UID 为 {uid} 的推广用户"
            )));
        };
        if matched.is_selected {
            let _ = Self::jinniu_eval(&mut task, ESCAPE_JS, OP_TIMEOUT).await;
            return Ok(matched);
        }
        let expression = select_live_user_option_js(uid);
        let clicked = Self::jinniu_eval(&mut task, &expression, OP_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if !clicked {
            return Err(MultizenError::Cdp(format!(
                "未能点击达人 UID 为 {uid} 的推广用户"
            )));
        }
        tokio::time::sleep(UI_SETTLE).await;
        tracing::info!(profile = %profile_id, account = %tab.account_id, uid = %uid, name = %matched.display_name, "jinniu live user selected");
        Ok(matched)
    }

    /// F5: plan-build phase1 — 推广方式 / 全站ROI目标 / ROI系数 / 净ROI /
    /// 日预算 / 创意组成方式 → 进入视频库 (`applyStoreCreatePhase1`,
    /// browser_service.py:294).
    pub async fn jinniu_apply_phase1(
        &self,
        profile_id: &str,
        config: &StoreCreatePhase1Config,
    ) -> Result<()> {
        let cfg = config.resolved();
        let tab = self.jinniu_open_store_create_tab(profile_id).await?;
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task =
            Self::jinniu_task_page(&session, cancel, &tab.target_id, VIDEO_LIBRARY_TIMEOUT).await?;
        task.wait_for_selector(
            SEL_STORE_READY,
            SelectorState::Visible,
            READY_TIMEOUT,
            POLL_INTERVAL,
        )
        .await
        .map_err(task_err)?;

        Self::jinniu_click_option_in_row(&mut task, "推广方式", &cfg.promote_type).await?;
        Self::jinniu_wait_for_text(&mut task, "全站ROI目标", OP_TIMEOUT).await?;

        Self::jinniu_click_option_in_row(&mut task, "全站ROI目标", &cfg.roi_target_mode).await?;
        Self::jinniu_wait_for_text(&mut task, "ROI系数", OP_TIMEOUT).await?;

        let expression = fill_input_in_row_js("ROI系数", &cfg.roi_coefficient);
        let filled = Self::jinniu_eval(&mut task, &expression, OP_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if !filled {
            return Err(MultizenError::Cdp("未能填写 ROI 系数".into()));
        }

        let net_roi = Self::jinniu_eval(&mut task, &set_net_roi_js(cfg.enable_net_roi), OP_TIMEOUT)
            .await?
            .as_str()
            .unwrap_or("missing")
            .to_string();
        if net_roi != "missing" && net_roi != "toggled" && net_roi != "unchanged" {
            return Err(MultizenError::Cdp(format!("净ROI 开关异常：{net_roi}")));
        }

        Self::jinniu_click_option_in_row(&mut task, "日预算", &cfg.daily_budget).await?;
        Self::jinniu_click_option_in_row(&mut task, "创意组成方式", &cfg.creative_mode).await?;

        Self::jinniu_wait_for_text(&mut task, "去视频库", OP_TIMEOUT).await?;
        Self::jinniu_click_button(&mut task, "去视频库").await?;
        task.wait_for_selector(
            SEL_VIDEO_LIBRARY_READY,
            SelectorState::Visible,
            VIDEO_LIBRARY_TIMEOUT,
            POLL_INTERVAL,
        )
        .await
        .map_err(task_err)?;
        tracing::info!(profile = %profile_id, account = %tab.account_id, "jinniu phase1 complete, video library open");
        Ok(())
    }

    /// F6: plan-build phase2 — 首个视频卡 + 确认 + 广告文案×3
    /// (`applyStoreCreatePhase2`). Returns the placeholder copies filled
    /// (kslivego default count = 3); the user edits them afterwards.
    pub async fn jinniu_apply_phase2(&self, profile_id: &str) -> Result<Vec<String>> {
        let tab = self.jinniu_open_store_create_tab(profile_id).await?;
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task =
            Self::jinniu_task_page(&session, cancel, &tab.target_id, VIDEO_LIBRARY_TIMEOUT).await?;
        task.wait_for_selector(
            SEL_VIDEO_LIBRARY_READY,
            SelectorState::Visible,
            READY_TIMEOUT,
            POLL_INTERVAL,
        )
        .await
        .map_err(task_err)?;

        let picked = Self::jinniu_eval(&mut task, &click_first_js(SEL_VIDEO_CARD), OP_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if !picked {
            return Err(MultizenError::Cdp("未找到可选择的视频卡片".into()));
        }
        tokio::time::sleep(UI_SETTLE).await;

        // Confirm button (确定 preferred, 确认 fallback); absent = skip.
        let confirmed = Self::jinniu_eval(&mut task, &click_button_by_text_js("确定"), OP_TIMEOUT)
            .await?
            .as_bool()
            .unwrap_or(false);
        if !confirmed {
            let _ = Self::jinniu_eval(&mut task, &click_button_by_text_js("确认"), OP_TIMEOUT).await;
        }
        tokio::time::sleep(Duration::from_millis(800)).await;

        let _ = Self::jinniu_eval(&mut task, DISMISS_CANCEL_CONFIRM_JS, OP_TIMEOUT).await;

        let count = Self::jinniu_eval(&mut task, COUNT_AD_COPY_TEXTAREAS_JS, OP_TIMEOUT)
            .await?
            .as_u64()
            .unwrap_or(0) as usize;
        let target = count.min(3);
        // Millis seed: placeholder text only, determinism not required live.
        let seed_base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut filled = Vec::with_capacity(target);
        for index in 0..target {
            let text = generate_ad_copy(seed_base.wrapping_add(index as u64), 15);
            let ok = Self::jinniu_eval(&mut task, &fill_ad_copy_js(index, &text), OP_TIMEOUT)
                .await?
                .as_bool()
                .unwrap_or(false);
            if !ok {
                return Err(MultizenError::Cdp(format!("第 {index} 个广告文案填写失败")));
            }
            filled.push(text);
        }
        tracing::info!(profile = %profile_id, account = %tab.account_id, filled = filled.len(), "jinniu phase2 complete");
        Ok(filled)
    }

    /// F7: submit — 点击"立即推广" + 弹窗二次确认 (`submitStoreCreate`,
    /// browser_service.py:323).
    pub async fn jinniu_submit_store_create(&self, profile_id: &str) -> Result<()> {
        let tab = self.jinniu_open_store_create_tab(profile_id).await?;
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task =
            Self::jinniu_task_page(&session, cancel, &tab.target_id, READY_TIMEOUT).await?;
        let state = Self::jinniu_eval(&mut task, SUBMIT_BUTTON_JS, OP_TIMEOUT)
            .await?
            .as_str()
            .unwrap_or("missing")
            .to_string();
        match state.as_str() {
            "clicked" => {}
            "disabled" => {
                return Err(MultizenError::Mcp("“立即推广”按钮当前不可点击".into()));
            }
            _ => {
                return Err(MultizenError::Mcp("未找到“立即推广”按钮".into()));
            }
        }
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let _ = Self::jinniu_eval(&mut task, SUBMIT_CONFIRM_JS, OP_TIMEOUT).await;
        tracing::info!(profile = %profile_id, account = %tab.account_id, "jinniu plan submitted");
        Ok(())
    }

    /// Find the first visible modal containing `text`; returns its text
    /// (browser_service.py:1635). `Ok(None)` when no modal matches — absence
    /// is a normal outcome, not an error.
    pub async fn jinniu_find_visible_modal_by_text(
        &self,
        profile_id: &str,
        target_id: &str,
        text: &str,
    ) -> Result<Option<String>> {
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task = Self::jinniu_task_page(&session, cancel, target_id, OP_TIMEOUT).await?;
        let value = Self::jinniu_eval(&mut task, &find_visible_modal_js(text), OP_TIMEOUT).await?;
        Ok(value.as_str().map(str::to_string))
    }

    /// Click by text within `scope_selector`
    /// (browser_service.py:1203 `_click_text_in_scope`).
    pub async fn jinniu_click_text_in_scope(
        &self,
        profile_id: &str,
        target_id: &str,
        scope_selector: &str,
        text: &str,
    ) -> Result<()> {
        let (session, cancel) = self.jinniu_session(profile_id).await?;
        let mut task = Self::jinniu_task_page(&session, cancel, target_id, OP_TIMEOUT).await?;
        let clicked =
            Self::jinniu_eval(&mut task, &click_text_in_scope_js(scope_selector, text), OP_TIMEOUT)
                .await?
                .as_bool()
                .unwrap_or(false);
        if clicked {
            Ok(())
        } else {
            Err(MultizenError::Cdp(format!(
                "scope 内未找到文本：{text}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_account_id_from_url_reads_param() {
        assert_eq!(
            extract_account_id_from_url(
                "https://niu.e.kuaishou.com/storeCreate?__accountId__=12345&homeType=new"
            ),
            Some("12345".into())
        );
        // Param order / extra params do not matter.
        assert_eq!(
            extract_account_id_from_url(
                "https://niu.e.kuaishou.com/home?foo=1&__accountId__=abc-9#frag"
            ),
            Some("abc-9".into())
        );
        // Missing param.
        assert_eq!(
            extract_account_id_from_url("https://niu.e.kuaishou.com/home?homeType=new"),
            None
        );
        // Empty value is not an account id.
        assert_eq!(
            extract_account_id_from_url("https://niu.e.kuaishou.com/home?__accountId__="),
            None
        );
        // Invalid URL.
        assert_eq!(extract_account_id_from_url("not a url"), None);
        assert_eq!(extract_account_id_from_url(""), None);
    }

    #[test]
    fn store_create_url_forces_home_type_new() {
        let url = build_store_create_url("987");
        assert!(url.contains("__accountId__=987"));
        assert!(home_type_is_new(&url));
        assert!(require_home_type_new(&url).is_ok());
    }

    #[test]
    fn home_type_assertion_rejects_non_new() {
        // Missing homeType.
        assert!(require_home_type_new("https://niu.e.kuaishou.com/storeCreate?__accountId__=1").is_err());
        // Wrong generation pin (old UI generation, not the legacy创编 page).
        assert!(require_home_type_new(
            "https://niu.e.kuaishou.com/storeCreate?__accountId__=1&homeType=old"
        )
        .is_err());
        assert!(!home_type_is_new("https://niu.e.kuaishou.com/storeCreate"));
        assert!(!home_type_is_new("not a url"));
    }

    #[test]
    fn find_store_create_page_prefers_newest_match() {
        let pages = vec![
            PageRef::new("t1", "https://niu.e.kuaishou.com/home?__accountId__=1"),
            PageRef::new(
                "t2",
                "https://niu.e.kuaishou.com/storeCreate?__accountId__=1&homeType=new",
            ),
            PageRef::new(
                "t3",
                "https://niu.e.kuaishou.com/storeCreate?__accountId__=2&homeType=new",
            ),
        ];
        assert_eq!(
            find_store_create_page(&pages).map(|p| p.target_id.as_str()),
            Some("t3")
        );
        let other = vec![PageRef::new("t1", "https://example.com/")];
        assert_eq!(find_store_create_page(&other), None);
        assert_eq!(find_store_create_page(&[]), None);
    }

    #[test]
    fn find_account_source_page_prefers_home_create_manage() {
        let fallback = PageRef::new(
            "t-fallback",
            "https://niu.e.kuaishou.com/report?__accountId__=7",
        );
        let home = PageRef::new("t-home", "https://niu.e.kuaishou.com/home?__accountId__=7");
        // Newest-first would pick t-home (later) only if preferred; put the
        // fallback later to prove tier priority beats recency.
        let pages = vec![home.clone(), fallback.clone()];
        assert_eq!(
            find_account_source_page(&pages).map(|p| p.target_id.as_str()),
            Some("t-home")
        );
        // storeManage also preferred.
        let manage = PageRef::new(
            "t-manage",
            "https://niu.e.kuaishou.com/storeManage?__accountId__=7",
        );
        let pages = vec![fallback.clone(), manage.clone()];
        assert_eq!(
            find_account_source_page(&pages).map(|p| p.target_id.as_str()),
            Some("t-manage")
        );
        // Fallback tier still works when nothing preferred exists.
        assert_eq!(
            find_account_source_page(std::slice::from_ref(&fallback))
                .map(|p| p.target_id.as_str()),
            Some("t-fallback")
        );
        // Pages without __accountId__ never qualify.
        let naked = vec![
            PageRef::new("t1", "https://niu.e.kuaishou.com/home"),
            PageRef::new("t2", "https://example.com/"),
        ];
        assert_eq!(find_account_source_page(&naked), None);
        assert_eq!(find_account_source_page(&[]), None);
    }

    #[test]
    fn parse_live_user_text_covers_jieger_shapes() {
        // Selected row, full-width colon.
        let user = parse_live_user_text("[selected] 张三 的直播间 达人UID：123456");
        assert_eq!(user.uid, "123456");
        assert_eq!(user.display_name, "张三");
        assert!(user.is_selected);
        assert_eq!(user.full_text, "张三 的直播间 达人UID：123456");
        // Half-width colon, unselected.
        let user = parse_live_user_text("李四 的直播间 达人UID:98765");
        assert_eq!(user.uid, "98765");
        assert_eq!(user.display_name, "李四");
        assert!(!user.is_selected);
        // No UID marker: uid falls back to the full text.
        let user = parse_live_user_text("未知主播 的直播间");
        assert_eq!(user.uid, "未知主播 的直播间");
        assert_eq!(user.display_name, "未知主播");
        // No whitespace before 的直播间: name falls back (mirrors \s+ regex).
        let user = parse_live_user_text("王五的直播间 达人UID：111");
        assert_eq!(user.uid, "111");
        assert_eq!(user.display_name, "王五的直播间 达人UID：111");
        // Whitespace runs collapse.
        let user = parse_live_user_text("  赵六   的直播间   达人UID：222  ");
        assert_eq!(user.uid, "222");
        assert_eq!(user.display_name, "赵六");
    }

    #[test]
    fn match_button_text_prefers_exact_match() {
        let buttons = vec![
            "去视频库看看".to_string(),
            "去视频库".to_string(),
            "取消".to_string(),
        ];
        assert_eq!(match_button_text(&buttons, "去视频库"), Some(1));
        // Contains fallback when no exact match exists.
        assert_eq!(match_button_text(&buttons, "视频库"), Some(0));
        assert_eq!(match_button_text(&buttons, " 取消 "), Some(2));
        assert_eq!(match_button_text(&buttons, "立即推广"), None);
        assert_eq!(match_button_text(&buttons, ""), None);
        assert_eq!(match_button_text(&[], "确定"), None);
    }

    #[test]
    fn js_builders_escape_text_safely() {
        let tricky = "确\"定\\换行\n";
        let expr = click_button_by_text_js(tricky);
        assert!(expr.contains(&js_str(tricky)));
        assert!(expr.contains("scrollIntoView"));

        let expr = wait_for_text_js("全站ROI目标");
        assert!(expr.contains(&js_str("全站ROI目标")));

        let expr = find_visible_modal_js("确定");
        for sel in MODAL_SELECTORS {
            assert!(expr.contains(&js_str(sel)));
        }

        let expr = click_text_in_scope_js(".ant-modal-wrap", "确定");
        assert!(expr.contains(&js_str(".ant-modal-wrap")));
        assert!(expr.contains(&js_str("确定")));

        let expr = click_option_in_row_js("推广方式", "长效推广");
        assert!(expr.contains(&js_str("推广方式")));
        assert!(expr.contains(&js_str("长效推广")));

        let expr = fill_input_in_row_js("ROI系数", "5");
        assert!(expr.contains(&js_str("ROI系数")));

        let expr = select_live_user_option_js("123");
        assert!(expr.contains(&js_str("123")));
        assert!(expr.contains("达人UID："));

        assert!(set_net_roi_js(true).contains("true"));
        assert!(set_net_roi_js(false).contains("false"));

        let expr = fill_ad_copy_js(2, "热卖");
        assert!(expr.contains(&js_str("热卖")));

        for expr in [SUBMIT_CONFIRM_JS, DISMISS_CANCEL_CONFIRM_JS] {
            assert!(expr.contains("return true"));
            assert!(expr.contains("return false"));
        }
        for state in ["missing", "disabled", "clicked"] {
            assert!(SUBMIT_BUTTON_JS.contains(state));
        }
    }

    #[test]
    fn phase1_config_applies_jieger_defaults() {
        let cfg = StoreCreatePhase1Config::default().resolved();
        assert!(!cfg.enable_net_roi);
        assert_eq!(cfg.daily_budget, "1000");
        assert_eq!(cfg.roi_coefficient, "5");
        assert_eq!(cfg.promote_type, "长效推广");
        assert_eq!(cfg.roi_target_mode, "自定义");
        assert_eq!(cfg.creative_mode, "程序化创意");

        let cfg = StoreCreatePhase1Config {
            enable_net_roi: Some(true),
            daily_budget: Some("500".into()),
            ..Default::default()
        }
        .resolved();
        assert!(cfg.enable_net_roi);
        assert_eq!(cfg.daily_budget, "500");
        assert_eq!(cfg.roi_coefficient, "5");
    }

    #[test]
    fn ad_copy_uses_fixed_charset_and_length() {
        const CHARSET: &str = "热卖好物精选优惠抢购上新限时折扣品质推荐爆款体验福利秒杀";
        let copy = generate_ad_copy(42, 15);
        assert_eq!(copy.chars().count(), 15);
        assert!(copy.chars().all(|c| CHARSET.contains(c)));
        assert_eq!(generate_ad_copy(0, 0), "");
        // Different seeds diverge (placeholder variety for the 3 copies).
        assert_ne!(generate_ad_copy(1, 15), generate_ad_copy(2, 15));
        // Same seed is stable.
        assert_eq!(generate_ad_copy(7, 15), generate_ad_copy(7, 15));
    }

    #[test]
    fn selector_reference_table_matches_jieger() {
        assert_eq!(ACCOUNT_ID_PARAM, "__accountId__");
        assert_eq!(SEL_LIVE_USER_CONTAINER, "#authorize-user-select .ant-select");
        assert_eq!(SEL_STORE_READY, ".ad-form-row");
        assert_eq!(SEL_SUBMIT_BUTTON, "button:has-text(\"立即推广\")");
        assert_eq!(SEL_VIDEO_CONFIRM_BUTTON, "button:has-text(\"确定\"), button:has-text(\"确认\")");
        assert!(ACCOUNT_SOURCE_KEYWORDS.contains(&"home"));
    }
}
