//! Kuaishou viewer sub-accounts (小号) — audience identity on the
//! kuaishou.com main site, distinct from shop-console (小店中控) accounts.
//!
//! Sub-accounts reuse `business_accounts` with `kind = kuaishou_sub`; no
//! historical data is migrated. Browser automation here is viewer-side only
//! (enter a live room, send danmaku); it never touches shop or seller flows.
//!
//! Testability: all DOM/keyword scoring, delay math, and constants are pure
//! functions covered below. The async flows need a live browser session and
//! are compile-checked only.

use super::{LauncherCmd, TauriBrowserDriver};
use cdp_driver::{session::BrowserSession, TaskCancel, TaskError, TaskPage};
use multizen_core::{BusinessAccount, BusinessAccountKind, MultizenError, Result};
use profile_manager::scenes::{RecordInteractionInput, SceneLineAction, SubAccountInteraction};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, Mutex as AsyncMutex, Semaphore};

// --- Site config -----------------------------------------------------------

/// Viewer entry on the kuaishou.com main site. Login itself is interactive
/// (the user scans a QR code in the visible browser window); the driver only
/// opens the entry and polls for the resulting login state.
#[derive(Debug, Clone, Copy)]
pub struct SubAccountSiteConfig {
    pub site_url: &'static str,
    pub login_url: &'static str,
}

pub const KUAISHOU_SUB_ACCOUNT_CONFIG: SubAccountSiteConfig = SubAccountSiteConfig {
    site_url: "https://www.kuaishou.com",
    login_url: "https://www.kuaishou.com/",
};

// --- Timing / concurrency budgets ------------------------------------------

/// Background login poll interval.
pub const LOGIN_POLL_INTERVAL_MS: u64 = 3_000;
/// Window to observe a usable login page before reporting it not ready.
pub const KS_SUB_LOGIN_DETECT_MS: u64 = 30_000;
/// Grace period for the user to solve a verification challenge manually
/// before the driver reports `SubAccountVerificationRequiredError`.
pub const KS_VERIFY_MS: u64 = 20_000;
/// Total login campaign budget.
pub const KS_LOGIN_MS: u64 = 180_000;
/// Settle time after closing an owned page so CDP target removal propagates.
pub const BROWSER_CLOSE_GRACE_MS: u64 = 150;
/// Max concurrent `batch_login` auth campaigns (anti-risk-control cap).
pub const MAX_CONCURRENT_LOGINS: usize = 3;
/// `enter_live_room` attempts (rate-limit retries included).
pub const RATE_LIMIT_RETRIES: u32 = 5;
/// Random settle delay after a room becomes ready.
pub const LIVE_ROOM_SETTLE_MIN_MS: u64 = 3_000;
pub const LIVE_ROOM_SETTLE_MAX_MS: u64 = 8_000;
/// Per-evaluate timeout inside task leases.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Task-lease acquisition timeout for sub-account operations.
pub const TASK_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
/// Upper bound for `waitForLiveRoomReady` polling.
pub const LIVE_ROOM_READY_TIMEOUT_MS: u64 = 30_000;
/// Client-side guard for danmaku length (platform limits are shorter; this
/// only rejects obviously invalid input before any browser work).
pub const MAX_DANMAKU_CHARS: usize = 500;

// --- Security verification --------------------------------------------------

/// Page-text keywords that indicate a verification challenge. All nine are
/// matched; see `check_security_verification`.
pub const SECURITY_VERIFICATION_KEYWORDS: [&str; 9] = [
    "安全验证",
    "滑块验证",
    "拼图验证",
    "点选验证",
    "短信验证",
    "图形验证",
    "人脸验证",
    "实名验证",
    "账号异常",
];

/// DOM selectors that indicate a verification challenge. All eight are
/// probed; see `security_check_script`.
pub const SECURITY_VERIFICATION_SELECTORS: [&str; 8] = [
    ".captcha-container",
    ".captcha-verify",
    "#captcha",
    ".slider-verify",
    ".nc-container",
    ".geetest_holder",
    ".yidun",
    "iframe[src*='captcha']",
];

/// Rate-limit signals that make `enter_live_room` back off and retry.
pub const RATE_LIMIT_KEYWORDS: [&str; 4] =
    ["操作频繁", "稍后再试", "访问频繁", "系统繁忙"];

/// Raised when a verification challenge blocks an interaction. Surfaced to
/// `MultizenError::Config` with a stable prefix (see `verification_error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAccountVerificationRequiredError {
    pub hits: Vec<String>,
}

impl fmt::Display for SubAccountVerificationRequiredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "小号安全验证（{}），请人工完成验证后重试",
            self.hits.join("、")
        )
    }
}

impl std::error::Error for SubAccountVerificationRequiredError {}

const VERIFICATION_PREFIX: &str = "小号安全验证：";

pub fn verification_error(hits: Vec<String>) -> MultizenError {
    MultizenError::Config(format!(
        "{VERIFICATION_PREFIX}{}，请人工完成验证后重试",
        hits.join("、")
    ))
}

/// Classify a driver error as a verification challenge. Used by future
/// UI/MCP callers that only see the stringified command error.
#[allow(dead_code)] // No in-crate reader yet; kept for IPC consumers.
pub fn is_verification_required_error(error: &MultizenError) -> bool {
    matches!(error, MultizenError::Config(message) if message.starts_with(VERIFICATION_PREFIX))
}

/// Pure combiner: keyword hits in page text plus present DOM selectors.
/// Both lists are matched in full (9 keywords + 8 selectors).
pub fn check_security_verification(
    page_text: &str,
    matched_selectors: &[&str],
) -> std::result::Result<(), SubAccountVerificationRequiredError> {
    let mut hits = Vec::new();
    for keyword in SECURITY_VERIFICATION_KEYWORDS {
        if page_text.contains(keyword) {
            hits.push(keyword.to_string());
        }
    }
    for selector in matched_selectors {
        if SECURITY_VERIFICATION_SELECTORS.contains(selector) {
            hits.push((*selector).to_string());
        }
    }
    if hits.is_empty() {
        Ok(())
    } else {
        Err(SubAccountVerificationRequiredError { hits })
    }
}

pub fn is_rate_limited(page_text: &str) -> bool {
    RATE_LIMIT_KEYWORDS.iter().any(|w| page_text.contains(w))
}

// --- Page-state scripts ------------------------------------------------------

/// Returns `{text, matched}` where `matched` lists the verification
/// selectors present in the DOM. Shared by login/enter/send probes.
pub fn security_check_script() -> String {
    let selectors = serde_json::to_string(&SECURITY_VERIFICATION_SELECTORS)
        .expect("serializing string literals cannot fail");
    format!(
        r#"(() => {{
            const selectors = {selectors};
            const matched = selectors.filter((s) => {{
                try {{ return !!document.querySelector(s); }} catch (e) {{ return false; }}
            }});
            const text = document.body ? document.body.innerText.slice(0, 8000) : '';
            return {{text: text, matched: matched}};
        }})()"#
    )
}

/// Login probe: `{pageOk, loggedIn, text}`.
pub const LOGIN_STATE_JS: &str = r#"(() => {
    const t = document.body ? document.body.innerText.slice(0, 8000) : '';
    const avatar = !!document.querySelector('.user-avatar, .header-avatar, [class*="user-info"]');
    return {pageOk: !!document.body, loggedIn: avatar || /我的关注|个人中心|退出登录/.test(t), text: t};
})()"#;

/// Live-room readiness: `{playing, liveText, url}`.
pub const LIVE_ROOM_READY_JS: &str = r#"(() => {
    const v = document.querySelector('video');
    const t = document.body ? document.body.innerText.slice(0, 4000) : '';
    return {playing: !!v && v.readyState > 0 && !v.paused && !v.ended,
        liveText: /直播中|LIVE/.test(t), url: location.href};
})()"#;

/// Tags each comment-input candidate with `data-mz-subidx` and returns
/// descriptors for Rust-side scoring (`pick_input`).
pub const COLLECT_INPUT_CANDIDATES_JS: &str = r#"(() => {
    const out = [];
    let i = 0;
    const vis = (el) => { const s = getComputedStyle(el); const r = el.getBoundingClientRect();
        return s.display !== 'none' && s.visibility !== 'hidden' && r.width > 0 && r.height > 0; };
    document.querySelectorAll('input, textarea, [contenteditable="true"]').forEach((el) => {
        if (el.type === 'hidden' || el.disabled || el.readOnly) return;
        const key = 'mzi' + (i++);
        el.setAttribute('data-mz-subidx', key);
        out.push({key: key, tag: (el.tagName || '').toLowerCase(),
            editable: el.isContentEditable === true, visible: vis(el),
            placeholder: el.getAttribute('placeholder') || '',
            label: el.getAttribute('aria-label') || ''});
    });
    return out;
})()"#;

/// Tags each send-button candidate with `data-mz-subidx` for `pick_send_button`.
pub const COLLECT_SEND_CANDIDATES_JS: &str = r#"(() => {
    const out = [];
    let i = 0;
    const vis = (el) => { const s = getComputedStyle(el); const r = el.getBoundingClientRect();
        return s.display !== 'none' && s.visibility !== 'hidden' && r.width > 0 && r.height > 0; };
    document.querySelectorAll('button, [role="button"], input[type="submit"]').forEach((el) => {
        if (el.disabled) return;
        const key = 'mzs' + (i++);
        el.setAttribute('data-mz-subidx', key);
        out.push({key: key, tag: (el.tagName || '').toLowerCase(), visible: vis(el),
            text: ((el.innerText || el.value || '').trim().slice(0, 20)),
            cls: (el.className && el.className.baseVal !== undefined ? '' : (el.className || '')).toString().slice(0, 120)});
    });
    return out;
})()"#;

/// Fill script compatible with input/textarea/contenteditable. Returns
/// `{ok, actual}` for readback comparison.
pub fn fill_comment_input_script(key: &str, text: &str) -> String {
    let key = serde_json::to_string(key).expect("serializing a string cannot fail");
    let text = serde_json::to_string(text).expect("serializing a string cannot fail");
    format!(
        r#"((key, text) => {{
            const el = document.querySelector('[data-mz-subidx="' + key + '"]');
            if (!el) return {{ok: false, actual: ''}};
            el.focus();
            const tag = (el.tagName || '').toLowerCase();
            if (tag === 'input' || tag === 'textarea') {{
                const proto = tag === 'input' ? window.HTMLInputElement.prototype : window.HTMLTextAreaElement.prototype;
                Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, text);
                el.dispatchEvent(new Event('input', {{bubbles: true}}));
                el.dispatchEvent(new Event('change', {{bubbles: true}}));
                return {{ok: true, actual: el.value}};
            }}
            if (el.isContentEditable) {{
                document.execCommand('selectAll', false, null);
                const inserted = document.execCommand('insertText', false, text);
                el.dispatchEvent(new InputEvent('input', {{bubbles: true, data: text}}));
                return {{ok: inserted, actual: el.innerText || ''}};
            }}
            return {{ok: false, actual: ''}};
        }})({key}, {text})"#
    )
}

/// Returns `{cleared, actual}` for the post-send emptiness check.
pub fn cleared_check_script(key: &str) -> String {
    let key = serde_json::to_string(key).expect("serializing a string cannot fail");
    format!(
        r#"((key) => {{
            const el = document.querySelector('[data-mz-subidx="' + key + '"]');
            if (!el) return {{cleared: false, actual: ''}};
            const tag = (el.tagName || '').toLowerCase();
            const actual = (tag === 'input' || tag === 'textarea') ? el.value : (el.innerText || '');
            return {{cleared: actual.trim() === '', actual: actual}};
        }})({key})"#
    )
}

/// Synthetic Enter fallback when no send button exists.
pub fn press_enter_script(key: &str) -> String {
    let key = serde_json::to_string(key).expect("serializing a string cannot fail");
    format!(
        r#"((key) => {{
            const el = document.querySelector('[data-mz-subidx="' + key + '"]');
            if (!el) return false;
            el.focus();
            for (const type of ['keydown', 'keypress', 'keyup']) {{
                el.dispatchEvent(new KeyboardEvent(type, {{key: 'Enter', code: 'Enter', keyCode: 13, bubbles: true}}));
            }}
            return true;
        }})({key})"#
    )
}

// --- Candidate scoring -------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct InputCandidate {
    pub key: String,
    pub tag: String,
    #[serde(default)]
    pub editable: bool,
    #[serde(default)]
    pub visible: bool,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendCandidate {
    pub key: String,
    pub tag: String,
    #[serde(default)]
    pub visible: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub cls: String,
}

const COMMENT_HINTS: [&str; 6] = ["说点什么", "评论", "弹幕", "发言", "聊一聊", "comment"];
const SEND_TEXT_EXACT: [&str; 3] = ["发送", "发表", "评论"];
const SEND_TEXT_PART: [&str; 4] = ["发送", "发表", "评论", "弹幕"];
const SEND_CLASS_HINTS: [&str; 3] = ["send", "submit", "comment"];

/// Score a comment-input candidate; `None` means unusable (hidden or not
/// text-fillable). Higher is better.
pub fn score_input_candidate(candidate: &InputCandidate) -> Option<i32> {
    if !candidate.visible {
        return None;
    }
    let mut score = 0i32;
    match candidate.tag.as_str() {
        "input" | "textarea" => score += 2,
        _ if candidate.editable => score += 3,
        _ => return None,
    }
    let haystack = format!("{}{}", candidate.placeholder, candidate.label);
    if COMMENT_HINTS.iter().any(|h| haystack.contains(h)) {
        score += 4;
    } else if !candidate.placeholder.is_empty() || !candidate.label.is_empty() {
        score += 1;
    }
    Some(score)
}

/// Pick the best comment-input key, or `None` when nothing is usable.
/// Ties keep the first candidate for determinism.
pub fn pick_input(candidates: &[InputCandidate]) -> Option<String> {
    let mut best: Option<(i32, &str)> = None;
    for candidate in candidates {
        if let Some(score) = score_input_candidate(candidate) {
            if best.is_none_or(|(top, _)| score > top) {
                best = Some((score, candidate.key.as_str()));
            }
        }
    }
    best.map(|(_, key)| key.to_string())
}

/// Score a send-button candidate; `None` means unusable. A visible
/// icon-only button with a send-like class still scores so it can be
/// clicked; a fully anonymous element falls back to Enter.
pub fn score_send_candidate(candidate: &SendCandidate) -> Option<i32> {
    if !candidate.visible {
        return None;
    }
    let mut score = 0i32;
    if candidate.tag == "button" {
        score += 1;
    }
    if SEND_TEXT_EXACT.iter().any(|w| candidate.text == *w) {
        score += 4;
    } else if SEND_TEXT_PART.iter().any(|w| candidate.text.contains(w)) {
        score += 2;
    } else if candidate.text.is_empty() {
        let cls = candidate.cls.to_lowercase();
        if SEND_CLASS_HINTS.iter().any(|h| cls.contains(h)) {
            score += 2;
        } else {
            return None;
        }
    }
    Some(score)
}

pub fn pick_send_button(candidates: &[SendCandidate]) -> Option<String> {
    let mut best: Option<(i32, &str)> = None;
    for candidate in candidates {
        if let Some(score) = score_send_candidate(candidate) {
            if best.is_none_or(|(top, _)| score > top) {
                best = Some((score, candidate.key.as_str()));
            }
        }
    }
    best.map(|(_, key)| key.to_string())
}

// --- Delay math -----------------------------------------------------------------

/// Map a `[0,1)` sample to the 3–8s room-settle window. Pure for testing.
pub fn live_room_settle_delay_ms(random01: f64) -> u64 {
    let clamped = random01.clamp(0.0, 1.0);
    LIVE_ROOM_SETTLE_MIN_MS
        + ((LIVE_ROOM_SETTLE_MAX_MS - LIVE_ROOM_SETTLE_MIN_MS) as f64 * clamped) as u64
}

pub fn random_settle_delay_ms() -> u64 {
    live_room_settle_delay_ms(rand::random::<f64>())
}

// --- IPC-facing types -----------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAccountLoginResult {
    pub account_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

impl SubAccountLoginResult {
    pub fn ok(account_id: &str) -> Self {
        Self {
            account_id: account_id.into(),
            ok: true,
            error: None,
        }
    }

    pub fn fail(account_id: &str, error: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            ok: false,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone)]
struct OwnedPage {
    profile_id: String,
    target: String,
    live_url: String,
}

// --- Runtime -----------------------------------------------------------------------

pub(in crate::driver) struct SubAccountRuntime {
    pub(in crate::driver) stop: TaskCancel,
    started: AtomicBool,
    /// Per-account serial locks: enter + send for one account never interleave.
    send_locks: StdMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    /// `batch_login` concurrency cap (anti-risk-control).
    login_permits: Arc<Semaphore>,
    /// Owned live-room pages per account, created by `enter_live_room`.
    pages: StdMutex<HashMap<String, OwnedPage>>,
}

impl SubAccountRuntime {
    pub fn new() -> Self {
        Self {
            stop: TaskCancel::new(),
            started: AtomicBool::new(false),
            send_locks: StdMutex::new(HashMap::new()),
            login_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_LOGINS)),
            pages: StdMutex::new(HashMap::new()),
        }
    }

    fn send_lock(&self, account_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.send_locks.lock().unwrap_or_else(|p| p.into_inner());
        locks
            .entry(account_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    fn owned_page(&self, account_id: &str) -> Option<OwnedPage> {
        self.pages
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(account_id)
            .cloned()
    }

    /// Insert, returning the replaced page (caller closes it) if any.
    fn set_owned_page(&self, account_id: &str, page: OwnedPage) -> Option<OwnedPage> {
        self.pages
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(account_id.to_string(), page)
    }

    fn remove_owned_page(&self, account_id: &str) {
        self.pages
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(account_id);
    }
}

fn task_error(error: TaskError) -> MultizenError {
    match error {
        TaskError::Driver(inner) => inner,
        other => MultizenError::Cdp(format!("小号页面任务失败：{other}")),
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct SecurityProbe {
    text: String,
    #[serde(default)]
    matched: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LoginProbe {
    #[serde(default)]
    page_ok: bool,
    #[serde(default)]
    logged_in: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RoomProbe {
    #[serde(default)]
    playing: bool,
    #[serde(default)]
    live_text: bool,
}

impl TauriBrowserDriver {
    // --- CRUD (小号观众身份固定 kind=kuaishou_sub) -----------------------------

    /// Save a viewer sub-account. Any incoming kind is corrected to
    /// `KuaishouSub` — sub-account audience identity must never become a
    /// shop-console record.
    pub async fn save_sub_account(
        &self,
        mut input: multizen_core::SaveBusinessAccountInput,
    ) -> Result<BusinessAccount> {
        input.kind = BusinessAccountKind::KuaishouSub;
        self.business_accounts_save(input).await
    }

    /// List viewer sub-accounts only (shop/live/mate/jinniu excluded).
    pub async fn list_sub_accounts(&self) -> Result<Vec<BusinessAccount>> {
        Ok(self
            .business_accounts_list()
            .await?
            .into_iter()
            .filter(|a| a.kind == BusinessAccountKind::KuaishouSub)
            .collect())
    }

    pub async fn unbind_sub_account(&self, id: &str) -> Result<()> {
        self.business_accounts_unbind(id).await
    }

    async fn require_sub_profile(&self, account_id: &str) -> Result<(BusinessAccount, String)> {
        let account = self
            .business_accounts_list()
            .await?
            .into_iter()
            .find(|a| a.id == account_id && a.kind == BusinessAccountKind::KuaishouSub)
            .ok_or_else(|| {
                MultizenError::Config(format!("小号 {account_id} 不存在或非小号身份"))
            })?;
        let profile_id = account.profile_id.clone().ok_or_else(|| {
            MultizenError::Config(format!("小号 {account_id} 未绑定环境，请先绑定"))
        })?;
        Ok((account, profile_id))
    }

    // --- Interaction records (launcher-thread SQLite) ---------------------------

    pub async fn record_interaction(
        &self,
        input: RecordInteractionInput,
    ) -> Result<SubAccountInteraction> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::RecordInteraction { input, resp })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    pub async fn sub_account_interactions(
        &self,
        account_id: &str,
    ) -> Result<Vec<SubAccountInteraction>> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::ListInteractions {
                account_id: account_id.into(),
                resp,
            })
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread closed".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("launcher thread dropped response".into()))?
    }

    // --- Login ---------------------------------------------------------------------

    /// Open the viewer login entry (visible tab for QR scan) and poll every
    /// 3s: detect window 30s, manual-verification grace 20s, total budget
    /// 3min. The login tab is closed on success and left open on failure so
    /// the user can finish manually.
    pub async fn ensure_auth(
        &self,
        profile_id: &str,
        config: &SubAccountSiteConfig,
    ) -> Result<()> {
        let session = self.require_session(profile_id).await?;
        let bound = session
            .new_bound_page(config.login_url)
            .await
            .map_err(|e| MultizenError::Cdp(format!("小号登录页打开失败：{e}")))?;
        let target = bound.target_id().to_string();
        session
            .activate_page(&target)
            .await
            .map_err(|e| MultizenError::Cdp(format!("小号登录页前置失败：{e}")))?;
        let mut task = session
            .task_page(&target, self.sub_account.stop.clone(), TASK_ACQUIRE_TIMEOUT)
            .await
            .map_err(task_error)?;
        let start = Instant::now();
        let total = Duration::from_millis(KS_LOGIN_MS);
        let detect = Duration::from_millis(KS_SUB_LOGIN_DETECT_MS);
        let verify_grace = Duration::from_millis(KS_VERIFY_MS);
        let poll = Duration::from_millis(LOGIN_POLL_INTERVAL_MS);
        let mut page_ok = false;
        let mut verify_since: Option<Instant> = None;
        loop {
            let elapsed = start.elapsed();
            if elapsed >= total {
                return Err(MultizenError::Config(format!(
                    "小号登录超时（{}分钟），请确认已在浏览器中完成扫码登录后重试",
                    KS_LOGIN_MS / 60_000
                )));
            }
            if let Ok(value) = task.evaluate(LOGIN_STATE_JS, PROBE_TIMEOUT).await {
                if let Ok(probe) = serde_json::from_value::<LoginProbe>(value) {
                    page_ok = page_ok || probe.page_ok;
                    if probe.logged_in {
                        task.release();
                        self.close_owned_page(&session, &target).await;
                        return Ok(());
                    }
                }
            }
            if !page_ok && elapsed >= detect {
                return Err(MultizenError::Config(
                    "小号登录页未就绪，请确认环境网络正常后重试".into(),
                ));
            }
            if let Ok(value) = task.evaluate(&security_check_script(), PROBE_TIMEOUT).await {
                if let Ok(probe) = serde_json::from_value::<SecurityProbe>(value) {
                    let matched: Vec<&str> =
                        probe.matched.iter().map(String::as_str).collect();
                    if let Err(failed) = check_security_verification(&probe.text, &matched) {
                        let since = match verify_since {
                            Some(seen) => seen,
                            None => {
                                let now = Instant::now();
                                verify_since = Some(now);
                                now
                            }
                        };
                        if since.elapsed() >= verify_grace {
                            return Err(verification_error(failed.hits));
                        }
                    } else {
                        verify_since = None;
                    }
                }
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// Single-account login. Never throws for auth outcomes: resolution
    /// failures (unknown id, unbound, no session) and auth failures
    /// (timeout, verification) are both encoded in the returned status so
    /// `batch_login` can collect per-account results in input order.
    pub async fn login_account(&self, account_id: &str) -> SubAccountLoginResult {
        let profile_id = match self.require_sub_profile(account_id).await {
            Ok((_, profile_id)) => profile_id,
            Err(e) => return SubAccountLoginResult::fail(account_id, e.to_string()),
        };
        match self
            .ensure_auth(&profile_id, &KUAISHOU_SUB_ACCOUNT_CONFIG)
            .await
        {
            Ok(()) => SubAccountLoginResult::ok(account_id),
            Err(e) => SubAccountLoginResult::fail(account_id, e.to_string()),
        }
    }

    /// Batch login with a concurrency cap of 3 (anti-risk-control).
    /// Results keep input order; a panic in one campaign becomes that
    /// account's failure instead of aborting the batch.
    pub async fn batch_login(
        self: &Arc<Self>,
        account_ids: Vec<String>,
    ) -> Vec<SubAccountLoginResult> {
        let mut handles = Vec::with_capacity(account_ids.len());
        for account_id in account_ids {
            let driver = self.clone();
            let permits = self.sub_account.login_permits.clone();
            let id = account_id.clone();
            handles.push((
                id.clone(),
                tokio::spawn(async move {
                    let _permit = permits.acquire_owned().await.map_err(|_| {
                        MultizenError::Mcp("小号登录并发槽不可用".to_string())
                    })?;
                    Ok::<_, MultizenError>(driver.login_account(&id).await)
                }),
            ));
        }
        let mut out = Vec::with_capacity(handles.len());
        for (account_id, handle) in handles {
            match handle.await {
                Ok(Ok(result)) => out.push(result),
                Ok(Err(e)) => out.push(SubAccountLoginResult::fail(&account_id, e.to_string())),
                Err(e) => out.push(SubAccountLoginResult::fail(
                    &account_id,
                    format!("小号登录任务异常：{e}"),
                )),
            }
        }
        out
    }

    // --- Live room + danmaku ---------------------------------------------------------

    async fn close_owned_page(&self, session: &Arc<BrowserSession>, target: &str) {
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            session.close_page(target),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(BROWSER_CLOSE_GRACE_MS)).await;
    }

    async fn wait_live_room_ready(
        &self,
        task: &mut TaskPage<'_>,
    ) -> std::result::Result<(), RoomBlock> {
        let deadline = Instant::now() + Duration::from_millis(LIVE_ROOM_READY_TIMEOUT_MS);
        loop {
            if Instant::now() >= deadline {
                return Err(RoomBlock::Failed("直播间就绪等待超时".into()));
            }
            if let Ok(value) = task.evaluate(&security_check_script(), PROBE_TIMEOUT).await {
                if let Ok(probe) = serde_json::from_value::<SecurityProbe>(value) {
                    let matched: Vec<&str> =
                        probe.matched.iter().map(String::as_str).collect();
                    if let Err(failed) = check_security_verification(&probe.text, &matched) {
                        return Err(RoomBlock::Verification(failed.hits));
                    }
                    if is_rate_limited(&probe.text) {
                        return Err(RoomBlock::RateLimited);
                    }
                    if let Ok(value) = task.evaluate(LIVE_ROOM_READY_JS, PROBE_TIMEOUT).await {
                        if let Ok(room) = serde_json::from_value::<RoomProbe>(value) {
                            if room.playing || room.live_text {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Navigate to a live room, wait for readiness, settle 3–8s randomly,
    /// retrying rate-limited attempts up to `RATE_LIMIT_RETRIES` times. The
    /// owned room page is kept per account for later `send_danmaku` calls.
    /// `send_danmaku` shares this per-account serial lock, so enter and send
    /// for one account never interleave.
    pub async fn enter_live_room(&self, account_id: &str, live_url: &str) -> Result<()> {
        let live_url = live_url.trim();
        if live_url.is_empty() {
            return Err(MultizenError::Config("直播间地址不能为空".into()));
        }
        let (_, profile_id) = self.require_sub_profile(account_id).await?;
        let session = self.require_session(&profile_id).await?;
        let _serial = self.sub_account.send_lock(account_id).lock_owned().await;
        let mut last_error = String::from("进入直播间失败");
        for attempt in 0..RATE_LIMIT_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(2 * u64::from(attempt))).await;
            }
            let target = match session.new_bound_page(live_url).await {
                Ok(page) => page.target_id().to_string(),
                Err(e) => {
                    last_error = format!("直播间页面创建失败：{e}");
                    continue;
                }
            };
            let mut task = match session
                .task_page(&target, self.sub_account.stop.clone(), TASK_ACQUIRE_TIMEOUT)
                .await
            {
                Ok(task) => task,
                Err(e) => {
                    last_error = format!("直播间页面接管失败：{}", task_error(e));
                    self.close_owned_page(&session, &target).await;
                    continue;
                }
            };
            match self.wait_live_room_ready(&mut task).await {
                Ok(()) => {
                    task.release();
                    tokio::time::sleep(Duration::from_millis(random_settle_delay_ms())).await;
                    if let Some(old) =
                        self.sub_account.set_owned_page(account_id, OwnedPage {
                            profile_id: profile_id.clone(),
                            target: target.clone(),
                            live_url: live_url.into(),
                        })
                    {
                        if old.target != target {
                            self.close_owned_page(&session, &old.target).await;
                        }
                    }
                    return Ok(());
                }
                Err(RoomBlock::RateLimited) => {
                    task.release();
                    self.close_owned_page(&session, &target).await;
                    last_error = "直播间访问受限（疑似限流），稍后重试".into();
                }
                Err(RoomBlock::Verification(hits)) => {
                    task.release();
                    return Err(verification_error(hits));
                }
                Err(RoomBlock::Failed(message)) => {
                    task.release();
                    self.close_owned_page(&session, &target).await;
                    last_error = message;
                }
            }
        }
        Err(MultizenError::Config(format!(
            "进入直播间失败（已重试{RATE_LIMIT_RETRIES}次）：{last_error}"
        )))
    }

    /// Shared danmaku interface with a stable `(account_id, content)`
    /// signature. Flow: verification check → score-best input → compatible
    /// fill (input/textarea/contenteditable) with readback → click the
    /// score-best send button, Enter fallback when absent → cleared check
    /// with one Enter resend. Every attempt is recorded.
    pub async fn send_danmaku(
        &self,
        account_id: &str,
        content: &str,
    ) -> Result<SubAccountInteraction> {
        let content = content.trim();
        if content.is_empty() {
            return Err(MultizenError::Config("弹幕内容不能为空".into()));
        }
        if content.chars().count() > MAX_DANMAKU_CHARS {
            return Err(MultizenError::Config(format!(
                "弹幕内容过长（最多{MAX_DANMAKU_CHARS}个字符）"
            )));
        }
        let owned = self.sub_account.owned_page(account_id).ok_or_else(|| {
            MultizenError::Config(format!("小号 {account_id} 尚未进入直播间，请先进入直播间"))
        })?;
        let session = self.require_session(&owned.profile_id).await?;
        let _serial = self.sub_account.send_lock(account_id).lock_owned().await;
        let fail = |error: String| RecordInteractionInput {
            account_id: account_id.into(),
            scene_id: None,
            action: SceneLineAction::Danmaku,
            message: Some(content.into()),
            live_room_url: Some(owned.live_url.clone()),
            ok: false,
            error: Some(error),
            duration_ms: None,
        };
        let page = match session.bind_page(&owned.target).await {
            Ok(page) => page,
            Err(e) => {
                self.sub_account.remove_owned_page(account_id);
                let error = format!("直播间页面已失效，请重新进入：{e}");
                let _ = self.record_interaction(fail(error.clone())).await;
                return Err(MultizenError::Cdp(error));
            }
        };
        let mut task = match page
            .into_task(self.sub_account.stop.clone(), TASK_ACQUIRE_TIMEOUT)
            .await
        {
            Ok(task) => task,
            Err(e) => {
                let error = format!("弹幕任务接管失败：{}", task_error(e));
                let _ = self.record_interaction(fail(error.clone())).await;
                return Err(MultizenError::Cdp(error));
            }
        };
        // 1. Verification gate.
        if let Ok(value) = task.evaluate(&security_check_script(), PROBE_TIMEOUT).await {
            if let Ok(probe) = serde_json::from_value::<SecurityProbe>(value) {
                let matched: Vec<&str> = probe.matched.iter().map(String::as_str).collect();
                if let Err(failed) = check_security_verification(&probe.text, &matched) {
                    task.release();
                    let error = verification_error(failed.hits);
                    let message = error.to_string();
                    let _ = self
                        .record_interaction(RecordInteractionInput {
                            error: Some(message),
                            ..fail(String::new())
                        })
                        .await;
                    return Err(error);
                }
            }
        }
        // 2. Score-best input + compatible fill with readback.
        let inputs: Vec<InputCandidate> =
            match task.evaluate(COLLECT_INPUT_CANDIDATES_JS, PROBE_TIMEOUT).await {
                Ok(value) => serde_json::from_value(value).unwrap_or_default(),
                Err(e) => {
                    let error = format!("评论输入框采集失败：{}", task_error(e));
                    let _ = self.record_interaction(fail(error.clone())).await;
                    return Err(MultizenError::Cdp(error));
                }
            };
        let Some(input_key) = pick_input(&inputs) else {
            let error = "未找到可用的评论输入框".to_string();
            let _ = self.record_interaction(fail(error.clone())).await;
            return Err(MultizenError::Config(error));
        };
        let filled: FillReadback = match task
            .evaluate(&fill_comment_input_script(&input_key, content), PROBE_TIMEOUT)
            .await
        {
            Ok(value) => serde_json::from_value(value).unwrap_or_default(),
            Err(e) => {
                let error = format!("弹幕填写失败：{}", task_error(e));
                let _ = self.record_interaction(fail(error.clone())).await;
                return Err(MultizenError::Cdp(error));
            }
        };
        if !filled.ok || !filled.actual.contains(content) {
            let error = format!("弹幕填写后回读不一致（实际：{}）", filled.actual);
            let _ = self.record_interaction(fail(error.clone())).await;
            return Err(MultizenError::Cdp(error));
        }
        // 3. Click the score-best send button; Enter fallback when absent.
        let sends: Vec<SendCandidate> =
            match task.evaluate(COLLECT_SEND_CANDIDATES_JS, PROBE_TIMEOUT).await {
                Ok(value) => serde_json::from_value(value).unwrap_or_default(),
                Err(e) => {
                    let error = format!("发送按钮采集失败：{}", task_error(e));
                    let _ = self.record_interaction(fail(error.clone())).await;
                    return Err(MultizenError::Cdp(error));
                }
            };
        let send_selector = pick_send_button(&sends)
            .map(|key| format!("[data-mz-subidx=\"{key}\"]"));
        if let Some(selector) = send_selector {
            if let Err(e) = task.click(&selector, PROBE_TIMEOUT).await {
                let error = format!("发送按钮点击失败：{}", task_error(e));
                let _ = self.record_interaction(fail(error.clone())).await;
                return Err(MultizenError::Cdp(error));
            }
        } else if task
            .evaluate(&press_enter_script(&input_key), PROBE_TIMEOUT)
            .await
            .is_err()
        {
            let error = "无发送按钮且回车发送失败".to_string();
            let _ = self.record_interaction(fail(error.clone())).await;
            return Err(MultizenError::Cdp(error));
        }
        // 4. Cleared check with one Enter resend.
        let cleared = self.input_cleared(&mut task, &input_key).await;
        if !cleared
            && task
                .evaluate(&press_enter_script(&input_key), PROBE_TIMEOUT)
                .await
                .is_ok()
            && !self.input_cleared(&mut task, &input_key).await
        {
            let error = "弹幕发送后输入框未清空（已回车补发一次）".to_string();
            let _ = self.record_interaction(fail(error.clone())).await;
            return Err(MultizenError::Cdp(error));
        }
        task.release();
        self.record_interaction(RecordInteractionInput {
            account_id: account_id.into(),
            scene_id: None,
            action: SceneLineAction::Danmaku,
            message: Some(content.into()),
            live_room_url: Some(owned.live_url.clone()),
            ok: true,
            error: None,
            duration_ms: None,
        })
        .await
    }

    async fn input_cleared(&self, task: &mut TaskPage<'_>, input_key: &str) -> bool {
        tokio::time::sleep(Duration::from_millis(500)).await;
        match task.evaluate(&cleared_check_script(input_key), PROBE_TIMEOUT).await {
            Ok(value) => serde_json::from_value::<ClearedReadback>(value)
                .map(|r| r.cleared)
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    // --- Background login polling ----------------------------------------------------

    /// Poll every 3s over accounts bound to running sessions and probe their
    /// owned room pages for login state. Read-only: never navigates, clicks,
    /// or writes; failures are traced and skipped.
    pub fn start_sub_account_login_monitor(self: &Arc<Self>) {
        if self.sub_account.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(self);
        let stop = self.sub_account.stop.clone();
        // Called from the Tauri setup hook (no Tokio runtime context), so use
        // `tauri::async_runtime::spawn` like the other monitors — plain
        // `tokio::spawn` panics here with "no reactor running".
        tauri::async_runtime::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_millis(LOGIN_POLL_INTERVAL_MS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let Some(driver) = weak.upgrade() else {
                    break;
                };
                driver.poll_sub_account_logins().await;
            }
        });
    }

    pub fn stop_sub_account_login_monitor(&self) {
        self.sub_account.stop.cancel();
    }

    async fn poll_sub_account_logins(&self) {
        let owned: Vec<(String, OwnedPage)> = self
            .sub_account
            .pages
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (account_id, owned) in owned {
            let Some(session) = self.registry.get(&owned.profile_id).await else {
                continue;
            };
            let Ok(page) = session.bind_page(&owned.target).await else {
                continue;
            };
            let Ok(mut task) = page
                .into_task(TaskCancel::new(), TASK_ACQUIRE_TIMEOUT)
                .await
            else {
                continue;
            };
            let state = match task.evaluate(LOGIN_STATE_JS, PROBE_TIMEOUT).await {
                Ok(value) => serde_json::from_value::<LoginProbe>(value)
                    .map(|p| p.logged_in)
                    .unwrap_or(false),
                Err(_) => false,
            };
            task.release();
            tracing::debug!(account = %account_id, logged_in = state, "sub-account login poll");
        }
    }
}

#[derive(Debug)]
enum RoomBlock {
    RateLimited,
    Verification(Vec<String>),
    Failed(String),
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct FillReadback {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    actual: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ClearedReadback {
    #[serde(default)]
    cleared: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_candidate(
        key: &str,
        tag: &str,
        editable: bool,
        visible: bool,
        placeholder: &str,
    ) -> InputCandidate {
        InputCandidate {
            key: key.into(),
            tag: tag.into(),
            editable,
            visible,
            placeholder: placeholder.into(),
            label: String::new(),
        }
    }

    fn send_candidate(key: &str, tag: &str, visible: bool, text: &str, cls: &str) -> SendCandidate {
        SendCandidate {
            key: key.into(),
            tag: tag.into(),
            visible,
            text: text.into(),
            cls: cls.into(),
        }
    }

    #[test]
    fn budgets_match_spec() {
        assert_eq!(LOGIN_POLL_INTERVAL_MS, 3_000);
        assert_eq!(KS_SUB_LOGIN_DETECT_MS, 30_000);
        assert_eq!(KS_VERIFY_MS, 20_000);
        assert_eq!(KS_LOGIN_MS, 180_000);
        assert_eq!(KS_LOGIN_MS / 60_000, 3);
        assert_eq!(BROWSER_CLOSE_GRACE_MS, 150);
        assert_eq!(MAX_CONCURRENT_LOGINS, 3);
        assert_eq!(RATE_LIMIT_RETRIES, 5);
        assert_eq!(LIVE_ROOM_SETTLE_MIN_MS, 3_000);
        assert_eq!(LIVE_ROOM_SETTLE_MAX_MS, 8_000);
        assert_eq!(KUAISHOU_SUB_ACCOUNT_CONFIG.site_url, "https://www.kuaishou.com");
        assert!(KUAISHOU_SUB_ACCOUNT_CONFIG
            .login_url
            .starts_with("https://www.kuaishou.com"));
    }

    #[test]
    fn security_lists_are_complete_and_each_fires() {
        assert_eq!(SECURITY_VERIFICATION_KEYWORDS.len(), 9);
        assert_eq!(SECURITY_VERIFICATION_SELECTORS.len(), 8);
        for keyword in SECURITY_VERIFICATION_KEYWORDS {
            let text = format!("页面出现{keyword}提示");
            let error = check_security_verification(&text, &[]).unwrap_err();
            assert_eq!(error.hits, vec![keyword.to_string()]);
        }
        for selector in SECURITY_VERIFICATION_SELECTORS {
            let error = check_security_verification("普通直播间", &[selector]).unwrap_err();
            assert_eq!(error.hits, vec![selector.to_string()]);
        }
        assert!(check_security_verification("普通直播间弹幕", &[]).is_ok());
        // Unknown selectors are ignored, not treated as challenges.
        assert!(check_security_verification("普通页面", &[".user-avatar"]).is_ok());
        let combined = check_security_verification(
            "请完成滑块验证后继续",
            &[".geetest_holder", ".user-avatar"],
        )
        .unwrap_err();
        assert_eq!(combined.hits.len(), 2);
    }

    #[test]
    fn verification_error_roundtrips_through_prefix() {
        let typed = SubAccountVerificationRequiredError {
            hits: vec!["滑块验证".into()],
        };
        assert!(typed.to_string().contains("滑块验证"));
        let error = verification_error(vec!["滑块验证".into(), ".captcha-container".into()]);
        assert!(is_verification_required_error(&error));
        assert!(error.to_string().contains("滑块验证"));
        assert!(!is_verification_required_error(&MultizenError::Mcp(
            "launcher thread closed".into()
        )));
        assert!(!is_verification_required_error(&MultizenError::Config(
            "弹幕内容不能为空".into()
        )));
    }

    #[test]
    fn rate_limit_keywords_each_fire() {
        for keyword in RATE_LIMIT_KEYWORDS {
            assert!(is_rate_limited(&format!("系统提示{keyword}")));
        }
        assert!(!is_rate_limited("直播进行中，主播正在发言"));
    }

    #[test]
    fn settle_delay_stays_in_window() {
        assert_eq!(live_room_settle_delay_ms(0.0), 3_000);
        assert_eq!(live_room_settle_delay_ms(0.5), 5_500);
        assert_eq!(live_room_settle_delay_ms(1.0), 8_000);
        assert_eq!(live_room_settle_delay_ms(-1.0), 3_000);
        assert_eq!(live_room_settle_delay_ms(2.0), 8_000);
        for _ in 0..100 {
            let delay = random_settle_delay_ms();
            assert!((LIVE_ROOM_SETTLE_MIN_MS..=LIVE_ROOM_SETTLE_MAX_MS).contains(&delay));
        }
    }

    #[test]
    fn pick_input_scores_visibility_tag_and_hints() {
        // Hidden hinted input loses to a visible plain one.
        let candidates = vec![
            input_candidate("a", "input", false, false, "说点什么"),
            input_candidate("b", "input", false, true, ""),
        ];
        assert_eq!(pick_input(&candidates).as_deref(), Some("b"));
        // Hinted placeholder wins among visible inputs.
        let candidates = vec![
            input_candidate("a", "input", false, true, ""),
            input_candidate("b", "textarea", false, true, "发条弹幕吧"),
        ];
        assert_eq!(pick_input(&candidates).as_deref(), Some("b"));
        // contenteditable outranks plain inputs without hints.
        let candidates = vec![
            input_candidate("a", "input", false, true, ""),
            input_candidate("b", "div", true, true, ""),
        ];
        assert_eq!(pick_input(&candidates).as_deref(), Some("b"));
        // Unknown non-editable tags and hidden elements are unusable.
        let candidates = vec![
            input_candidate("a", "span", false, true, "评论"),
            input_candidate("b", "input", false, false, "说点什么"),
        ];
        assert!(pick_input(&candidates).is_none());
        assert!(pick_input(&[]).is_none());
    }

    #[test]
    fn pick_send_button_prefers_exact_text_then_falls_back() {
        let candidates = vec![
            send_candidate("a", "button", true, "更多", ""),
            send_candidate("b", "button", true, "发送", ""),
        ];
        assert_eq!(pick_send_button(&candidates).as_deref(), Some("b"));
        // Partial text still scores; hidden exact text does not.
        let candidates = vec![
            send_candidate("a", "button", false, "发送", ""),
            send_candidate("b", "div", true, "发表评论", ""),
        ];
        assert_eq!(pick_send_button(&candidates).as_deref(), Some("b"));
        // Icon-only button with a send-like class stays clickable.
        let candidates = vec![send_candidate("a", "button", true, "", "send-btn icon")];
        assert_eq!(pick_send_button(&candidates).as_deref(), Some("a"));
        // Fully anonymous element means Enter fallback.
        let candidates = vec![send_candidate("a", "span", true, "", "icon")];
        assert!(pick_send_button(&candidates).is_none());
    }

    #[test]
    fn scripts_embed_all_selectors_and_escape_safely() {
        for script in [security_check_script()] {
            for selector in SECURITY_VERIFICATION_SELECTORS {
                assert!(script.contains(selector), "{selector}");
            }
        }
        assert!(LOGIN_STATE_JS.contains("loggedIn"));
        assert!(LIVE_ROOM_READY_JS.contains("playing"));
        assert!(COLLECT_INPUT_CANDIDATES_JS.contains("data-mz-subidx"));
        assert!(COLLECT_SEND_CANDIDATES_JS.contains("data-mz-subidx"));
        let fill = fill_comment_input_script("mzi0", "你好\"弹幕\"\n换行");
        assert!(fill.contains("mzi0"));
        // Quotes/newlines are JSON-escaped, never raw in the script.
        assert!(!fill.contains("你好\"弹幕\""));
        assert!(cleared_check_script("mzi1").contains("mzi1"));
        assert!(press_enter_script("mzi2").contains("KeyboardEvent"));
    }

    #[test]
    fn runtime_locks_and_pages_are_per_account() {
        let runtime = SubAccountRuntime::new();
        assert!(Arc::ptr_eq(
            &runtime.send_lock("a"),
            &runtime.send_lock("a")
        ));
        assert!(!Arc::ptr_eq(
            &runtime.send_lock("a"),
            &runtime.send_lock("b")
        ));
        assert!(runtime.owned_page("a").is_none());
        let replaced = runtime.set_owned_page(
            "a",
            OwnedPage {
                profile_id: "p".into(),
                target: "t1".into(),
                live_url: "https://live.kuaishou.com/u/1".into(),
            },
        );
        assert!(replaced.is_none());
        assert_eq!(runtime.owned_page("a").unwrap().target, "t1");
        let replaced = runtime.set_owned_page(
            "a",
            OwnedPage {
                profile_id: "p".into(),
                target: "t2".into(),
                live_url: "https://live.kuaishou.com/u/1".into(),
            },
        );
        assert_eq!(replaced.unwrap().target, "t1");
        runtime.remove_owned_page("a");
        assert!(runtime.owned_page("a").is_none());
    }

    #[tokio::test]
    async fn crud_forces_sub_kind_and_filters_list() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let profile = driver
            .create_profile(multizen_core::CreateProfileInput {
                name: "sub crud".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(driver.list_sub_accounts().await.unwrap().is_empty());
        // A shop-kind request is corrected to kuaishou_sub, never stored as shop.
        let other = driver
            .create_profile(multizen_core::CreateProfileInput {
                name: "other".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        driver
            .business_accounts_save(multizen_core::SaveBusinessAccountInput {
                id: None,
                profile_id: other.id.clone(),
                kind: BusinessAccountKind::KuaishouShop,
                display_name: "Shop".into(),
                platform_user_id: None,
            })
            .await
            .unwrap();
        let sub = driver
            .save_sub_account(multizen_core::SaveBusinessAccountInput {
                id: None,
                profile_id: profile.id.clone(),
                kind: BusinessAccountKind::KuaishouShop,
                display_name: "小号".into(),
                platform_user_id: None,
            })
            .await
            .unwrap();
        assert_eq!(sub.kind, BusinessAccountKind::KuaishouSub);
        let listed = driver.list_sub_accounts().await.unwrap();
        assert_eq!(listed, vec![sub.clone()]);
        driver.unbind_sub_account(&sub.id).await.unwrap();
        assert!(driver.list_sub_accounts().await.unwrap()[0]
            .profile_id
            .is_none());
        driver.shutdown().await;
    }

    #[tokio::test]
    async fn interactions_roundtrip_through_launcher() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let recorded = driver
            .record_interaction(RecordInteractionInput {
                account_id: "a1".into(),
                scene_id: None,
                action: SceneLineAction::Danmaku,
                message: Some("好".into()),
                live_room_url: Some("https://live.kuaishou.com/u/1".into()),
                ok: true,
                error: None,
                duration_ms: None,
            })
            .await
            .unwrap();
        assert_eq!(driver.sub_account_interactions("a1").await.unwrap(), vec![recorded]);
        assert!(driver.sub_account_interactions("a2").await.unwrap().is_empty());
        driver.shutdown().await;
    }

    #[tokio::test]
    async fn login_reports_per_account_results_in_order() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let driver = Arc::new(driver);
        let missing = driver.login_account("no-such-account").await;
        assert!(!missing.ok);
        assert!(missing.error.unwrap().contains("不存在"));
        let batch = driver
            .batch_login(vec!["b".into(), "a".into()])
            .await;
        assert_eq!(
            batch.iter().map(|r| r.account_id.clone()).collect::<Vec<_>>(),
            vec!["b".to_string(), "a".to_string()]
        );
        assert!(batch.iter().all(|r| !r.ok));
        assert!(driver.batch_login(vec![]).await.is_empty());
        // Validation happens before any browser work.
        assert!(driver.enter_live_room("no-such-account", " ").await.is_err());
        assert!(driver
            .send_danmaku("no-such-account", "hi")
            .await
            .is_err());
        driver.shutdown().await;
    }

    #[tokio::test]
    async fn login_monitor_starts_once_and_stops() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let driver = Arc::new(driver);
        driver.start_sub_account_login_monitor();
        driver.start_sub_account_login_monitor();
        driver.stop_sub_account_login_monitor();
        driver.shutdown().await;
    }
}
