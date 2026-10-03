//! 直播伴侣 (kuaishou live mate) HTTP QR login state machine.
//!
//! Rust port of the jieger Electron flow
//! `electron/main/tasks/mateLogin/index.ts` (4-step state machine:
//! `start` → `scanResult` long poll → `acceptResult` long poll → `receive`).
//! The port is HTTP-only: no browser page, no CDP, no cookie access.
//!
//! # Result code table (extracted from the jieger source, not guessed)
//!
//! | endpoint     | `result` | meaning (jieger behavior)                              |
//! |--------------|----------|----------------------------------------------------------|
//! | start        | `1`      | QR issued; requires `qrLoginToken` + `qrLoginSignature` + `imageData`, anything else is an error (`index.ts:273-280`) |
//! | scanResult   | `1`      | scanned; requires `user`, advances to `awaiting-confirm` (`index.ts:218,299-301`) |
//! | scanResult   | `707`    | QR expired → `expired` (`index.ts:219,291-298`)          |
//! | scanResult   | other    | keep polling after `POLL_RETRY_DELAY_MS` (`index.ts:220`) |
//! | acceptResult | `1`      | confirmed; requires `qrToken`, advances to `receiving` (`index.ts:254,325-327`) |
//! | acceptResult | `707`    | QR expired → `expired` (`index.ts:255,317-324`)          |
//! | acceptResult | other    | keep polling after `POLL_RETRY_DELAY_MS` (`index.ts:256`) |
//! | receive      | `1`      | success; requires `user`, anything else is an error (`index.ts:341-345`) |
//!
//! # Token policy
//!
//! Login tokens (`passToken`/`token`/`lmtoken`/`mate_st`/`mate_h5_st`) are
//! deliberately NOT persisted: every login re-scans a fresh QR code. The
//! success state carries only the confirmed user identity; binding that
//! identity to a `business_accounts` row stays an explicit manual
//! `business_accounts_save` call with kind `kuaishou-mate`.
//!
//! # Cancellation
//!
//! Each in-flight flow owns a `cdp_driver::TaskCancel`. Every network await
//! and every retry-delay sleep is wrapped in `tokio::select!` against
//! `TaskCancel::cancelled`, so `cancel` interrupts a 70 s long poll
//! immediately instead of polling for a flag.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex as StdMutex,
};

use cdp_driver::TaskCancel;
use multizen_core::{BusinessAccountKind, MultizenError, Result};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Frontend push event carrying the full [`MateLoginState`] snapshot.
pub const MATE_LOGIN_STATE_CHANGED: &str = "mate-login-state-changed";

/// Production endpoint set (jieger `index.ts:39-42`).
pub const QR_START_URL: &str = "https://qr.kuaishou.com/rest/q/user/login/qrcode/start";
pub const SCAN_RESULT_URL: &str = "https://id.kwaixiaodian.com/rest/c/infra/ks/qr/scanResult";
pub const ACCEPT_RESULT_URL: &str = "https://id.kwaixiaodian.com/rest/c/infra/ks/qr/acceptResult";
pub const RECEIVE_URL: &str = "https://qr.kuaishou.com/rest/q/user/login/qrcode/receive";

/// Fixed `sid` posted to `start` and `acceptResult` (jieger `index.ts:43`).
const SID_SHOP_B: &str = "kuaishou.shop.b";
/// Browser UA for `start`/`scanResult`/`acceptResult` (jieger `index.ts:44`).
pub const UA_BROWSER: &str = "Mozilla/4.0 (compatible; MSIE 9.0; Windows NT 6.1)";
/// App UA required by `receive` (jieger `index.ts:45`).
pub const UA_RECEIVE: &str = "kuaishou 5.105.2.3505";

/// Per-request long-poll timeout (jieger `index.ts:46`).
pub const POLL_TIMEOUT_MS: u64 = 70_000;
/// Delay between poll rounds / after transport errors (jieger `index.ts:47`).
pub const POLL_RETRY_DELAY_MS: u64 = 1_500;
/// Consecutive transport/timeout failures before the flow errors
/// (jieger `index.ts:48`).
pub const POLL_MAX_RETRIES: u32 = 5;
/// One-shot timeout for `start` and `receive` (jieger `index.ts:49`).
pub const ONE_SHOT_TIMEOUT_MS: u64 = 20_000;

/// Success code shared by all four endpoints (see module table).
pub const RESULT_OK: i64 = 1;
/// QR-expired code for the two long-poll endpoints (see module table).
pub const RESULT_EXPIRED: i64 = 707;

/// Login stages, wire-identical to jieger `MateLoginStage` (`index.ts:15-24`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MateLoginStage {
    Idle,
    Starting,
    AwaitingScan,
    AwaitingConfirm,
    Receiving,
    Success,
    Expired,
    Cancelled,
    Error,
}

/// Confirmed user identity. Tokens are intentionally absent (see module docs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MateLoginUser {
    pub user_id: String,
    pub user_name: String,
    pub avatar_url: Option<String>,
}

/// Full login snapshot pushed via [`MATE_LOGIN_STATE_CHANGED`].
/// Field-for-field equivalent of jieger `MateLoginState` (`index.ts:26-37`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MateLoginState {
    pub account_id: String,
    pub stage: MateLoginStage,
    pub qr_image_data_url: Option<String>,
    pub qr_login_token: Option<String>,
    pub qr_login_signature: Option<String>,
    pub expire_at: Option<i64>,
    pub error_message: Option<String>,
    pub user: Option<MateLoginUser>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

impl MateLoginState {
    fn idle(account_id: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            stage: MateLoginStage::Idle,
            qr_image_data_url: None,
            qr_login_token: None,
            qr_login_signature: None,
            expire_at: None,
            error_message: None,
            user: None,
            started_at: None,
            finished_at: None,
        }
    }
}

/// The four HTTP endpoints. Overridable for tests; production default matches
/// the jieger constants above.
#[derive(Debug, Clone)]
pub struct MateLoginEndpoints {
    pub start: String,
    pub scan_result: String,
    pub accept_result: String,
    pub receive: String,
}

impl Default for MateLoginEndpoints {
    fn default() -> Self {
        Self {
            start: QR_START_URL.to_string(),
            scan_result: SCAN_RESULT_URL.to_string(),
            accept_result: ACCEPT_RESULT_URL.to_string(),
            receive: RECEIVE_URL.to_string(),
        }
    }
}

// --- jieger response DTOs (field names follow the platform JSON) -------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrStartResp {
    #[serde(default)]
    result: i64,
    #[serde(default)]
    qr_login_token: Option<String>,
    #[serde(default)]
    qr_login_signature: Option<String>,
    #[serde(default)]
    expire_time: Option<i64>,
    #[serde(default)]
    image_data: Option<String>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QrUserJson {
    #[serde(rename = "user_id")]
    user_id: String,
    #[serde(rename = "user_name")]
    user_name: String,
    #[serde(default)]
    headurl: Option<String>,
    #[serde(default)]
    headurls: Option<Vec<String>>,
}

impl QrUserJson {
    fn into_user(self) -> MateLoginUser {
        let avatar_url = self
            .headurl
            .or_else(|| self.headurls.and_then(|urls| urls.into_iter().next()));
        MateLoginUser {
            user_id: self.user_id,
            user_name: self.user_name,
            avatar_url,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanResultResp {
    #[serde(default)]
    result: i64,
    #[serde(default)]
    user: Option<QrUserJson>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptResultResp {
    #[serde(default)]
    result: i64,
    #[serde(default)]
    qr_token: Option<String>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiveResp {
    #[serde(default)]
    result: i64,
    #[serde(default)]
    user: Option<QrUserJson>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
}

// --- HTTP plumbing -----------------------------------------------------------

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn form_body(pairs: &[(&str, &str)]) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    pairs
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                utf8_percent_encode(k, NON_ALPHANUMERIC),
                utf8_percent_encode(v, NON_ALPHANUMERIC)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn error_text(msg: Option<String>) -> String {
    msg.unwrap_or_default()
}

/// A single HTTP step failed. `Cancelled` wins over transport outcomes so a
/// concurrent `cancel` is never reported as a network error.
enum StepError {
    Cancelled,
    Transport(String),
}

/// POST `application/x-www-form-urlencoded` (jieger `postForm`, `index.ts:156-177`)
/// with per-request `timeout`; cancellation interrupts via `select!`.
async fn post_form<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    pairs: &[(&str, &str)],
    ua: &str,
    timeout: std::time::Duration,
    cancel: &TaskCancel,
) -> std::result::Result<T, StepError> {
    let work = async {
        let resp = client
            .post(url)
            .header(reqwest::header::USER_AGENT, ua)
            .header(reqwest::header::ACCEPT, "*/*")
            .header("Accept-Language", "zh-cn")
            .header(reqwest::header::REFERER, url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded; Charset=UTF-8",
            )
            .body(form_body(pairs))
            .send()
            .await
            .map_err(|e| format!("网络请求失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "HTTP {} {}",
                resp.status().as_u16(),
                resp.status().canonical_reason().unwrap_or_default()
            ));
        }
        resp.json::<T>()
            .await
            .map_err(|e| format!("解析响应失败：{e}"))
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(StepError::Cancelled),
        out = tokio::time::timeout(timeout, work) => match out {
            Err(_) if cancel.is_cancelled() => Err(StepError::Cancelled),
            Err(_) => Err(StepError::Transport("请求超时，请检查网络后重试".to_string())),
            Ok(result) => result.map_err(|e| {
                if cancel.is_cancelled() { StepError::Cancelled } else { StepError::Transport(e) }
            }),
        },
    }
}

/// POST JSON (jieger `postJson`, `index.ts:179-194`) with per-request
/// `timeout`; cancellation interrupts via `select!`.
async fn post_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
    ua: &str,
    timeout: std::time::Duration,
    cancel: &TaskCancel,
) -> std::result::Result<T, StepError> {
    let work = async {
        let resp = client
            .post(url)
            .header(reqwest::header::USER_AGENT, ua)
            .header(reqwest::header::ACCEPT, "*/*")
            .header("Accept-Language", "zh-cn")
            .header(reqwest::header::REFERER, url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/json; Charset=UTF-8",
            )
            .json(body)
            .send()
            .await
            .map_err(|e| format!("网络请求失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "HTTP {} {}",
                resp.status().as_u16(),
                resp.status().canonical_reason().unwrap_or_default()
            ));
        }
        resp.json::<T>()
            .await
            .map_err(|e| format!("解析响应失败：{e}"))
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(StepError::Cancelled),
        out = tokio::time::timeout(timeout, work) => match out {
            Err(_) if cancel.is_cancelled() => Err(StepError::Cancelled),
            Err(_) => Err(StepError::Transport("请求超时，请检查网络后重试".to_string())),
            Ok(result) => result.map_err(|e| {
                if cancel.is_cancelled() { StepError::Cancelled } else { StepError::Transport(e) }
            }),
        },
    }
}

/// Outcome of one long-poll phase. Non-terminal `result` values keep polling
/// (jieger `index.ts:220,256`); only `1`/`707` settle the phase.
enum PollOutcome<T> {
    Ready(T),
    Expired,
    Cancelled,
    Failed(String),
}

/// `scanResult` long poll (jieger `pollScanResult`, `index.ts:196-228`).
async fn poll_scan_result(
    client: &reqwest::Client,
    endpoints: &MateLoginEndpoints,
    qr_login_token: &str,
    qr_login_signature: &str,
    cancel: &TaskCancel,
) -> PollOutcome<MateLoginUser> {
    let mut consecutive_errors: u32 = 0;
    let mut last_platform_error: Option<String> = None;
    loop {
        if cancel.is_cancelled() {
            return PollOutcome::Cancelled;
        }
        let step: std::result::Result<ScanResultResp, StepError> = post_form(
            client,
            &endpoints.scan_result,
            &[
                ("qrLoginToken", qr_login_token),
                ("qrLoginSignature", qr_login_signature),
                ("channelType", "UNKNOWN"),
                ("encryptHeaders", ""),
            ],
            UA_BROWSER,
            std::time::Duration::from_millis(POLL_TIMEOUT_MS),
            cancel,
        )
        .await;
        match step {
            Ok(data) => {
                consecutive_errors = 0;
                if data.result == RESULT_OK && data.user.is_some() {
                    return PollOutcome::Ready(data.user.map(QrUserJson::into_user).unwrap());
                }
                if data.result == RESULT_EXPIRED {
                    return PollOutcome::Expired;
                }
                last_platform_error = data.error_msg;
            }
            Err(StepError::Transport(message)) if !cancel.is_cancelled() => {
                consecutive_errors += 1;
                if consecutive_errors >= POLL_MAX_RETRIES {
                    return PollOutcome::Failed(with_platform_error(
                        message,
                        last_platform_error.take(),
                    ));
                }
            }
            Err(_) => return PollOutcome::Cancelled,
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return PollOutcome::Cancelled,
            _ = tokio::time::sleep(std::time::Duration::from_millis(POLL_RETRY_DELAY_MS)) => {}
        }
    }
}

/// `acceptResult` long poll (jieger `pollAcceptResult`, `index.ts:230-263`).
async fn poll_accept_result(
    client: &reqwest::Client,
    endpoints: &MateLoginEndpoints,
    qr_login_token: &str,
    qr_login_signature: &str,
    cancel: &TaskCancel,
) -> PollOutcome<()> {
    let mut consecutive_errors: u32 = 0;
    let mut last_platform_error: Option<String> = None;
    loop {
        if cancel.is_cancelled() {
            return PollOutcome::Cancelled;
        }
        let step: std::result::Result<AcceptResultResp, StepError> = post_form(
            client,
            &endpoints.accept_result,
            &[
                ("qrLoginToken", qr_login_token),
                ("qrLoginSignature", qr_login_signature),
                ("sid", SID_SHOP_B),
                ("channelType", "UNKNOWN"),
                ("encryptHeaders", ""),
            ],
            UA_BROWSER,
            std::time::Duration::from_millis(POLL_TIMEOUT_MS),
            cancel,
        )
        .await;
        match step {
            Ok(data) => {
                consecutive_errors = 0;
                if data.result == RESULT_OK && data.qr_token.is_some() {
                    return PollOutcome::Ready(());
                }
                if data.result == RESULT_EXPIRED {
                    return PollOutcome::Expired;
                }
                last_platform_error = data.error_msg;
            }
            Err(StepError::Transport(message)) if !cancel.is_cancelled() => {
                consecutive_errors += 1;
                if consecutive_errors >= POLL_MAX_RETRIES {
                    return PollOutcome::Failed(with_platform_error(
                        message,
                        last_platform_error.take(),
                    ));
                }
            }
            Err(_) => return PollOutcome::Cancelled,
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return PollOutcome::Cancelled,
            _ = tokio::time::sleep(std::time::Duration::from_millis(POLL_RETRY_DELAY_MS)) => {}
        }
    }
}

/// Append the last platform-supplied error text (if any) to a transport
/// failure so diagnostics keep both causes.
fn with_platform_error(transport: String, platform: Option<String>) -> String {
    match platform.filter(|m| !m.trim().is_empty()) {
        Some(platform) => format!("{transport}（平台：{platform}）"),
        None => transport,
    }
}

/// Terminal outcome of one flow run.
enum FlowOutcome {
    Success(MateLoginUser),
    Expired,
    Cancelled,
    Failed(String),
}

/// Full 4-step flow (jieger `runFlow`, `index.ts:265-372`) minus token
/// persistence: tokens are dropped after `receive`, only the user is kept.
async fn flow_inner(
    client: &reqwest::Client,
    endpoints: &MateLoginEndpoints,
    rt: &MateLoginRuntime,
    account_id: &str,
    cancel: &TaskCancel,
) -> FlowOutcome {
    // Step 1: start (one-shot, jieger `index.ts:266-288`).
    let start: std::result::Result<QrStartResp, StepError> = post_form(
        client,
        &endpoints.start,
        &[("sid", SID_SHOP_B)],
        UA_BROWSER,
        std::time::Duration::from_millis(ONE_SHOT_TIMEOUT_MS),
        cancel,
    )
    .await;
    let start_resp = match start {
        Ok(resp) => resp,
        Err(StepError::Transport(message)) if !cancel.is_cancelled() => {
            return FlowOutcome::Failed(message)
        }
        Err(_) => return FlowOutcome::Cancelled,
    };
    let (qr_login_token, qr_login_signature, image_data) = match start_resp {
        QrStartResp {
            result,
            qr_login_token: Some(token),
            qr_login_signature: Some(signature),
            image_data: Some(image),
            ..
        } if result == RESULT_OK => (token, signature, image),
        other => {
            return FlowOutcome::Failed(format!(
                "获取二维码失败：result={} {}",
                other.result,
                error_text(other.error_msg)
            ));
        }
    };
    rt.patch(account_id, MateLoginStage::AwaitingScan, |s| {
        s.qr_image_data_url = Some(format!("data:image/png;base64,{image_data}"));
        s.qr_login_token = Some(qr_login_token.clone());
        s.qr_login_signature = Some(qr_login_signature.clone());
        s.expire_at = start_resp.expire_time;
    });

    // Step 2: scanResult long poll (jieger `index.ts:290-310`).
    match poll_scan_result(
        client,
        endpoints,
        &qr_login_token,
        &qr_login_signature,
        cancel,
    )
    .await
    {
        PollOutcome::Cancelled => return FlowOutcome::Cancelled,
        PollOutcome::Expired => {
            rt.finish_expired(account_id, "二维码已过期，请重新获取");
            return FlowOutcome::Expired;
        }
        PollOutcome::Failed(message) => {
            return FlowOutcome::Failed(format!("扫码状态异常：{message}"));
        }
        PollOutcome::Ready(user) => {
            rt.patch(account_id, MateLoginStage::AwaitingConfirm, |s| {
                s.user = Some(MateLoginUser {
                    user_id: user.user_id.clone(),
                    user_name: user.user_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                });
            });
        }
    }

    // Step 3: acceptResult long poll (jieger `index.ts:312-329`).
    match poll_accept_result(
        client,
        endpoints,
        &qr_login_token,
        &qr_login_signature,
        cancel,
    )
    .await
    {
        PollOutcome::Cancelled => return FlowOutcome::Cancelled,
        PollOutcome::Expired => {
            rt.finish_expired(account_id, "二维码已过期，请重新获取");
            return FlowOutcome::Expired;
        }
        PollOutcome::Failed(message) => {
            return FlowOutcome::Failed(format!("手机端确认失败：{message}"));
        }
        PollOutcome::Ready(()) => {
            rt.patch(account_id, MateLoginStage::Receiving, |_| {});
        }
    }

    // Step 4: receive, JSON body + app UA (jieger `index.ts:331-345`).
    let receive: std::result::Result<ReceiveResp, StepError> = post_json(
        client,
        &endpoints.receive,
        &serde_json::json!({
            "qrLoginSignature": qr_login_signature,
            "qrLoginToken": qr_login_token,
        }),
        UA_RECEIVE,
        std::time::Duration::from_millis(ONE_SHOT_TIMEOUT_MS),
        cancel,
    )
    .await;
    let receive_resp = match receive {
        Ok(resp) => resp,
        Err(StepError::Transport(message)) if !cancel.is_cancelled() => {
            return FlowOutcome::Failed(message)
        }
        Err(_) => return FlowOutcome::Cancelled,
    };
    match receive_resp {
        ReceiveResp {
            result,
            user: Some(user),
            ..
        } if result == RESULT_OK => FlowOutcome::Success(user.into_user()),
        other => FlowOutcome::Failed(format!(
            "领取登录凭证失败：result={} {}",
            other.result,
            error_text(other.error_msg)
        )),
    }
}

// --- runtime ---------------------------------------------------------------

/// In-memory login runtime: one [`TaskCancel`] + one [`MateLoginState`] per
/// account id. States are never written to disk (tokens live only here).
pub struct MateLoginRuntime {
    client: reqwest::Client,
    endpoints: MateLoginEndpoints,
    states: StdMutex<HashMap<String, MateLoginState>>,
    cancels: StdMutex<HashMap<String, (u64, TaskCancel)>>,
    generation: AtomicU64,
    app: StdMutex<Option<tauri::AppHandle>>,
}

impl MateLoginRuntime {
    pub fn new() -> Self {
        Self::with_endpoints(MateLoginEndpoints::default())
    }

    pub fn with_endpoints(endpoints: MateLoginEndpoints) -> Self {
        Self {
            client: reqwest::Client::builder()
                .build()
                .expect("mate login reqwest client"),
            endpoints,
            states: StdMutex::new(HashMap::new()),
            cancels: StdMutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            app: StdMutex::new(None),
        }
    }

    pub fn set_app(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }

    /// Abort every in-flight flow (app shutdown path).
    pub fn cancel_all(&self) {
        for (_, cancel) in self.cancels.lock().unwrap().values() {
            cancel.cancel();
        }
    }

    fn emit(&self, state: &MateLoginState) {
        let guard = self.app.lock().unwrap();
        if let Some(app) = guard.as_ref() {
            if let Err(e) = app.emit(MATE_LOGIN_STATE_CHANGED, state) {
                tracing::warn!(event = MATE_LOGIN_STATE_CHANGED, error = %e, "tauri emit failed");
            }
        }
    }

    /// Current snapshot; unknown ids report `idle` (jieger `getState`).
    pub fn get_state(&self, account_id: &str) -> MateLoginState {
        self.states
            .lock()
            .unwrap()
            .get(account_id)
            .cloned()
            .unwrap_or_else(|| MateLoginState::idle(account_id))
    }

    fn set_state(&self, state: MateLoginState) {
        self.states
            .lock()
            .unwrap()
            .insert(state.account_id.clone(), state.clone());
        self.emit(&state);
    }

    fn patch(
        &self,
        account_id: &str,
        stage: MateLoginStage,
        apply: impl FnOnce(&mut MateLoginState),
    ) {
        let mut state = self.get_state(account_id);
        state.stage = stage;
        apply(&mut state);
        self.set_state(state);
    }

    fn finish_expired(&self, account_id: &str, message: &str) {
        let mut state = self.get_state(account_id);
        state.stage = MateLoginStage::Expired;
        state.error_message = Some(message.to_string());
        state.finished_at = Some(now_ms());
        self.set_state(state);
    }

    fn current_generation(&self, account_id: &str) -> Option<u64> {
        self.cancels
            .lock()
            .unwrap()
            .get(account_id)
            .map(|(gen, _)| *gen)
    }

    /// Start (or restart) the flow for `account_id`, aborting any previous
    /// run first (jieger `start`, `index.ts:374-414`).
    pub fn begin(self: &Arc<Self>, account_id: &str) -> MateLoginState {
        if let Some((_, previous)) = self.cancels.lock().unwrap().get(account_id) {
            previous.cancel();
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let cancel = TaskCancel::new();
        self.cancels
            .lock()
            .unwrap()
            .insert(account_id.to_string(), (generation, cancel.clone()));
        let mut state = MateLoginState::idle(account_id);
        state.stage = MateLoginStage::Starting;
        state.started_at = Some(now_ms());
        self.set_state(state.clone());

        let rt = self.clone();
        let id = account_id.to_string();
        tokio::spawn(async move {
            let outcome = flow_inner(&rt.client, &rt.endpoints, &rt, &id, &cancel).await;
            // A superseded run must not overwrite the newer run's state.
            let current = rt.current_generation(&id) == Some(generation);
            match outcome {
                _ if !current => {}
                FlowOutcome::Success(user) => {
                    let mut state = rt.get_state(&id);
                    state.stage = MateLoginStage::Success;
                    state.user = Some(user.clone());
                    state.finished_at = Some(now_ms());
                    rt.set_state(state);
                    tracing::info!(account = %id, user = %user.user_name, "mate login succeeded");
                }
                FlowOutcome::Expired => {
                    if rt.current_generation(&id) == Some(generation) {
                        rt.cancels.lock().unwrap().remove(&id);
                    }
                }
                FlowOutcome::Cancelled => {
                    let mut state = rt.get_state(&id);
                    state.stage = MateLoginStage::Cancelled;
                    state.finished_at = Some(now_ms());
                    rt.set_state(state);
                }
                FlowOutcome::Failed(message) => {
                    tracing::error!(account = %id, error = %message, "mate login failed");
                    let mut state = rt.get_state(&id);
                    state.stage = MateLoginStage::Error;
                    state.error_message = Some(message);
                    state.finished_at = Some(now_ms());
                    rt.set_state(state);
                }
            }
            if rt.current_generation(&id) == Some(generation) {
                rt.cancels.lock().unwrap().remove(&id);
            }
        });
        state
    }

    /// Abort the in-flight flow. The flow task itself publishes `cancelled`
    /// (or stays silent when superseded); unknown ids stay `idle`
    /// (jieger `cancel`, `index.ts:416-422`).
    pub fn cancel(&self, account_id: &str) -> MateLoginState {
        if let Some((_, cancel)) = self.cancels.lock().unwrap().get(account_id) {
            cancel.cancel();
        }
        self.get_state(account_id)
    }
}

impl Default for MateLoginRuntime {
    fn default() -> Self {
        Self::new()
    }
}

// --- driver wiring -----------------------------------------------------------

impl super::TauriBrowserDriver {
    /// Start the mate QR flow. The account must be a registered
    /// `business_accounts` row of kind `kuaishou-mate`; login tokens are
    /// never persisted — each login re-scans a fresh QR code.
    pub async fn mate_login_start(&self, account_id: &str) -> Result<MateLoginState> {
        let accounts = self.business_accounts_list().await?;
        match accounts.iter().find(|a| a.id == account_id) {
            Some(account) if account.kind == BusinessAccountKind::KuaishouMate => {}
            Some(_) => {
                return Err(MultizenError::Config(format!(
                    "账号 `{account_id}` 不是直播伴侣账号（kind 必须为 kuaishou-mate），已拒绝扫码"
                )));
            }
            None => {
                return Err(MultizenError::NotFound(format!(
                    "直播伴侣账号 `{account_id}` 不存在，请刷新后重试"
                )));
            }
        }
        Ok(self.mate_login.begin(account_id))
    }

    pub async fn mate_login_cancel(&self, account_id: &str) -> Result<MateLoginState> {
        Ok(self.mate_login.cancel(account_id))
    }

    pub async fn mate_login_state(&self, account_id: &str) -> Result<MateLoginState> {
        Ok(self.mate_login.get_state(account_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::time::Duration;

    #[test]
    fn stage_wire_values_match_jieger() {
        let cases = [
            (MateLoginStage::Idle, "idle"),
            (MateLoginStage::Starting, "starting"),
            (MateLoginStage::AwaitingScan, "awaiting-scan"),
            (MateLoginStage::AwaitingConfirm, "awaiting-confirm"),
            (MateLoginStage::Receiving, "receiving"),
            (MateLoginStage::Success, "success"),
            (MateLoginStage::Expired, "expired"),
            (MateLoginStage::Cancelled, "cancelled"),
            (MateLoginStage::Error, "error"),
        ];
        for (stage, wire) in cases {
            assert_eq!(serde_json::to_value(stage).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<MateLoginStage>(json!(wire)).unwrap(),
                stage
            );
        }
    }

    #[test]
    fn result_code_table_matches_jieger_source() {
        // Extracted from F:/jieger/electron/main/tasks/mateLogin/index.ts:
        // `1` = ok everywhere; `707` = QR expired on the two long polls.
        assert_eq!(RESULT_OK, 1);
        assert_eq!(RESULT_EXPIRED, 707);
    }

    #[test]
    fn production_endpoints_and_state_shape_match_jieger() {
        let endpoints = MateLoginEndpoints::default();
        assert_eq!(endpoints.start, QR_START_URL);
        assert_eq!(endpoints.scan_result, SCAN_RESULT_URL);
        assert_eq!(endpoints.accept_result, ACCEPT_RESULT_URL);
        assert_eq!(endpoints.receive, RECEIVE_URL);
        assert_eq!(
            UA_BROWSER,
            "Mozilla/4.0 (compatible; MSIE 9.0; Windows NT 6.1)"
        );
        assert_eq!(UA_RECEIVE, "kuaishou 5.105.2.3505");
        assert_eq!(POLL_TIMEOUT_MS, 70_000);
        assert_eq!(POLL_RETRY_DELAY_MS, 1_500);
        assert_eq!(POLL_MAX_RETRIES, 5);
        assert_eq!(ONE_SHOT_TIMEOUT_MS, 20_000);

        let value = serde_json::to_value(MateLoginState::idle("a1")).unwrap();
        for key in [
            "accountId",
            "stage",
            "qrImageDataUrl",
            "qrLoginToken",
            "qrLoginSignature",
            "expireAt",
            "errorMessage",
            "user",
            "startedAt",
            "finishedAt",
        ] {
            assert!(value.get(key).is_some(), "missing state field {key}");
        }
    }

    #[test]
    fn cancel_without_flow_keeps_idle() {
        let rt = MateLoginRuntime::new();
        assert_eq!(rt.cancel("never-started").stage, MateLoginStage::Idle);
        assert_eq!(rt.get_state("never-started").stage, MateLoginStage::Idle);
    }

    async fn spawn_mock(router: Router) -> (MateLoginEndpoints, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (
            MateLoginEndpoints {
                start: format!("{base}/start"),
                scan_result: format!("{base}/scanResult"),
                accept_result: format!("{base}/acceptResult"),
                receive: format!("{base}/receive"),
            },
            handle,
        )
    }

    async fn wait_for(
        rt: &MateLoginRuntime,
        id: &str,
        mut pred: impl FnMut(&MateLoginState) -> bool,
        timeout: Duration,
    ) -> MateLoginState {
        let start = std::time::Instant::now();
        loop {
            let state = rt.get_state(id);
            if pred(&state) {
                return state;
            }
            assert!(
                start.elapsed() < timeout,
                "timed out waiting, last state: {state:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn user_json() -> Value {
        json!({"user_id": "u1001", "user_name": "测试主播", "headurl": "http://example.com/a.png"})
    }

    async fn start_ok() -> Json<Value> {
        Json(json!({
            "result": 1,
            "qrLoginToken": "tok",
            "qrLoginSignature": "sig",
            "expireTime": 1700000000000i64,
            "imageData": "aGk=",
        }))
    }

    async fn accept_ok() -> Json<Value> {
        Json(json!({"result": 1, "qrToken": "qr", "sid": "kuaishou.shop.b"}))
    }

    #[tokio::test]
    async fn full_flow_reaches_success() {
        async fn scan_ok() -> Json<Value> {
            // Small delay so the test can observe awaiting-scan first.
            tokio::time::sleep(Duration::from_millis(300)).await;
            Json(json!({"result": 1, "user": user_json()}))
        }
        async fn receive_ok() -> Json<Value> {
            Json(json!({"result": 1, "user": user_json()}))
        }
        let router = Router::new()
            .route("/start", post(start_ok))
            .route("/scanResult", post(scan_ok))
            .route("/acceptResult", post(accept_ok))
            .route("/receive", post(receive_ok));
        let (endpoints, server) = spawn_mock(router).await;
        let rt = Arc::new(MateLoginRuntime::with_endpoints(endpoints));

        let first = rt.begin("a-success");
        assert_eq!(first.stage, MateLoginStage::Starting);

        let scanning = wait_for(
            &rt,
            "a-success",
            |s| s.stage == MateLoginStage::AwaitingScan,
            Duration::from_secs(10),
        )
        .await;
        assert_eq!(
            scanning.qr_image_data_url.as_deref(),
            Some("data:image/png;base64,aGk=")
        );

        let done = wait_for(
            &rt,
            "a-success",
            |s| matches!(s.stage, MateLoginStage::Success | MateLoginStage::Error),
            Duration::from_secs(15),
        )
        .await;
        assert_eq!(done.stage, MateLoginStage::Success);
        let user = done.user.unwrap();
        assert_eq!(user.user_id, "u1001");
        assert_eq!(user.user_name, "测试主播");
        assert_eq!(user.avatar_url.as_deref(), Some("http://example.com/a.png"));
        assert!(done.finished_at.is_some());
        server.abort();
    }

    #[tokio::test]
    async fn scan_expired_maps_to_expired() {
        async fn scan_gone() -> Json<Value> {
            Json(json!({"result": 707}))
        }
        async fn receive_unreached() -> Json<Value> {
            panic!("receive must not run after expiry")
        }
        let router = Router::new()
            .route("/start", post(start_ok))
            .route("/scanResult", post(scan_gone))
            .route("/acceptResult", post(accept_ok))
            .route("/receive", post(receive_unreached));
        let (endpoints, server) = spawn_mock(router).await;
        let rt = Arc::new(MateLoginRuntime::with_endpoints(endpoints));

        rt.begin("a-expired");
        let done = wait_for(
            &rt,
            "a-expired",
            |s| matches!(s.stage, MateLoginStage::Expired | MateLoginStage::Error),
            Duration::from_secs(10),
        )
        .await;
        assert_eq!(done.stage, MateLoginStage::Expired);
        assert!(done.error_message.unwrap().contains("过期"));
        assert!(done.finished_at.is_some());
        server.abort();
    }

    #[tokio::test]
    async fn cancel_interrupts_long_poll() {
        async fn scan_hangs() -> Json<Value> {
            // Longer than any test deadline: only cancellation can end this.
            tokio::time::sleep(Duration::from_secs(60)).await;
            Json(json!({"result": 1, "user": user_json()}))
        }
        async fn receive_unreached() -> Json<Value> {
            panic!("receive must not run after cancel")
        }
        let router = Router::new()
            .route("/start", post(start_ok))
            .route("/scanResult", post(scan_hangs))
            .route("/acceptResult", post(accept_ok))
            .route("/receive", post(receive_unreached));
        let (endpoints, server) = spawn_mock(router).await;
        let rt = Arc::new(MateLoginRuntime::with_endpoints(endpoints));

        rt.begin("a-cancel");
        wait_for(
            &rt,
            "a-cancel",
            |s| s.stage == MateLoginStage::AwaitingScan,
            Duration::from_secs(10),
        )
        .await;
        rt.cancel("a-cancel");
        // Cancellation must land promptly even though the long poll would
        // block for another ~60 s; a sleep-polling cancel could not do this.
        let done = wait_for(
            &rt,
            "a-cancel",
            |s| s.stage == MateLoginStage::Cancelled,
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(done.stage, MateLoginStage::Cancelled);
        assert!(done.finished_at.is_some());
        server.abort();
    }

    #[tokio::test]
    async fn start_failure_maps_to_error() {
        async fn start_busy() -> Json<Value> {
            Json(json!({"result": 50001, "error_msg": "busy"}))
        }
        let router = Router::new()
            .route("/start", post(start_busy))
            .route("/scanResult", post(accept_ok))
            .route("/acceptResult", post(accept_ok))
            .route("/receive", post(accept_ok));
        let (endpoints, server) = spawn_mock(router).await;
        let rt = Arc::new(MateLoginRuntime::with_endpoints(endpoints));

        rt.begin("a-bad-start");
        let done = wait_for(
            &rt,
            "a-bad-start",
            |s| s.stage == MateLoginStage::Error,
            Duration::from_secs(10),
        )
        .await;
        assert!(done.error_message.unwrap().contains("50001"));
        server.abort();
    }

    #[tokio::test]
    async fn driver_start_rejects_unknown_account_and_wrong_kind() {
        let (_dir, driver) =
            crate::driver::business_tests::fixture(multizen_core::ChromixSettings::default());
        let err = driver.mate_login_start("absent").await.unwrap_err();
        assert!(err.to_string().contains("不存在"), "unexpected: {err}");

        let profile = driver
            .create_profile(multizen_core::CreateProfileInput {
                name: "mate guard fixture".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let shop = driver
            .business_accounts_save(multizen_core::SaveBusinessAccountInput {
                id: None,
                profile_id: profile.id.clone(),
                kind: multizen_core::BusinessAccountKind::KuaishouShop,
                display_name: "Shop".into(),
                platform_user_id: None,
            })
            .await
            .unwrap();
        let err = driver.mate_login_start(&shop.id).await.unwrap_err();
        assert!(err.to_string().contains("直播伴侣"), "unexpected: {err}");

        // No network is touched on the rejection paths above; these stay local.
        assert_eq!(
            driver.mate_login_state("absent").await.unwrap().stage,
            MateLoginStage::Idle
        );
        assert_eq!(
            driver.mate_login_cancel("absent").await.unwrap().stage,
            MateLoginStage::Idle
        );
        driver.shutdown().await;
    }
}
