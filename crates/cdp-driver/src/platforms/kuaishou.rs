//! Kuaishou platform connection primitives (connect / login / ensure_auth).
//!
//! Rust+chromiumoxide port of the jieger Kuaishou platform layer:
//!
//! - Config shapes/URLs/selectors: `F:/jieger/electron/main/utils/platformConfig.ts`
//!   (`KUAISHOU_CONFIG`, `KUAISHOU_SUB_ACCOUNT_CONFIG`, `KUAISHOU_JINNIU_CONFIG`,
//!   `buildStartupUrl`, `extractJinniuAccountIdFromUrl`).
//! - Shop connect/login/ensure flow:
//!   `F:/jieger/electron/main/platforms/kuaishou/connection.ts`
//!   (`kuaishouConnect` / `kuaishouLogin` / `ensureKuaishouAuth`).
//! - Jinniu URL-verdict login helpers mirror
//!   `F:/jieger/electron/main/tasks/jinniu/index.ts`
//!   (`verifyLoggedIn` / `waitForLogin`); the sub-account home-URL helper
//!   mirrors `isSubAccountHomeUrl` in
//!   `F:/jieger/electron/main/tasks/subAccount/index.ts`.
//!
//! Layering rules:
//!
//! - Only [`BrowserSession`] binding (`bind_page` / `new_bound_page`) plus
//!   [`BoundPage::into_task`] acquisition and [`TaskPage`] operations are used.
//!   Nothing here touches the legacy active-page cache, activates a target,
//!   launches a browser, or switches headless mode.
//! - Headless switching stays above this layer: [`ensure_auth`] never
//!   relaunches; [`EnsureAuthResult::scanned`] tells the caller whether a user
//!   scan completed so it can decide about headless restore itself.
//! - Phase reporting is a plain [`PhaseCallback`]; this crate never depends on
//!   Tauri. The Tauri command layer maps [`AuthPhase`] to its
//!   `kuaishou-auth-phase` event.
//! - Selector strings are jieger placeholder skeletons, NOT calibrated against
//!   live DOM. Treat selector hits as transport-level observations only.
//!
//! [`BrowserSession`]: crate::session::BrowserSession
//! [`BoundPage::into_task`]: crate::BoundPage::into_task
//! [`TaskPage`]: crate::TaskPage

use std::sync::Arc;
use std::time::Duration;

use multizen_core::MultizenError;

use crate::session::BrowserSession;
use crate::{SelectorState, TaskCancel, TaskError, TaskPage, TaskResult};

/// Shop connect/verify budget, mirroring `KS_CONNECT_VERIFY_TIMEOUT_MS` (60s).
pub const KS_CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
/// Shop QR-scan window, mirroring `KS_LOGIN_WAIT_TIMEOUT_MS` (5 minutes).
pub const KS_LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Selector poll cadence for connect/login waits.
pub const KS_SELECTOR_POLL: Duration = Duration::from_millis(500);
/// URL poll cadence for [`wait_until_authenticated_url`].
pub const KS_LOGIN_POLL: Duration = Duration::from_secs(1);
/// `location.href` read used for URL verdicts (never parsed as DOM evidence).
const CURRENT_URL_EXPR: &str = "location.href";

// ---------------------------------------------------------------------------
// Configs (jieger `platformConfig.ts`)
// ---------------------------------------------------------------------------

/// URL/selector contract for the shop control surface.
///
/// `login_page_pattern` / `pattern` / `store_home_pattern` are plain
/// substrings mirroring the jieger regexes (`/login\.kwaixiaodian\.com\//`,
/// `/zs\.kwaixiaodian\.com/`, `/s\.kwaixiaodian\.com\/zone\/home/`). A new
/// regex dependency is deliberately avoided: matching is `str::contains`.
#[derive(Debug, Clone, Copy)]
pub struct KuaishouVerify {
    /// Substring of an authenticated control URL (`zs.kwaixiaodian.com`).
    pub pattern: &'static str,
    /// Substring of the login page (`login.kwaixiaodian.com/`).
    pub login_page_pattern: &'static str,
    /// Logged-in DOM marker, jieger `H.LOGGED_IN` (`[class^=nickname] [class^=name]`).
    pub logged_in_selector: &'static str,
    /// Control-panel marker, jieger `H.IN_LIVE_CONTROL` (`div[class^=live-panel]`).
    pub in_live_control_selector: &'static str,
    /// Substring of the shop home URL (`s.kwaixiaodian.com/zone/home`).
    pub store_home_pattern: &'static str,
}

/// Shop platform config, mirroring jieger `KUAISHOU_CONFIG`.
#[derive(Debug, Clone, Copy)]
pub struct KuaishouConfig {
    pub id: &'static str,
    pub name: &'static str,
    /// QR login page (redirects to the control page after a scan).
    pub login_url: &'static str,
    /// Control page: the connect entry point after login.
    pub live_control_url: &'static str,
    /// Alias of the control page kept for jieger field-name compatibility.
    pub login_redirect_url: &'static str,
    /// Shop home page (account-login entry that skips the live-control panel).
    pub store_home_url: &'static str,
    /// Login page redirecting at the shop home page instead of control.
    pub store_login_url: &'static str,
    pub verify: KuaishouVerify,
}

/// 小店中控 (`zs.kwaixiaodian.com`). Note the domain split from the user-side
/// main site: the shop system lives on `kwaixiaodian.com`, not `kuaishou.com`.
pub const KUAISHOU_CONFIG: KuaishouConfig = KuaishouConfig {
    id: "kuaishou",
    name: "快手小店",
    login_url: "https://login.kwaixiaodian.com/?biz=merchantLivePc&redirect_url=https%3A%2F%2Fzs.kwaixiaodian.com%2Fpage%2Fhelper",
    live_control_url: "https://zs.kwaixiaodian.com/page/helper",
    login_redirect_url: "https://zs.kwaixiaodian.com/page/helper",
    store_home_url: "https://s.kwaixiaodian.com/zone/home",
    store_login_url: "https://login.kwaixiaodian.com/?biz=merchantLivePc&redirect_url=https%3A%2F%2Fs.kwaixiaodian.com%2Fzone%2Fhome",
    verify: KuaishouVerify {
        pattern: "zs.kwaixiaodian.com",
        login_page_pattern: "login.kwaixiaodian.com/",
        logged_in_selector: "[class^=nickname] [class^=name]",
        in_live_control_selector: "div[class^=live-panel]",
        store_home_pattern: "s.kwaixiaodian.com/zone/home",
    },
};

/// Sub-account (viewer identity) platform config, mirroring jieger
/// `KUAISHOU_SUB_ACCOUNT_CONFIG` (SubAccountManager `kuaishou` entry).
#[derive(Debug, Clone, Copy)]
pub struct KuaishouSubAccountConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub login_url: &'static str,
    pub live_room_url_template: &'static str,
    pub comment_input_selector: &'static str,
    pub send_button_selector: &'static str,
    pub logged_in_selector: &'static str,
    pub login_button_selector: &'static str,
    pub send_method: &'static str,
}

/// 小号观众 (`www.kuaishou.com`).
pub const KUAISHOU_SUB_ACCOUNT_CONFIG: KuaishouSubAccountConfig =
    KuaishouSubAccountConfig {
        id: "kuaishou",
        name: "快手",
        login_url: "https://www.kuaishou.com/",
        live_room_url_template: "https://www.kuaishou.com/live/{roomId}",
        comment_input_selector: ".comment-input textarea, [placeholder*=\"说点什么\"]",
        send_button_selector: ".comment-submit, .send-btn",
        logged_in_selector:
            ".sidebar .user.item, [class*=\"sidebar\"] .user.item, .navbar .user.item",
        login_button_selector:
            "button[class*=\"login\"], [class*=\"login-button\"], [data-e2e=\"login-button\"]",
        send_method: "click",
    };

/// Expand the live-room template for a room id (raw substitution; room ids
/// are alphanumeric in practice).
pub fn sub_account_live_room_url(room_id: &str) -> String {
    KUAISHOU_SUB_ACCOUNT_CONFIG
        .live_room_url_template
        .replace("{roomId}", room_id)
}

/// Mirror of jieger `isSubAccountHomeUrl`: on the main site without any
/// login/passport/signin/auth marker.
pub fn is_sub_account_home_url(url: &str) -> bool {
    !(url.contains("login")
        || url.contains("passport")
        || url.contains("signin")
        || url.contains("auth"))
        && url.starts_with(KUAISHOU_SUB_ACCOUNT_CONFIG.login_url)
}

/// Jinniu login verdict: URL substrings that mean "still on a login page",
/// mirroring `/passport\.kuaishou\.com|niu\.e\.kuaishou\.com\/login/`.
#[derive(Debug, Clone, Copy)]
pub struct JinniuVerify {
    pub login_page_markers: &'static [&'static str],
    /// `None` in jieger: Jinniu login is a pure URL verdict, no DOM selector.
    pub logged_in_selector: Option<&'static str>,
}

/// Jinniu account-dialog selector skeleton, mirroring jieger
/// `KUAISHOU_JINNIU_CONFIG.selectors`. Heuristic alternates tried in order by
/// the caller; uncalibrated.
#[derive(Debug, Clone, Copy)]
pub struct JinniuSelectors {
    pub account_dialog: &'static [&'static str],
    pub master_item: &'static [&'static str],
    pub master_name: &'static [&'static str],
    pub master_id: &'static [&'static str],
    pub topbar_account_trigger: &'static [&'static str],
}

/// Jinniu platform config, mirroring jieger `KUAISHOU_JINNIU_CONFIG`.
#[derive(Debug, Clone, Copy)]
pub struct KuaishouJinniuConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub login_url: &'static str,
    pub home_url: &'static str,
    pub verify: JinniuVerify,
    pub selectors: JinniuSelectors,
}

/// 磁力金牛 (`niu.e.kuaishou.com/home?homeType=new`).
///
/// Version pin: every entry URL carries `homeType=new` (the old layout).
/// Kuaishou naming is inverted — `homeType=new` is the OLD version,
/// `homeType=super` the new one whose top-bar popover breaks balance/account
/// reads.
pub const KUAISHOU_JINNIU_CONFIG: KuaishouJinniuConfig = KuaishouJinniuConfig {
    id: "kuaishou-jinniu",
    name: "磁力金牛",
    login_url: "https://niu.e.kuaishou.com/home?homeType=new",
    home_url: "https://niu.e.kuaishou.com/home?homeType=new",
    verify: JinniuVerify {
        login_page_markers: &["passport.kuaishou.com", "niu.e.kuaishou.com/login"],
        logged_in_selector: None,
    },
    selectors: JinniuSelectors {
        account_dialog: &[
            ".ant-modal[class*=\"account\"]",
            ".account-select-modal",
            "[role=\"dialog\"]:has(input[placeholder*=\"搜索\"])",
            "[class*=\"AccountDialog\"]",
            ".ant-modal-wrap:not([style*=\"display: none\"]) .ant-modal",
        ],
        master_item: &[
            "[class*=\"master\"]",
            "[class*=\"main-account\"]",
            "[class*=\"dialog\"] [class*=\"account-item\"]:first-child",
            ".ant-modal-body [class*=\"item\"]:first-child",
        ],
        master_name: &[
            "[class*=\"nickname\"]",
            "[class*=\"name\"]",
            "[class*=\"title\"]",
        ],
        master_id: &["[class*=\"account-id\"]", "[class*=\"uid\"]", "[class*=\"id\"]"],
        topbar_account_trigger: &[
            "header img[class*=\"avatar\"]",
            "header [class*=\"avatar\"]",
            "[class*=\"header\"] [class*=\"avatar\"]",
            "header [class*=\"user-info\"]",
            "header [class*=\"userInfo\"]",
            "[class*=\"header\"] [class*=\"user\"]",
            "[class*=\"topbar\"] [class*=\"account\"]",
            "[class*=\"header-right\"]",
        ],
    },
};

/// Build the Jinniu startup URL, mirroring jieger `buildStartupUrl`:
/// optional Jinniu sub-account id is appended as `__accountId__`, and
/// `homeType=new` (old layout) is always forced.
pub fn build_startup_url(target_account_id: Option<&str>) -> String {
    match target_account_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => format!(
            "https://niu.e.kuaishou.com/home?__accountId__={}&homeType=new",
            percent_encode_query(id)
        ),
        None => KUAISHOU_JINNIU_CONFIG.login_url.to_owned(),
    }
}

/// Extract the Jinniu sub-account id (`__accountId__` query param),
/// mirroring jieger `extractJinniuAccountIdFromUrl`. Missing/empty/invalid
/// URLs yield `None`.
pub fn extract_jinniu_account_id_from_url(url: &str) -> Option<String> {
    if url.trim().is_empty() {
        return None;
    }
    let query = url.split('#').next()?.split_once('?')?.1;
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => continue,
        };
        if key == "__accountId__" {
            let decoded = percent_decode_query(value);
            let trimmed = decoded.trim();
            return if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            };
        }
    }
    None
}

/// `true` when the URL is still on a shop login page.
pub fn is_kuaishou_login_page(url: &str) -> bool {
    url.contains(KUAISHOU_CONFIG.verify.login_page_pattern)
}

/// `true` when the URL is still on a Jinniu login page.
pub fn is_jinniu_login_page(url: &str) -> bool {
    KUAISHOU_JINNIU_CONFIG
        .verify
        .login_page_markers
        .iter()
        .any(|marker| url.contains(marker))
}

/// DOM selectors that locate a login QR image, most specific first.
///
/// Shop page (`login.kwaixiaodian.com`): a single `<img alt="qrcode"
/// class="qrcode-img">` whose `src` is already an inline `data:image/png;base64,…`
/// PNG. The class carries a Svelte scope hash, so only the stable `alt` and the
/// `qrcode` class prefix are matched.
///
/// Main-site passport page (`passport.kuaishou.com/pc/account/login`): once the
/// QR tab is active the image lives in `.qrcode > img` (also an inline
/// `data:image/png;base64,…` PNG, ~180px), covered by `.qrcode img`.
/// No broad base64-`img` fallback is used: an unrelated avatar rendered as a
/// data URL must never be mistaken for a login QR.
const QR_IMG_SELECTORS: &[&str] = &[
    "img[alt=\"qrcode\"]",
    "img.qrcode-img",
    "img[class*=\"qrcode\"]",
    ".qrcode-img img",
    ".qrcode img",
    "[class*=\"qrcode\"] img",
];

/// The page marks an expired QR by swapping the status overlay to this class
/// (shop: "二维码已过期"; main site: "二维码失效") while leaving the stale `<img>`
/// in place — so reading `src` alone would keep returning the dead QR. Both pages
/// expose a refresh control *inside* this overlay, but with different selectors:
/// the shop page uses `.scan-button`, the main site a `p.qrcode-desc-text`
/// ("点击刷新"). Both must be clicked, else the main-site QR never refreshes.
const QR_EXPIRED_SELECTOR: &str = ".qrcode-status-timeout";
const QR_REFRESH_SELECTORS: &[&str] = &[".scan-button", ".qrcode-desc-text"];

/// The main-site homepage (`www.kuaishou.com/new-reco`) only reveals the login QR
/// after the user clicks **立即登录**; the passport login page defaults to the
/// password tab and must be switched to the 扫码 tab. Both are the same shape:
/// click a trigger, then read on the next poll. Neither class/text exists on the
/// shop login page, so the clicks are no-ops there.
const QR_SWITCH_SELECTOR: &str = ".platform-switch-tips";
/// Guard so only the 扫码 (QR) switch is ever clicked, never a password tab.
const QR_SWITCH_MARKER: &str = "扫码";
/// The main-site "sign in now" trigger whose click surfaces the QR.
const QR_LOGIN_TEXT: &str = "立即登录";

/// Build the single self-contained `Runtime.evaluate` expression that reads the
/// login QR: refresh it when the page flags it expired, otherwise return the
/// first inline PNG found. When no QR is rendered yet it clicks the reveal
/// trigger — the passport 扫码 tab ([`QR_SWITCH_SELECTOR`]) or the main-site
/// 立即登录 button ([`QR_LOGIN_TEXT`]) — so the next poll reads the freshly shown
/// image. Kept as a function so a unit test can pin the selector wiring without a
/// live browser.
fn qr_expression(selectors: &str) -> String {
    let refresh = serde_json::to_string(QR_REFRESH_SELECTORS).expect("static selector list");
    [
        "(function(){var expired=document.querySelector('",
        QR_EXPIRED_SELECTOR,
        "');if(expired){var rs=",
        refresh.as_str(),
        ";for(var k=0;k<rs.length;k++){var rb=expired.querySelector(rs[k]);if(rb&&rb.click){rb.click();break;}}return null;}var sels=",
        selectors,
        ";var img=null;for(var i=0;i<sels.length;i++){var el=document.querySelector(sels[i]);if(el){img=el;break;}}var src=img&&(img.currentSrc||img.src);if(src&&src.indexOf('data:image/png;base64,')===0)return src;var sw=document.querySelector('",
        QR_SWITCH_SELECTOR,
        "');if(sw&&sw.textContent&&sw.textContent.indexOf('",
        QR_SWITCH_MARKER,
        "')>=0){if(sw.click)sw.click();return null;}var els=document.querySelectorAll('a,button,[role=\"button\"],span,div,p');for(var j=0;j<els.length;j++){if((els[j].textContent||'').trim()==='",
        QR_LOGIN_TEXT,
        "'){if(els[j].click)els[j].click();break;}}return null;})()",
    ]
    .concat()
}

/// Read a login QR straight from the page DOM, refreshing it when stale.
///
/// The shop login page and the main-site passport login page both render the QR
/// as an `<img>` whose `src` is an inline `data:image/png;base64,…` PNG, so the
/// exact image can be lifted without a screenshot or a brittle region crop. When
/// the QR expires the page only swaps its status overlay to
/// [`QR_EXPIRED_SELECTOR`] and leaves the dead image in the DOM; this helper
/// clicks the page's own refresh control ([`QR_REFRESH_SELECTORS`]) to mint a new
/// one and returns `Ok(None)` for that poll. On the passport page the QR is hidden
/// behind the password tab, and on the main-site homepage it is behind 立即登录,
/// so [`QR_SWITCH_SELECTOR`] / [`QR_LOGIN_TEXT`] are clicked to reveal it (again
/// returning `Ok(None)` for that poll).
///
/// Returns the bare base64 payload (no `data:` prefix) so the wire shape matches
/// the previous screenshot contract. `Ok(None)` means "no valid QR right now"
/// (not rendered, expired-and-refreshing, password tab just switched, or the
/// target is not a login page) — callers keep polling, never treat it as an error.
pub async fn qr_image(
    session: &BrowserSession,
    target_id: &str,
    cancel: TaskCancel,
    timeout: Duration,
) -> TaskResult<Option<String>> {
    if cancel.is_cancelled() {
        return Err(TaskError::Cancelled);
    }
    let bound = session
        .bind_page(target_id)
        .await
        .map_err(TaskError::Driver)?;
    let mut page = bound.into_task(cancel, timeout).await?;
    let selectors = serde_json::to_string(QR_IMG_SELECTORS).map_err(|error| {
        TaskError::Driver(MultizenError::Cdp(format!("qr selectors: {error}")))
    })?;
    // One round-trip: refresh an expired QR, else return the first selector's
    // inline PNG data URL, else reveal the QR tab on the passport page.
    let expression = qr_expression(&selectors);
    let value = page.evaluate(&expression, timeout).await?;
    page.release();
    let Some(src) = value.as_str() else {
        return Ok(None);
    };
    let payload = src
        .strip_prefix("data:image/png;base64,")
        .filter(|payload| !payload.is_empty());
    Ok(payload.map(str::to_owned))
}

fn percent_encode_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode_query(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let raw = input.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        match raw[index] {
            b'%' if index + 2 < raw.len() => {
                let hex: Vec<u8> = raw[index + 1..index + 3].to_vec();
                match u8::from_str_radix(std::str::from_utf8(&hex).unwrap_or(""), 16) {
                    Ok(byte) => {
                        bytes.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        bytes.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                bytes.push(b' ');
                index += 1;
            }
            byte => {
                bytes.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(bytes).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Auth phases / options / results (jieger `connection.ts`)
// ---------------------------------------------------------------------------

/// Auth lifecycle phases, mirroring jieger `AuthPhase`.
///
/// This layer only emits [`AuthPhase::VerifyingSession`] and
/// [`AuthPhase::WaitingForLogin`]. `LaunchingBrowser` / `RestoringHeadless`
/// are emitted by the upper (browser-owning) layer: this crate never
/// launches, closes, or relaunches a browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthPhase {
    LaunchingBrowser,
    VerifyingSession,
    WaitingForLogin,
    RestoringHeadless,
}

impl AuthPhase {
    /// Snake-case wire name shared with the Tauri `kuaishou-auth-phase` event.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LaunchingBrowser => "launching_browser",
            Self::VerifyingSession => "verifying_session",
            Self::WaitingForLogin => "waiting_for_login",
            Self::RestoringHeadless => "restoring_headless",
        }
    }
}

/// Phase reporter decoupled from Tauri: the command layer wraps `app.emit`.
pub type PhaseCallback = Arc<dyn Fn(AuthPhase) + Send + Sync>;

/// Options for [`ensure_auth`].
#[derive(Clone, Default)]
pub struct EnsureAuthOptions {
    pub cancel: TaskCancel,
    pub verify_timeout: Duration,
    pub login_timeout: Duration,
    pub on_phase: Option<PhaseCallback>,
}

impl EnsureAuthOptions {
    pub fn new(cancel: TaskCancel) -> Self {
        Self {
            cancel,
            verify_timeout: KS_CONNECT_TIMEOUT,
            login_timeout: KS_LOGIN_TIMEOUT,
            on_phase: None,
        }
    }

    pub fn with_phase_callback(mut self, callback: PhaseCallback) -> Self {
        self.on_phase = Some(callback);
        self
    }
}

/// Outcome of [`ensure_auth`], mirroring jieger `EnsureAuthResult`.
///
/// `scanned` is the headless-switch signal for the upper layer: `false`
/// means cookie reuse succeeded (or nothing was obtained), `true` means a
/// user scan completed and the caller may persist state / restore headless
/// itself. This layer never performs that switch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureAuthResult {
    pub ok: bool,
    pub scanned: bool,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// Read the current URL of a leased page. Accepts a bare string
/// (`location.href` in production) or an object with a `url` field.
async fn current_url(page: &mut TaskPage<'_>, timeout: Duration) -> TaskResult<String> {
    let value = page.evaluate(CURRENT_URL_EXPR, timeout).await?;
    if let Some(url) = value.as_str() {
        return Ok(url.to_owned());
    }
    if let Some(url) = value.get("url").and_then(|url| url.as_str()) {
        return Ok(url.to_owned());
    }
    Err(TaskError::Driver(MultizenError::Cdp(
        "current url is not a string".into(),
    )))
}

/// Generic connect core shared by the shop and Jinniu entries.
///
/// Navigates `target_id` to `entry_url`, then races the two jieger legs:
/// a login-page URL means "needs login" (`false`); otherwise the logged-in
/// marker decides. `logged_in_selector = None` (Jinniu, whose jieger selector
/// is `null`) makes this a pure URL verdict. A wait timeout or dispatch
/// error falls back to a fresh-lease URL verdict, mirroring jieger's
/// `catch` → URL check. Cancellation/interruption propagate.
pub async fn connect_to(
    session: &BrowserSession,
    target_id: &str,
    entry_url: &str,
    is_login_page: fn(&str) -> bool,
    logged_in_selector: Option<&str>,
    cancel: TaskCancel,
    timeout: Duration,
) -> TaskResult<bool> {
    if cancel.is_cancelled() {
        return Err(TaskError::Cancelled);
    }
    let bound = session
        .bind_page(target_id)
        .await
        .map_err(TaskError::Driver)?;
    let mut page = bound.into_task(cancel.clone(), timeout).await?;
    let nav = page.navigate(entry_url, timeout).await?;
    if is_login_page(&nav.url) {
        page.release();
        return Ok(false);
    }
    let Some(selector) = logged_in_selector else {
        page.release();
        return Ok(true);
    };
    match page
        .wait_for_selector(selector, SelectorState::Visible, timeout, KS_SELECTOR_POLL)
        .await
    {
        Ok(()) => {
            let url = current_url(&mut page, timeout).await?;
            page.release();
            Ok(!is_login_page(&url))
        }
        Err(error @ (TaskError::Cancelled | TaskError::Interrupted)) => return Err(error),
        Err(_) => {
            drop(page);
            let bound = session
                .bind_page(target_id)
                .await
                .map_err(TaskError::Driver)?;
            let mut fresh = bound.into_task(cancel, timeout).await?;
            let url = current_url(&mut fresh, timeout).await?;
            fresh.release();
            Ok(!is_login_page(&url))
        }
    }
}

/// Shop connect: goto the control page and race-verify, mirroring jieger
/// `kuaishouConnect` (`goto liveControlUrl` + race login-URL vs
/// in-live-control selector + URL verdict). `true` = authenticated,
/// `false` = login required.
pub async fn connect(
    session: &BrowserSession,
    target_id: &str,
    cancel: TaskCancel,
    timeout: Duration,
) -> TaskResult<bool> {
    connect_to(
        session,
        target_id,
        KUAISHOU_CONFIG.live_control_url,
        is_kuaishou_login_page,
        Some(KUAISHOU_CONFIG.verify.in_live_control_selector),
        cancel,
        timeout,
    )
    .await
}

/// Jinniu connect: navigate to `startup_url` (see [`build_startup_url`]) and
/// apply the URL-only verdict, mirroring jieger `verifyLoggedIn`. `true` =
/// stored session reusable, `false` = QR login required.
pub async fn connect_jinniu(
    session: &BrowserSession,
    target_id: &str,
    startup_url: &str,
    cancel: TaskCancel,
    timeout: Duration,
) -> TaskResult<bool> {
    connect_to(
        session,
        target_id,
        startup_url,
        is_jinniu_login_page,
        KUAISHOU_JINNIU_CONFIG.verify.logged_in_selector,
        cancel,
        timeout,
    )
    .await
}

/// Shop login: goto the login page unless already there, then wait for the
/// logged-in marker (user QR completion), mirroring jieger `kuaishouLogin`.
pub async fn login(
    session: &BrowserSession,
    target_id: &str,
    cancel: TaskCancel,
    timeout: Duration,
) -> TaskResult<()> {
    login_to(
        session,
        target_id,
        KUAISHOU_CONFIG.login_url,
        KUAISHOU_CONFIG.verify.logged_in_selector,
        is_kuaishou_login_page,
        cancel,
        timeout,
    )
    .await
}

/// Generic login core: [`login`] uses the shop entry; other surfaces pass
/// their own login URL, marker, and login-page verdict.
pub async fn login_to(
    session: &BrowserSession,
    target_id: &str,
    login_url: &str,
    logged_in_selector: &str,
    is_login_page: fn(&str) -> bool,
    cancel: TaskCancel,
    timeout: Duration,
) -> TaskResult<()> {
    if cancel.is_cancelled() {
        return Err(TaskError::Cancelled);
    }
    let bound = session
        .bind_page(target_id)
        .await
        .map_err(TaskError::Driver)?;
    let mut page = bound.into_task(cancel, timeout).await?;
    match current_url(&mut page, timeout).await {
        Ok(url) if !is_login_page(&url) => {
            page.navigate(login_url, timeout).await?;
        }
        Ok(_) => {}
        Err(_) => {
            page.navigate(login_url, timeout).await?;
        }
    }
    page.wait_for_selector(
        logged_in_selector,
        SelectorState::Visible,
        timeout,
        KS_SELECTOR_POLL,
    )
    .await?;
    page.release();
    Ok(())
}

/// Poll until the page URL leaves the login markers (QR completion without a
/// DOM selector), mirroring jieger Jinniu `waitForLogin` and the sub-account
/// `waitForLoggedIn` URL leg. Fail-closed on read errors; lease-terminal
/// states propagate.
pub async fn wait_until_authenticated_url(
    session: &BrowserSession,
    target_id: &str,
    is_login_page: fn(&str) -> bool,
    cancel: TaskCancel,
    timeout: Duration,
    poll_interval: Duration,
) -> TaskResult<()> {
    if poll_interval.is_zero() {
        return Err(TaskError::InvalidOptions(
            "poll_interval must be positive",
        ));
    }
    if cancel.is_cancelled() {
        return Err(TaskError::Cancelled);
    }
    let bound = session
        .bind_page(target_id)
        .await
        .map_err(TaskError::Driver)?;
    let mut page = bound.into_task(cancel.clone(), timeout).await?;
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .ok_or(TaskError::InvalidOptions("timeout exceeds clock range"))?;
    loop {
        if cancel.is_cancelled() {
            return Err(TaskError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(TaskError::TimedOut);
        }
        let slice = remaining.min(Duration::from_secs(10));
        let url = current_url(&mut page, slice).await?;
        if !is_login_page(&url) {
            page.release();
            return Ok(());
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(TaskError::Cancelled),
            _ = tokio::time::sleep(poll_interval.min(remaining)) => {}
        }
    }
}

/// Full shop flow, mirroring jieger `ensureKuaishouAuth` minus browser
/// ownership: verify via [`connect`]; on `false` report
/// [`AuthPhase::WaitingForLogin`] and run [`login`]. Cookie reuse returns
/// `scanned: false`; a completed scan returns `scanned: true` so the caller
/// can persist state and restore headless itself.
pub async fn ensure_auth(
    session: &BrowserSession,
    target_id: &str,
    options: EnsureAuthOptions,
) -> EnsureAuthResult {
    ensure_auth_to(
        session,
        target_id,
        KUAISHOU_CONFIG.live_control_url,
        is_kuaishou_login_page,
        Some(KUAISHOU_CONFIG.verify.in_live_control_selector),
        KUAISHOU_CONFIG.login_url,
        KUAISHOU_CONFIG.verify.logged_in_selector,
        options,
    )
    .await
}

/// Generic ensure core behind [`ensure_auth`]: `entry_url` is the connect
/// entry, `login_url`/`logged_in_selector` the scan wait. Exposed so tests
/// and future surfaces can drive the same verify-then-scan shape without a
/// second navigation per target (the offline peer completes only the first
/// navigation per target; real browsers mint fresh loader ids freely).
pub async fn ensure_auth_to(
    session: &BrowserSession,
    target_id: &str,
    entry_url: &str,
    is_login_page: fn(&str) -> bool,
    control_selector: Option<&str>,
    login_url: &str,
    logged_in_selector: &str,
    options: EnsureAuthOptions,
) -> EnsureAuthResult {
    let emit = |phase: AuthPhase| {
        if let Some(callback) = &options.on_phase {
            callback(phase);
        }
    };
    emit(AuthPhase::VerifyingSession);
    match connect_to(
        session,
        target_id,
        entry_url,
        is_login_page,
        control_selector,
        options.cancel.clone(),
        options.verify_timeout,
    )
    .await
    {
        Ok(true) => {
            return EnsureAuthResult {
                ok: true,
                scanned: false,
                error: None,
            };
        }
        Ok(false) => {}
        Err(error) => {
            return EnsureAuthResult {
                ok: false,
                scanned: false,
                error: Some(error.to_string()),
            };
        }
    }
    emit(AuthPhase::WaitingForLogin);
    match login_to(
        session,
        target_id,
        login_url,
        logged_in_selector,
        is_login_page,
        options.cancel.clone(),
        options.login_timeout,
    )
    .await
    {
        Ok(()) => EnsureAuthResult {
            ok: true,
            scanned: true,
            error: None,
        },
        Err(error) => EnsureAuthResult {
            ok: false,
            scanned: false,
            error: Some(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shop_config_urls_and_patterns_match_jieger() {
        assert_eq!(KUAISHOU_CONFIG.id, "kuaishou");
        assert!(KUAISHOU_CONFIG.login_url.starts_with("https://login.kwaixiaodian.com/"));
        assert!(KUAISHOU_CONFIG.live_control_url.starts_with("https://zs.kwaixiaodian.com/"));
        assert!(KUAISHOU_CONFIG.store_home_url.starts_with("https://s.kwaixiaodian.com/"));
        assert!(KUAISHOU_CONFIG
            .store_login_url
            .starts_with("https://login.kwaixiaodian.com/"));
        // Login page: still on login.
        assert!(is_kuaishou_login_page(KUAISHOU_CONFIG.login_url));
        assert!(is_kuaishou_login_page(KUAISHOU_CONFIG.store_login_url));
        // Control / store home: authenticated.
        assert!(!is_kuaishou_login_page(KUAISHOU_CONFIG.live_control_url));
        assert!(!is_kuaishou_login_page(KUAISHOU_CONFIG.store_home_url));
        assert!(KUAISHOU_CONFIG.store_home_url.contains(KUAISHOU_CONFIG.verify.store_home_pattern));
        assert!(KUAISHOU_CONFIG
            .live_control_url
            .contains(KUAISHOU_CONFIG.verify.pattern));
        // Placeholder selector skeleton is present but uncalibrated.
        assert!(!KUAISHOU_CONFIG.verify.logged_in_selector.is_empty());
        assert!(!KUAISHOU_CONFIG.verify.in_live_control_selector.is_empty());
    }

    #[test]
    fn sub_account_config_shape_and_room_url() {
        assert_eq!(KUAISHOU_SUB_ACCOUNT_CONFIG.id, "kuaishou");
        assert_eq!(
            KUAISHOU_SUB_ACCOUNT_CONFIG.login_url,
            "https://www.kuaishou.com/"
        );
        assert_eq!(
            sub_account_live_room_url("abc123"),
            "https://www.kuaishou.com/live/abc123"
        );
        assert!(!sub_account_live_room_url("abc123").contains("{roomId}"));
        assert!(is_sub_account_home_url("https://www.kuaishou.com/"));
        assert!(is_sub_account_home_url("https://www.kuaishou.com/live/abc123"));
        assert!(!is_sub_account_home_url(
            "https://www.kuaishou.com/login?redirect=/"
        ));
        assert!(!is_sub_account_home_url("https://passport.kuaishou.com/"));
        assert_eq!(KUAISHOU_SUB_ACCOUNT_CONFIG.send_method, "click");
    }

    #[test]
    fn jinniu_startup_url_always_pins_old_layout() {
        // No account: the bare login URL already carries homeType=new.
        let bare = build_startup_url(None);
        assert_eq!(bare, KUAISHOU_JINNIU_CONFIG.login_url);
        assert!(bare.contains("homeType=new"));
        // Blank ids fall back to the bare URL.
        for blank in [Some(""), Some("   "), None] {
            assert_eq!(build_startup_url(blank), bare);
        }
        // With an account: __accountId__ plus the forced old-layout flag.
        let with_account = build_startup_url(Some(" 12345 "));
        assert!(with_account.contains("__accountId__=12345"));
        assert!(with_account.contains("homeType=new"));
        assert!(!with_account.contains("homeType=super"));
        // Values are query-encoded.
        let encoded = build_startup_url(Some("a b&c"));
        assert!(encoded.contains("__accountId__=a%20b%26c"));
        assert!(encoded.contains("homeType=new"));
    }

    #[test]
    fn extract_jinniu_account_id_boundaries() {
        assert_eq!(extract_jinniu_account_id_from_url(""), None);
        assert_eq!(extract_jinniu_account_id_from_url("   "), None);
        assert_eq!(extract_jinniu_account_id_from_url("not a url"), None);
        assert_eq!(
            extract_jinniu_account_id_from_url("https://niu.e.kuaishou.com/home?homeType=new"),
            None
        );
        assert_eq!(
            extract_jinniu_account_id_from_url("https://niu.e.kuaishou.com/home?__accountId__="),
            None
        );
        assert_eq!(
            extract_jinniu_account_id_from_url("https://niu.e.kuaishou.com/home?__accountId__=   "),
            None
        );
        assert_eq!(
            extract_jinniu_account_id_from_url(
                "https://niu.e.kuaishou.com/home?__accountId__=12345&homeType=new"
            ),
            Some("12345".to_owned())
        );
        // Percent-decoding plus trimming, fragment ignored.
        assert_eq!(
            extract_jinniu_account_id_from_url(
                "https://niu.e.kuaishou.com/home?x=1&__accountId__=%31%32%33+&homeType=new#frag"
            ),
            Some("123".to_owned())
        );
        // Unrelated params never match.
        assert_eq!(
            extract_jinniu_account_id_from_url("https://niu.e.kuaishou.com/home?accountId=9"),
            None
        );
    }

    #[test]
    fn jinniu_login_page_markers() {
        assert!(is_jinniu_login_page("https://passport.kuaishou.com/login?x=1"));
        assert!(is_jinniu_login_page("https://niu.e.kuaishou.com/login"));
        assert!(!is_jinniu_login_page(KUAISHOU_JINNIU_CONFIG.home_url));
        assert!(!is_jinniu_login_page(
            "https://niu.e.kuaishou.com/home?__accountId__=1&homeType=new"
        ));
        assert_eq!(KUAISHOU_JINNIU_CONFIG.verify.logged_in_selector, None);
        assert!(!KUAISHOU_JINNIU_CONFIG.selectors.account_dialog.is_empty());
        assert!(!KUAISHOU_JINNIU_CONFIG.selectors.topbar_account_trigger.is_empty());
    }

    #[test]
    fn auth_phase_wire_names_and_result_shape() {
        assert_eq!(AuthPhase::VerifyingSession.as_str(), "verifying_session");
        assert_eq!(AuthPhase::WaitingForLogin.as_str(), "waiting_for_login");
        assert_eq!(AuthPhase::LaunchingBrowser.as_str(), "launching_browser");
        assert_eq!(AuthPhase::RestoringHeadless.as_str(), "restoring_headless");
        let ok = EnsureAuthResult {
            ok: true,
            scanned: false,
            error: None,
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            serde_json::json!({"ok": true, "scanned": false, "error": null})
        );
    }

    #[test]
    fn qr_expression_covers_both_login_pages_and_switches_to_qr_tab() {
        let expr = qr_expression(&serde_json::to_string(QR_IMG_SELECTORS).unwrap());
        // Shop login page: the inline `alt="qrcode"` / `qrcode-img` image is tried.
        assert!(expr.contains("qrcode-img"));
        // Main-site passport page: the QR sits under `.qrcode`, and the password
        // tab must be switched to the QR tab before it renders.
        assert!(expr.contains(".qrcode img"));
        assert!(expr.contains(QR_SWITCH_SELECTOR));
        assert!(expr.contains(QR_SWITCH_MARKER));
        // Main-site homepage: 立即登录 must be clicked to surface the QR.
        assert!(expr.contains(QR_LOGIN_TEXT));
        // An expired QR is refreshed through the page's own control — the shop
        // page's `.scan-button` and the main site's `.qrcode-desc-text`.
        assert!(expr.contains(QR_EXPIRED_SELECTOR));
        assert!(expr.contains(".scan-button"));
        assert!(expr.contains(".qrcode-desc-text"));
        // Self-contained single-evaluate IIFE.
        assert!(expr.starts_with("(function(){"));
        assert!(expr.ends_with("})()"));
    }
}
