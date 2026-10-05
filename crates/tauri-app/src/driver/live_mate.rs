//! 直播伴侣（liveMate）推流接口层。
//!
//! 逐行移植自 jieger 参考实现
//! `F:\jieger\electron\main\services\kuaishou\liveMate.ts`（Electron/TypeScript），
//! 保持完全一致的请求形状（multipart 手工构造、query/body 参与 NS 签名、固定
//! `pushInfo` JSON 等），便于与真机抓包/Node 实现交叉验证。
//!
//! # 五个端点
//!
//! | 步骤        | 端点                                                              |
//! |-------------|-------------------------------------------------------------------|
//! | authStatus  | `https://apijs.gifshow.com/rest/n/live/authStatus`                |
//! | prePush     | `https://apijs.ksapisrv.com/rest/n/live/mate/pc/prePushMobileOrigin` |
//! | startPush   | `https://api.gifshow.com/rest/n/live/mate/pc/startPush`           |
//! | stopPush    | `https://apijs.ksapisrv.com/rest/n/live/mate/pc/stopPush/v2`      |
//! | heartbeat   | `https://live.ksapisrv.com/rest/n/dynamicIcon/info`               |
//!
//! # 安全约定（token 策略）
//!
//! `mate_token` / `mate_st` 只出现在 **multipart body** 与签名基串中，
//! **绝不进入任何日志或事件**：本模块的 `tracing` 只记 `uid` / `result` /
//! HTTP 状态码 / 平台 `error_msg`；所有 `Err` 文案同样不含 token 值。
//! 参考实现里 `console.log` 也只打印 `uid` 与响应前 120 字符。
//!
//! # 端点可注入
//!
//! [`LiveMateEndpoints`] 允许测试把五个 URL 指向本地 axum mock（离线可跑）。
//! 生产默认值与 jieger 常量逐字一致。

use std::time::Duration;

use base64::Engine as _;
use md5::{Digest as _, Md5};
use multizen_core::{MultizenError, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::Sha256;

use super::kuaishou_sign::{build_signed_kuaishou_url, encode_component, sign_kuaishou_request};

/// App 版本号（jieger `APPVER`）。
pub const APPVER: &str = "5.105.2.3505";
/// 请求 UA（jieger `USER_AGENT`）。
pub const USER_AGENT: &str = "kuaishou 5.105.2.3505";
/// 快手 `client_key`（jieger `CLIENT_KEY`，同时决定签名 salt）。
pub const CLIENT_KEY: &str = "74901a18";
/// 单次请求超时（jieger `TIMEOUT_MS`）。
pub const TIMEOUT_MS: u64 = 45_000;
/// 拉取封面图的超时（jieger `fetchCover` 的 `AbortSignal.timeout(10_000)`）。
const COVER_TIMEOUT_MS: u64 = 10_000;

/// authStatus 端点。
pub const AUTH_STATUS_URL: &str = "https://apijs.gifshow.com/rest/n/live/authStatus";
/// prePushMobileOrigin 端点。
pub const PRE_PUSH_URL: &str = "https://apijs.ksapisrv.com/rest/n/live/mate/pc/prePushMobileOrigin";
/// startPush 端点。
pub const START_PUSH_URL: &str = "https://api.gifshow.com/rest/n/live/mate/pc/startPush";
/// stopPush/v2 端点。
pub const STOP_PUSH_URL: &str = "https://apijs.ksapisrv.com/rest/n/live/mate/pc/stopPush/v2";
/// 心跳（dynamicIcon/info）端点。
pub const DYNAMIC_ICON_URL: &str = "https://live.ksapisrv.com/rest/n/dynamicIcon/info";

/// 内置 1×1 JPEG 兜底封面（jieger `FALLBACK_JPEG`，base64 逐字照搬）。
const FALLBACK_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////2wBDAf//////////////////////////////////////////////////////////////////////////////////////wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAX/xAAVEAEBAAAAAAAAAAAAAAAAAAAAAf/aAAwDAQACEAMQAAAB9A//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAEFAqf/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAEDAQE/ASP/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAECAQE/ASP/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAY/Al//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/IV//2gAMAwEAAgADAAAAEP/EABQRAQAAAAAAAAAAAAAAAAAAABD/2gAIAQMBAT8QH//EABQRAQAAAAAAAAAAAAAAAAAAABD/2gAIAQIBAT8QH//EABQQAQAAAAAAAAAAAAAAAAAAABD/2gAIAQEAAT8QH//Z";

/// 五个端点（可注入以便离线测试）。
#[derive(Debug, Clone)]
pub struct LiveMateEndpoints {
    pub auth_status: String,
    pub pre_push: String,
    pub start_push: String,
    pub stop_push: String,
    pub dynamic_icon: String,
}

impl Default for LiveMateEndpoints {
    fn default() -> Self {
        Self {
            auth_status: AUTH_STATUS_URL.to_string(),
            pre_push: PRE_PUSH_URL.to_string(),
            start_push: START_PUSH_URL.to_string(),
            stop_push: STOP_PUSH_URL.to_string(),
            dynamic_icon: DYNAMIC_ICON_URL.to_string(),
        }
    }
}

/// 开播所需的账号凭证快照（来自 `mate_accounts` 记录）。
///
/// 仅承载接口层需要的字段；token 只在本结构体与请求体之间流动。
#[derive(Debug, Clone, Default)]
pub struct LiveMateAccount {
    pub account_id: String,
    pub platform_user_id: Option<String>,
    pub mate_token: Option<String>,
    pub mate_st: Option<String>,
    pub avatar_url: Option<String>,
}

/// 开播成功返回的推流凭证（jieger `LiveMateStreamCredentials`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveMateStreamCredentials {
    pub push_rtmp_url: String,
    pub live_stream_id: String,
}

/// 关播结果（jieger `stopLiveMatePush` 的 `{ ok, reason }`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveMateStopResult {
    pub ok: bool,
    pub reason: Option<String>,
}

/// RTMP 地址拆分结果（jieger `splitRtmpUrl`）。
///
/// 供下一节点（live_launch 接入）使用，当前仅测试消费，故 `allow(dead_code)`。
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtmpUrl {
    pub rtmp_server: String,
    pub stream_key: String,
}

// --- 纯计算：did / uid / pushInfo / query / multipart ------------------------

/// `sha256("{uid}|jieger")` 的**大写** hex 前 32 位，切成
/// `C_0-D_{0..12}-M_{12..24}-V_{24..32}`（jieger `createStableLiveMateDid`）。
pub fn create_stable_live_mate_did(uid: &str) -> String {
    let seed = format!("{:X}", Sha256::digest(format!("{uid}|jieger").as_bytes()));
    format!(
        "C_0-D_{}-M_{}-V_{}",
        &seed[0..12],
        &seed[12..24],
        &seed[24..32]
    )
}

/// 解析 uid：token 含 `-` 时取最后一个 `-` 之后的部分，否则回退 `platform_user_id`
/// （jieger `resolveUid`）。两处都为空时返回空串。
pub fn resolve_uid(platform_user_id: Option<&str>, token: &str) -> String {
    let token_uid = match token.rfind('-') {
        Some(index) => &token[index + 1..],
        None => "",
    };
    if !token_uid.is_empty() {
        token_uid.to_string()
    } else {
        platform_user_id.unwrap_or("").to_string()
    }
}

/// 固定 `pushInfo` JSON（字段名与顺序逐字照搬 jieger `buildPushInfo`）。
pub fn build_push_info() -> String {
    r#"{"gameId":1992,"initBitRate":8000,"keyFrameInterval":2000,"main_screen_size":"1920x1080","main_screen_type":["image_source"],"maxBitRate":12000,"minBitRate":4000,"resolution":4,"screen_streaming_type":"Portrait","videoCodec":10,"videoFrameRate":30,"videoQualityType":3}"#
        .to_string()
}

/// 公共 query（jieger `commonQuery`）。`did` 存在时插入在 `sys` 之后。
pub fn common_query(did: Option<&str>) -> Vec<String> {
    let mut query = vec![
        format!("appver={APPVER}"),
        "sys=PC_10".to_string(),
    ];
    if let Some(did) = did {
        query.push(format!("did={did}"));
    }
    query.extend([
        format!("client_key={CLIENT_KEY}"),
        "mobileCountryCode=".to_string(),
        "country_code=cn".to_string(),
        "language=zh-Hans-CN;q=1".to_string(),
        "kpn=KUAISHOU_LIVE_MATE".to_string(),
        "kpf=WINDOWS_PC".to_string(),
    ]);
    query
}

/// multipart 文件段。
struct MultipartFile {
    name: String,
    filename: String,
    content_type: String,
    data: Vec<u8>,
}

/// 小写 hex（用于 boundary）。
fn to_hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// `----kla-<24 hex>`（jieger `createBoundary`，12 随机字节 → 24 hex）。
fn create_boundary() -> String {
    let bytes: [u8; 12] = rand::random();
    format!("----kla-{}", to_hex_lower(&bytes))
}

/// 手工构造 multipart body（jieger `buildMultipart`）。
///
/// **严格照搬**格式：字段段 `--b\r\nContent-Disposition: form-data; name="X"\r\n\r\n<v>\r\n`；
/// 文件段额外带 `Content-Type: ...; charset=UTF-8` 与 `Content-Transfer-Encoding: binary`；
/// 末尾 `--b--\r\n`。签名与平台校验依赖该字节序列。
fn build_multipart_with_boundary(
    boundary: &str,
    fields: &[(String, String)],
    files: &[MultipartFile],
) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (name, value) in fields {
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        out.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    for file in files {
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        out.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                file.name, file.filename
            )
            .as_bytes(),
        );
        out.extend_from_slice(
            format!("Content-Type: {}; charset=UTF-8\r\n", file.content_type).as_bytes(),
        );
        out.extend_from_slice(b"Content-Transfer-Encoding: binary\r\n\r\n");
        out.extend_from_slice(&file.data);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}

/// 生成随机 boundary 并构造 multipart body。
fn build_multipart(fields: &[(String, String)], files: &[MultipartFile]) -> (Vec<u8>, String) {
    let boundary = create_boundary();
    let body = build_multipart_with_boundary(&boundary, fields, files);
    (body, boundary)
}

/// 构造签名 URL：query 与 body（= fields 的 `k=v`）一起参与 NS 签名，
/// `client_key` 固定为 [`CLIENT_KEY`]（jieger `signedUrl`）。文件字段不参与签名。
fn signed_url(base_url: &str, query: &[String], fields: &[(String, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    build_signed_kuaishou_url(base_url, query, &body, Some(CLIENT_KEY))
}

/// 按字符截断（对应 JS `text.slice(0, 120)`）。
fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// 内置兜底封面字节。
fn fallback_jpeg() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(FALLBACK_JPEG_B64)
        .expect("内置 1×1 JPEG 常量必须是合法 base64")
}

// --- 响应 DTO ---------------------------------------------------------------

/// 平台通用响应字段（`result` / `error_msg` / `message`）。
trait ResultFields {
    fn result(&self) -> Option<i64>;
    fn error_msg(&self) -> Option<&str>;
    fn message(&self) -> Option<&str>;
}

fn assert_result_ok<R: ResultFields>(resp: &R, step: &str) -> Result<()> {
    if resp.result() == Some(1) {
        return Ok(());
    }
    let result = resp
        .result()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let reason = resp.error_msg().or_else(|| resp.message()).unwrap_or("");
    Err(MultizenError::Mcp(
        format!("{step} 失败：result={result} {reason}")
            .trim()
            .to_string(),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatusResp {
    #[serde(default)]
    result: Option<i64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl ResultFields for AuthStatusResp {
    fn result(&self) -> Option<i64> {
        self.result
    }
    fn error_msg(&self) -> Option<&str> {
        self.error_msg.as_deref()
    }
    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrePushResp {
    #[serde(default)]
    result: Option<i64>,
    #[serde(default)]
    live_stream_id: Option<String>,
    #[serde(default)]
    pre_push_attach: Option<String>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl ResultFields for PrePushResp {
    fn result(&self) -> Option<i64> {
        self.result
    }
    fn error_msg(&self) -> Option<&str> {
        self.error_msg.as_deref()
    }
    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartPushResp {
    #[serde(default)]
    result: Option<i64>,
    #[serde(default)]
    live_stream_id: Option<String>,
    #[serde(default)]
    push_rtmp_url: Option<String>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl ResultFields for StartPushResp {
    fn result(&self) -> Option<i64> {
        self.result
    }
    fn error_msg(&self) -> Option<&str> {
        self.error_msg.as_deref()
    }
    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopPushResp {
    #[serde(default)]
    result: Option<i64>,
    #[serde(default)]
    live_stream_end_reason: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "error_msg")]
    error_msg: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl ResultFields for StopPushResp {
    fn result(&self) -> Option<i64> {
        self.result
    }
    fn error_msg(&self) -> Option<&str> {
        self.error_msg.as_deref()
    }
    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

// --- 客户端 -----------------------------------------------------------------

/// liveMate 接口客户端：持有 reqwest 连接池 + 可注入端点。
///
/// HTTPS 一律 `danger_accept_invalid_certs(true)`（jieger `rejectUnauthorized:false`）。
pub struct LiveMateClient {
    client: reqwest::Client,
    endpoints: LiveMateEndpoints,
}

impl LiveMateClient {
    pub fn new() -> Self {
        Self::with_endpoints(LiveMateEndpoints::default())
    }

    pub fn with_endpoints(endpoints: LiveMateEndpoints) -> Self {
        Self {
            client: reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .expect("live mate reqwest client"),
            endpoints,
        }
    }

    /// POST multipart，返回解析后的 JSON。
    ///
    /// 处理顺序与 jieger 一致：先尝试 `JSON.parse`（失败 → `返回非 JSON：HTTP <code> <前120字符>`），
    /// 再判 2xx（失败 → `HTTP <code>：<error_msg|message|前120字符>`）。
    async fn post_multipart_json<T: DeserializeOwned>(
        &self,
        url: &str,
        fields: &[(String, String)],
        files: &[MultipartFile],
    ) -> Result<T> {
        let (body, boundary) = build_multipart(fields, files);
        let content_length = body.len().to_string();
        let resp = self
            .client
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}; Charset=UTF-8"),
            )
            .header(reqwest::header::CONTENT_LENGTH, content_length)
            .header(reqwest::header::ACCEPT, "*/*")
            .header("Accept-Language", "zh-cn")
            .header(reqwest::header::REFERER, url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .header(reqwest::header::CACHE_CONTROL, "no-cache")
            .timeout(Duration::from_millis(TIMEOUT_MS))
            .body(body)
            .send()
            .await
            .map_err(|e| MultizenError::Mcp(format!("快手接口请求失败：{e}")))?;
        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| MultizenError::Mcp(format!("快手接口读取响应失败：{e}")))?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
            MultizenError::Mcp(format!(
                "快手接口返回非 JSON：HTTP {status} {}",
                truncate_chars(&text, 120)
            ))
        })?;
        if !(200..300).contains(&status) {
            let detail = value
                .get("error_msg")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("message").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| truncate_chars(&text, 120));
            return Err(MultizenError::Mcp(format!(
                "快手接口 HTTP {status}：{detail}"
            )));
        }
        serde_json::from_value(value)
            .map_err(|e| MultizenError::Mcp(format!("快手接口响应解析失败：{e}")))
    }

    /// 拉取封面：失败/空响应回退内置 1×1 JPEG（jieger `fetchCover`）。
    async fn fetch_cover(&self, avatar_url: Option<&str>) -> Vec<u8> {
        let Some(url) = avatar_url.filter(|url| !url.is_empty()) else {
            return fallback_jpeg();
        };
        let fetched = async {
            let resp = self
                .client
                .get(url)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .header(reqwest::header::ACCEPT, "image/*,*/*")
                .timeout(Duration::from_millis(COVER_TIMEOUT_MS))
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let bytes = resp.bytes().await.ok()?;
            if bytes.is_empty() {
                None
            } else {
                Some(bytes.to_vec())
            }
        }
        .await;
        fetched.unwrap_or_else(fallback_jpeg)
    }

    /// 三步开播（jieger `startLiveMatePush`）：authStatus → prePush → startPush。
    ///
    /// 成功返回 `pushRtmpUrl` + `liveStreamId`。
    pub async fn start_live_mate_push(
        &self,
        account: &LiveMateAccount,
    ) -> Result<LiveMateStreamCredentials> {
        let (token, mate_st) = credentials(account, "开播")?;
        let uid = resolve_uid(account.platform_user_id.as_deref(), &token);
        if uid.is_empty() {
            return Err(MultizenError::Config("账号缺少 UID，无法开播".into()));
        }
        let did = create_stable_live_mate_did(&uid);

        // ① authStatus
        let auth_fields = vec![
            ("token".to_string(), token.clone()),
            ("kuaishou.live.mate_st".to_string(), mate_st.clone()),
        ];
        let auth_url = signed_url(&self.endpoints.auth_status, &common_query(None), &auth_fields);
        let auth: AuthStatusResp = self
            .post_multipart_json(&auth_url, &auth_fields, &[])
            .await?;
        assert_result_ok(&auth, "authStatus")?;
        if let Some(status) = auth.status.as_deref() {
            if !status.is_empty() && status != "available" {
                return Err(MultizenError::Mcp(format!("直播伴侣登录态不可用：{status}")));
            }
        }

        // ② prePushMobileOrigin（query 带 did）
        let pre_fields = vec![
            ("isPaidShow".to_string(), "0".to_string()),
            ("isRePush".to_string(), "true".to_string()),
            ("kuaishou.live.mate_st".to_string(), mate_st.clone()),
            ("token".to_string(), token.clone()),
        ];
        let pre_query = common_query(Some(&did));
        let pre_url = signed_url(&self.endpoints.pre_push, &pre_query, &pre_fields);
        let pre: PrePushResp = self
            .post_multipart_json(&pre_url, &pre_fields, &[])
            .await?;
        assert_result_ok(&pre, "prePush")?;
        let (live_stream_id, pre_push_attach) = match (pre.live_stream_id, pre.pre_push_attach) {
            (Some(id), Some(attach)) if !id.is_empty() && !attach.is_empty() => (id, attach),
            _ => {
                return Err(MultizenError::Mcp(
                    "prePush 未返回 liveStreamId/prePushAttach".into(),
                ))
            }
        };

        // ③ startPush（17 个 fields + cover 文件）
        let start_fields = vec![
            ("announcement".to_string(), String::new()),
            ("anonymousLiveMode".to_string(), "2".to_string()),
            ("caption".to_string(), String::new()),
            ("enableRecruit".to_string(), "false".to_string()),
            ("enableShop".to_string(), "1".to_string()),
            ("hasLandscape".to_string(), "false".to_string()),
            ("isOriginalCover".to_string(), "1".to_string()),
            ("kuaishou.live.mate_st".to_string(), mate_st.clone()),
            ("liveMode".to_string(), "1".to_string()),
            ("liveStreamId".to_string(), live_stream_id),
            ("notificationLater".to_string(), "false".to_string()),
            ("prePushAttach".to_string(), pre_push_attach),
            ("privateType".to_string(), "0".to_string()),
            ("pushInfo".to_string(), build_push_info()),
            ("streamType".to_string(), "1".to_string()),
            ("token".to_string(), token.clone()),
            ("ud".to_string(), uid.clone()),
        ];
        let cover = self.fetch_cover(account.avatar_url.as_deref()).await;
        let files = vec![MultipartFile {
            name: "cover".to_string(),
            filename: "last.jpg".to_string(),
            content_type: "image/jpeg".to_string(),
            data: cover,
        }];
        let start_url = signed_url(&self.endpoints.start_push, &pre_query, &start_fields);
        let start: StartPushResp = self
            .post_multipart_json(&start_url, &start_fields, &files)
            .await?;
        assert_result_ok(&start, "startPush")?;
        let (push_rtmp_url, live_stream_id) = match (start.push_rtmp_url, start.live_stream_id) {
            (Some(url), Some(id)) if !url.is_empty() && !id.is_empty() => (url, id),
            _ => return Err(MultizenError::Mcp("startPush 未返回推流地址".into())),
        };

        tracing::info!(
            account = %account.account_id,
            uid = %uid,
            live_stream_id = %live_stream_id,
            "liveMate startPush ok"
        );
        Ok(LiveMateStreamCredentials {
            push_rtmp_url,
            live_stream_id,
        })
    }

    /// 关播（jieger `stopLiveMatePush`）。
    pub async fn stop_live_mate_push(
        &self,
        account: &LiveMateAccount,
        live_stream_id: &str,
    ) -> Result<LiveMateStopResult> {
        let (token, mate_st) = credentials(account, "关闭直播")?;
        let uid = resolve_uid(account.platform_user_id.as_deref(), &token);
        if uid.is_empty() {
            return Err(MultizenError::Config("账号缺少 UID，无法关闭直播".into()));
        }
        let did = create_stable_live_mate_did(&uid);
        let fields = vec![
            ("liveStreamId".to_string(), live_stream_id.to_string()),
            ("token".to_string(), token.clone()),
            ("kuaishou.live.mate_st".to_string(), mate_st.clone()),
        ];
        let url = signed_url(&self.endpoints.stop_push, &common_query(Some(&did)), &fields);
        let resp: StopPushResp = self.post_multipart_json(&url, &fields, &[]).await?;
        assert_result_ok(&resp, "stopPush")?;
        Ok(LiveMateStopResult {
            ok: true,
            reason: resp.live_stream_end_reason.or(resp.title),
        })
    }

    /// 心跳（jieger `sendLiveMateHeartbeat`）。**失败仅记日志，不返回错误**。
    pub async fn send_live_mate_heartbeat(&self, account: &LiveMateAccount) {
        let Some(token) = account
            .mate_token
            .as_deref()
            .filter(|token| !token.is_empty())
        else {
            return;
        };
        let uid = resolve_uid(account.platform_user_id.as_deref(), token);
        if uid.is_empty() {
            return;
        }
        let did = format!("{:x}", Md5::digest(uid.as_bytes()));
        let did = &did[0..12];

        let query_params = vec![
            "kpn=KUAISHOU".to_string(),
            "kpf=IPHONE".to_string(),
            format!("net={}", encode_component("中国电信_5")),
            "appver=5.11.3.1368".to_string(),
            format!("mod={}", encode_component("iPhone8,4")),
            "ver=5.11".to_string(),
            "c=a".to_string(),
            "sh=1136".to_string(),
            "sys=ios13.3.1".to_string(),
            "isp=CTCC".to_string(),
            format!("did={did}"),
        ];
        let body_params = vec![format!("authorId={uid}")];

        let mut signing: Vec<String> = query_params.clone();
        signing.extend(body_params.iter().cloned());
        let sig = sign_kuaishou_request(&signing, None, None);

        let url = format!("{}?{}", self.endpoints.dynamic_icon, query_params.join("&"));
        let body = format!("{}&sig={sig}", body_params.join("&"));

        let attempt = async {
            let resp = self
                .client
                .post(&url)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(reqwest::header::CONNECTION, "keep-alive")
                .header(reqwest::header::CONTENT_LENGTH, body.len().to_string())
                .timeout(Duration::from_millis(TIMEOUT_MS))
                .body(body)
                .send()
                .await
                .map_err(|e| format!("请求失败：{e}"))?;
            let text = resp.text().await.map_err(|e| format!("读取响应失败：{e}"))?;
            Ok::<String, String>(text)
        }
        .await;
        match attempt {
            Ok(text) => tracing::info!(
                account = %account.account_id,
                uid = %uid,
                resp = %truncate_chars(&text, 120),
                "liveMate heartbeat ok"
            ),
            Err(error) => tracing::warn!(
                account = %account.account_id,
                uid = %uid,
                error = %error,
                "liveMate heartbeat failed"
            ),
        }
    }
}

impl Default for LiveMateClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 校验凭证完整性，返回 `(token, mate_st)`。
///
/// 缺失或为空 → 「该直播伴侣账号未登录或凭证不完整，请先扫码登录」（jieger 原文）。
fn credentials(account: &LiveMateAccount, action: &str) -> Result<(String, String)> {
    match (account.mate_token.as_deref(), account.mate_st.as_deref()) {
        (Some(token), Some(mate_st)) if !token.is_empty() && !mate_st.is_empty() => {
            Ok((token.to_string(), mate_st.to_string()))
        }
        _ => Err(MultizenError::Config(format!(
            "该直播伴侣账号未登录或凭证不完整，请先扫码登录（{action}）"
        ))),
    }
}

/// 拆分 RTMP 推流地址（jieger `splitRtmpUrl`）。
///
/// 取最后一个 `/`；若其位置 ≤ `rtmp://`.len()（或不存在）则整串作为 server、key 为空。
#[allow(dead_code)]
pub fn split_rtmp_url(push_rtmp_url: &str) -> RtmpUrl {
    match push_rtmp_url.rfind('/') {
        Some(index) if index > "rtmp://".len() => RtmpUrl {
            rtmp_server: push_rtmp_url[..index].to_string(),
            stream_key: push_rtmp_url[index + 1..].to_string(),
        },
        _ => RtmpUrl {
            rtmp_server: push_rtmp_url.to_string(),
            stream_key: String::new(),
        },
    }
}

// --- driver wiring -----------------------------------------------------------

impl super::TauriBrowserDriver {
    /// 从 `mate_accounts` 读取凭证快照（token 不落日志）。
    async fn load_live_mate_account(&self, account_id: &str) -> Result<LiveMateAccount> {
        let id = account_id.to_string();
        let account = self
            .mate_store(move |pm| pm.mate_account_get(&id))
            .await?
            .ok_or_else(|| {
                MultizenError::NotFound(format!(
                    "直播伴侣账号 `{account_id}` 不存在，请刷新后重试"
                ))
            })?;
        Ok(LiveMateAccount {
            account_id: account.id,
            platform_user_id: account.platform_user_id,
            mate_token: account.mate_token,
            mate_st: account.mate_st,
            avatar_url: account.avatar_url,
        })
    }

    /// 三步开播，返回推流地址。
    pub async fn live_mate_start_push(
        &self,
        account_id: &str,
    ) -> Result<LiveMateStreamCredentials> {
        let account = self.load_live_mate_account(account_id).await?;
        self.live_mate.start_live_mate_push(&account).await
    }

    /// 关播。
    pub async fn live_mate_stop_push(
        &self,
        account_id: &str,
        live_stream_id: &str,
    ) -> Result<LiveMateStopResult> {
        let account = self.load_live_mate_account(account_id).await?;
        self.live_mate
            .stop_live_mate_push(&account, live_stream_id)
            .await
    }

    /// 心跳（best-effort）。
    pub async fn live_mate_heartbeat(&self, account_id: &str) -> Result<()> {
        let account = self.load_live_mate_account(account_id).await?;
        self.live_mate.send_live_mate_heartbeat(&account).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::extract::{Request, State};
    use axum::response::{IntoResponse, Response};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// 金标准向量：由 Node v24 直接运行 jieger 参考实现
    /// `liveMate.ts` + `sign.ts` 生成（见交接文档）。sig 分段书写以规避终端脱敏。
    const TOKEN: &str = "tok-abc-1234567890";
    const MATE_ST: &str = "st-abcdef";
    const PLATFORM_USER_ID: &str = "1234567890";
    const GOLDEN_UID: &str = "1234567890";
    const GOLDEN_DID: &str = "C_0-D_08B695B79F18-M_7A18B38CE89B-V_531B3E30";
    const GOLDEN_AUTH_SIG: &str = "c62af7716c6ebc36b1c3b3b64fcc4a85";
    const GOLDEN_PRE_SIG: &str = "6c2541030c5c0fed3ba17f7d741e804e";
    const GOLDEN_START_SIG: &str = "ffd7d009941f3801c99c846a8b26eb64";
    const GOLDEN_STOP_SIG: &str = "313b4e5b57f4fa3076d881ba21d10eae";
    const GOLDEN_HB_DID: &str = "e807f1fcf82d";
    const GOLDEN_HB_SIG: &str = "d1690ffe7602a5a49bf3f000f7c23a4d";
    const GOLDEN_PUSH_INFO: &str = r#"{"gameId":1992,"initBitRate":8000,"keyFrameInterval":2000,"main_screen_size":"1920x1080","main_screen_type":["image_source"],"maxBitRate":12000,"minBitRate":4000,"resolution":4,"screen_streaming_type":"Portrait","videoCodec":10,"videoFrameRate":30,"videoQualityType":3}"#;

    fn golden_account() -> LiveMateAccount {
        LiveMateAccount {
            account_id: "acct-1".to_string(),
            platform_user_id: Some(PLATFORM_USER_ID.to_string()),
            mate_token: Some(TOKEN.to_string()),
            mate_st: Some(MATE_ST.to_string()),
            avatar_url: None,
        }
    }

    // --- 纯计算 -------------------------------------------------------------

    #[test]
    fn did_is_deterministic_and_matches_golden() {
        let a = create_stable_live_mate_did(GOLDEN_UID);
        let b = create_stable_live_mate_did(GOLDEN_UID);
        assert_eq!(a, b);
        assert_eq!(a, GOLDEN_DID);
        assert_ne!(a, create_stable_live_mate_did("999"));
        // 格式：C_0-D_{12}-M_{12}-V_{8}（共 3 个 '-'）。
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(parts.len(), 4, "{a}");
        assert_eq!(parts[0], "C_0");
        assert_eq!(parts[1].len(), 2 + 12); // "D_" + 12 hex
        assert_eq!(parts[2].len(), 2 + 12); // "M_" + 12 hex
        assert_eq!(parts[3].len(), 2 + 8); // "V_" + 8 hex
        // 逐段核对：D_ 前 12 位、M_ 中 12 位、V_ 后 8 位。
        let seed = format!("{:X}", Sha256::digest(format!("{GOLDEN_UID}|jieger").as_bytes()));
        assert!(parts[1].starts_with("D_"));
        assert_eq!(&parts[1][2..], &seed[0..12]);
        assert!(parts[2].starts_with("M_"));
        assert_eq!(&parts[2][2..], &seed[12..24]);
        assert!(parts[3].starts_with("V_"));
        assert_eq!(&parts[3][2..], &seed[24..32]);
    }

    #[test]
    fn resolve_uid_matches_jieger() {
        // token 含 '-' → 取最后一个 '-' 之后。
        assert_eq!(resolve_uid(Some("999"), "a-b-c123"), "c123");
        // token 不含 '-' → 回退 platform_user_id。
        assert_eq!(resolve_uid(Some("999"), "nosep"), "999");
        // 都没有 → 空串。
        assert_eq!(resolve_uid(None, ""), "");
        assert_eq!(resolve_uid(None, "nosep"), "");
        // 尾部为空（token 以 '-' 结尾）→ 回退 platform_user_id。
        assert_eq!(resolve_uid(Some("999"), "x-"), "999");
    }

    #[test]
    fn push_info_fields_and_order_match_golden() {
        assert_eq!(build_push_info(), GOLDEN_PUSH_INFO);
    }

    #[test]
    fn common_query_matches_golden() {
        let without = common_query(None);
        assert_eq!(
            without,
            vec![
                "appver=5.105.2.3505".to_string(),
                "sys=PC_10".to_string(),
                "client_key=74901a18".to_string(),
                "mobileCountryCode=".to_string(),
                "country_code=cn".to_string(),
                "language=zh-Hans-CN;q=1".to_string(),
                "kpn=KUAISHOU_LIVE_MATE".to_string(),
                "kpf=WINDOWS_PC".to_string(),
            ]
        );
        let with = common_query(Some(GOLDEN_DID));
        assert_eq!(
            with,
            vec![
                "appver=5.105.2.3505".to_string(),
                "sys=PC_10".to_string(),
                format!("did={GOLDEN_DID}"),
                "client_key=74901a18".to_string(),
                "mobileCountryCode=".to_string(),
                "country_code=cn".to_string(),
                "language=zh-Hans-CN;q=1".to_string(),
                "kpn=KUAISHOU_LIVE_MATE".to_string(),
                "kpf=WINDOWS_PC".to_string(),
            ]
        );
        // did 插在 sys 之后、client_key 之前。
        assert_eq!(with.iter().position(|p| p.starts_with("did=")), Some(2));
        assert_eq!(
            with.iter().position(|p| p.starts_with("client_key=")),
            Some(3)
        );
    }

    #[test]
    fn split_rtmp_url_boundaries_match_golden() {
        assert_eq!(
            split_rtmp_url("rtmp://push.ksapisrv.com/live/abc123"),
            RtmpUrl {
                rtmp_server: "rtmp://push.ksapisrv.com/live".into(),
                stream_key: "abc123".into(),
            }
        );
        assert_eq!(
            split_rtmp_url("rtmp://a.b/c"),
            RtmpUrl {
                rtmp_server: "rtmp://a.b".into(),
                stream_key: "c".into(),
            }
        );
        // 最后一个 '/' 落在 "rtmp://" 之内 → 整串作为 server。
        assert_eq!(
            split_rtmp_url("rtmp://"),
            RtmpUrl {
                rtmp_server: "rtmp://".into(),
                stream_key: String::new(),
            }
        );
        // 无 '/'。
        assert_eq!(
            split_rtmp_url("nonsense"),
            RtmpUrl {
                rtmp_server: "nonsense".into(),
                stream_key: String::new(),
            }
        );
        // 取最后一个 '/'。
        assert_eq!(
            split_rtmp_url("rtmp://x/y/z"),
            RtmpUrl {
                rtmp_server: "rtmp://x/y".into(),
                stream_key: "z".into(),
            }
        );
    }

    #[test]
    fn signed_urls_match_golden() {
        let auth_fields = vec![
            ("token".to_string(), TOKEN.to_string()),
            ("kuaishou.live.mate_st".to_string(), MATE_ST.to_string()),
        ];
        let auth = signed_url(AUTH_STATUS_URL, &common_query(None), &auth_fields);
        assert_eq!(
            auth,
            format!(
                "https://apijs.gifshow.com/rest/n/live/authStatus?appver=5.105.2.3505&sys=PC_10&client_key=74901a18&mobileCountryCode=&country_code=cn&language=zh-Hans-CN;q=1&kpn=KUAISHOU_LIVE_MATE&kpf=WINDOWS_PC&sig={GOLDEN_AUTH_SIG}"
            )
        );
        assert_eq!(auth.matches("sig=").count(), 1);

        let pre_fields = vec![
            ("isPaidShow".to_string(), "0".to_string()),
            ("isRePush".to_string(), "true".to_string()),
            ("kuaishou.live.mate_st".to_string(), MATE_ST.to_string()),
            ("token".to_string(), TOKEN.to_string()),
        ];
        let pre = signed_url(PRE_PUSH_URL, &common_query(Some(GOLDEN_DID)), &pre_fields);
        assert_eq!(
            pre,
            format!(
                "https://apijs.ksapisrv.com/rest/n/live/mate/pc/prePushMobileOrigin?appver=5.105.2.3505&sys=PC_10&did={GOLDEN_DID}&client_key=74901a18&mobileCountryCode=&country_code=cn&language=zh-Hans-CN;q=1&kpn=KUAISHOU_LIVE_MATE&kpf=WINDOWS_PC&sig={GOLDEN_PRE_SIG}"
            )
        );

        let start_fields = vec![
            ("announcement".to_string(), String::new()),
            ("anonymousLiveMode".to_string(), "2".to_string()),
            ("caption".to_string(), String::new()),
            ("enableRecruit".to_string(), "false".to_string()),
            ("enableShop".to_string(), "1".to_string()),
            ("hasLandscape".to_string(), "false".to_string()),
            ("isOriginalCover".to_string(), "1".to_string()),
            ("kuaishou.live.mate_st".to_string(), MATE_ST.to_string()),
            ("liveMode".to_string(), "1".to_string()),
            ("liveStreamId".to_string(), "LS-1".to_string()),
            ("notificationLater".to_string(), "false".to_string()),
            ("prePushAttach".to_string(), "ATT-1".to_string()),
            ("privateType".to_string(), "0".to_string()),
            ("pushInfo".to_string(), build_push_info()),
            ("streamType".to_string(), "1".to_string()),
            ("token".to_string(), TOKEN.to_string()),
            ("ud".to_string(), GOLDEN_UID.to_string()),
        ];
        let start = signed_url(START_PUSH_URL, &common_query(Some(GOLDEN_DID)), &start_fields);
        assert_eq!(
            start,
            format!(
                "https://api.gifshow.com/rest/n/live/mate/pc/startPush?appver=5.105.2.3505&sys=PC_10&did={GOLDEN_DID}&client_key=74901a18&mobileCountryCode=&country_code=cn&language=zh-Hans-CN;q=1&kpn=KUAISHOU_LIVE_MATE&kpf=WINDOWS_PC&sig={GOLDEN_START_SIG}"
            )
        );

        let stop_fields = vec![
            ("liveStreamId".to_string(), "LS-1".to_string()),
            ("token".to_string(), TOKEN.to_string()),
            ("kuaishou.live.mate_st".to_string(), MATE_ST.to_string()),
        ];
        let stop = signed_url(STOP_PUSH_URL, &common_query(Some(GOLDEN_DID)), &stop_fields);
        assert_eq!(
            stop,
            format!(
                "https://apijs.ksapisrv.com/rest/n/live/mate/pc/stopPush/v2?appver=5.105.2.3505&sys=PC_10&did={GOLDEN_DID}&client_key=74901a18&mobileCountryCode=&country_code=cn&language=zh-Hans-CN;q=1&kpn=KUAISHOU_LIVE_MATE&kpf=WINDOWS_PC&sig={GOLDEN_STOP_SIG}"
            )
        );
    }

    #[test]
    fn multipart_format_matches_reference() {
        let fields = vec![
            ("token".to_string(), "T".to_string()),
            ("kuaishou.live.mate_st".to_string(), "S".to_string()),
        ];
        let files = vec![MultipartFile {
            name: "cover".to_string(),
            filename: "last.jpg".to_string(),
            content_type: "image/jpeg".to_string(),
            data: vec![0x01, 0x02],
        }];
        let boundary = "----kla-00112233445566778899aabb";
        let body = build_multipart_with_boundary(boundary, &fields, &files);
        let expected = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"token\"\r\n\r\nT\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"kuaishou.live.mate_st\"\r\n\r\nS\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"cover\"; filename=\"last.jpg\"\r\n\
             Content-Type: image/jpeg; charset=UTF-8\r\nContent-Transfer-Encoding: binary\r\n\r\n"
        );
        let mut expected_bytes = expected.into_bytes();
        expected_bytes.extend_from_slice(&[0x01, 0x02]);
        expected_bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        assert_eq!(body, expected_bytes);

        // 随机 boundary 形状：----kla- + 24 hex。
        let random = create_boundary();
        assert!(random.starts_with("----kla-"), "{random}");
        assert_eq!(random.len(), "----kla-".len() + 24);
        assert!(random["----kla-".len()..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn fallback_cover_is_valid_jpeg() {
        let bytes = fallback_jpeg();
        assert!(bytes.starts_with(&[0xFF, 0xD8]), "JPEG SOI");
        assert!(bytes.ends_with(&[0xFF, 0xD9]), "JPEG EOI");
        // 与参考实现 FALLBACK_JPEG 常量逐字节一致（base64 解码长度）。
        assert_eq!(bytes.len(), 519);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(FALLBACK_JPEG_B64)
                .unwrap(),
            bytes
        );
    }

    // --- 离线 mock HTTP -----------------------------------------------------

    #[derive(Debug, Clone)]
    struct CapturedReq {
        method: String,
        path: String,
        query: String,
        body: Vec<u8>,
        content_type: Option<String>,
    }

    #[derive(Default)]
    struct MockState {
        captured: Mutex<Vec<CapturedReq>>,
        replies: Mutex<HashMap<String, Value>>,
        statuses: Mutex<HashMap<String, u16>>,
        /// path → 原始响应体（text/plain），优先于 `replies`（用于非 JSON 场景）。
        raw: Mutex<HashMap<String, String>>,
    }

    async fn mock_handler(State(state): State<Arc<MockState>>, req: Request) -> Response {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        let content_type = req
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let body = to_bytes(req.into_body(), usize::MAX).await.unwrap().to_vec();
        state.captured.lock().unwrap().push(CapturedReq {
            method,
            path: path.clone(),
            query,
            body,
            content_type,
        });
        let status = state
            .statuses
            .lock()
            .unwrap()
            .get(&path)
            .copied()
            .unwrap_or(200);
        if let Some(text) = state.raw.lock().unwrap().get(&path).cloned() {
            return (
                axum::http::StatusCode::from_u16(status).unwrap(),
                [(reqwest::header::CONTENT_TYPE, "text/plain")],
                text,
            )
                .into_response();
        }
        let reply = state
            .replies
            .lock()
            .unwrap()
            .get(&path)
            .cloned()
            .unwrap_or_else(|| json!({"result": 1}));
        (
            axum::http::StatusCode::from_u16(status).unwrap(),
            Json(reply),
        )
            .into_response()
    }

    async fn spawn_mock(
        replies: Vec<(&str, Value)>,
    ) -> (LiveMateEndpoints, Arc<MockState>, tokio::task::JoinHandle<()>) {
        let state = Arc::new(MockState::default());
        {
            let mut map = state.replies.lock().unwrap();
            for (path, value) in replies {
                map.insert(path.to_string(), value);
            }
        }
        let router = Router::new().fallback(mock_handler).with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let endpoints = LiveMateEndpoints {
            auth_status: format!("{base}/authStatus"),
            pre_push: format!("{base}/prePush"),
            start_push: format!("{base}/startPush"),
            stop_push: format!("{base}/stopPush"),
            dynamic_icon: format!("{base}/dynamicIcon"),
        };
        (endpoints, state, handle)
    }

    fn captured_body_text(req: &CapturedReq) -> String {
        String::from_utf8_lossy(&req.body).to_string()
    }

    #[tokio::test]
    async fn start_push_success_returns_rtmp() {
        let (endpoints, state, server) = spawn_mock(vec![
            (
                "/authStatus",
                json!({"result": 1, "status": "available"}),
            ),
            (
                "/prePush",
                json!({"result": 1, "liveStreamId": "LS-77", "prePushAttach": "ATT-88"}),
            ),
            (
                "/startPush",
                json!({"result": 1, "liveStreamId": "LS-77", "pushRtmpUrl": "rtmp://push.ksapisrv.com/live/abc123"}),
            ),
        ])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        let creds = client
            .start_live_mate_push(&golden_account())
            .await
            .expect("start push");
        assert_eq!(creds.live_stream_id, "LS-77");
        assert_eq!(creds.push_rtmp_url, "rtmp://push.ksapisrv.com/live/abc123");
        assert_eq!(
            split_rtmp_url(&creds.push_rtmp_url),
            RtmpUrl {
                rtmp_server: "rtmp://push.ksapisrv.com/live".into(),
                stream_key: "abc123".into(),
            }
        );

        let captured = state.captured.lock().unwrap().clone();
        assert_eq!(captured.len(), 3, "auth + pre + start");
        // 顺序：authStatus（无 did）→ prePush（带 did）→ startPush（带 did）。
        assert!(captured[0].query.contains("client_key=74901a18"));
        assert!(!captured[0].query.contains(&format!("did={GOLDEN_DID}")));
        assert!(captured[1].query.contains(&format!("did={GOLDEN_DID}")));
        assert!(captured[2].query.contains(&format!("did={GOLDEN_DID}")));
        assert!(captured[0].query.contains("&sig="));
        assert!(captured[1].query.contains("&sig="));
        assert!(captured[2].query.contains("&sig="));

        // multipart 头 + 内容。
        for req in &captured {
            let ct = req.content_type.as_deref().unwrap_or("");
            assert!(ct.starts_with("multipart/form-data; boundary=----kla-"), "{ct}");
            assert!(ct.ends_with("; Charset=UTF-8"), "{ct}");
        }
        let auth_body = captured_body_text(&captured[0]);
        assert!(auth_body.contains("Content-Disposition: form-data; name=\"token\""));
        assert!(auth_body.contains("Content-Disposition: form-data; name=\"kuaishou.live.mate_st\""));
        assert!(auth_body.starts_with("--"));
        assert!(auth_body.ends_with("--\r\n"));

        // startPush body：17 个字段 + cover 文件段。
        let start_body = captured_body_text(&captured[2]);
        for name in [
            "announcement",
            "anonymousLiveMode",
            "caption",
            "enableRecruit",
            "enableShop",
            "hasLandscape",
            "isOriginalCover",
            "kuaishou.live.mate_st",
            "liveMode",
            "liveStreamId",
            "notificationLater",
            "prePushAttach",
            "privateType",
            "pushInfo",
            "streamType",
            "token",
            "ud",
        ] {
            assert!(
                start_body.contains(&format!("name=\"{name}\"")),
                "missing field {name}"
            );
        }
        assert!(start_body.contains("name=\"cover\"; filename=\"last.jpg\""));
        assert!(start_body.contains("Content-Type: image/jpeg; charset=UTF-8"));
        assert!(start_body.contains("Content-Transfer-Encoding: binary"));
        // pushInfo 原样内嵌。
        assert!(start_body.contains(GOLDEN_PUSH_INFO));
        assert!(start_body.contains("name=\"ud\"\r\n\r\n1234567890"));
        // prePush 回传值被带入 startPush。
        assert!(start_body.contains("name=\"liveStreamId\"\r\n\r\nLS-77"));
        assert!(start_body.contains("name=\"prePushAttach\"\r\n\r\nATT-88"));
        server.abort();
    }

    #[tokio::test]
    async fn start_push_failure_reports_step_without_token() {
        let (endpoints, _state, server) = spawn_mock(vec![(
            "/authStatus",
            json!({"result": 50001, "error_msg": "busy"}),
        )])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        let err = client
            .start_live_mate_push(&golden_account())
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("authStatus"), "{message}");
        assert!(message.contains("50001"), "{message}");
        assert!(message.contains("busy"), "{message}");
        // token / mate_st 绝不外泄到错误文案。
        assert!(!message.contains(TOKEN), "{message}");
        assert!(!message.contains(MATE_ST), "{message}");
        server.abort();
    }

    #[tokio::test]
    async fn start_push_missing_attach_is_error() {
        let (endpoints, _state, server) = spawn_mock(vec![
            ("/authStatus", json!({"result": 1})),
            ("/prePush", json!({"result": 1, "liveStreamId": "LS-1"})),
        ])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        let err = client
            .start_live_mate_push(&golden_account())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("prePush"), "{err}");
        server.abort();
    }

    #[tokio::test]
    async fn start_push_unavailable_status_is_error() {
        let (endpoints, _state, server) = spawn_mock(vec![(
            "/authStatus",
            json!({"result": 1, "status": "logout"}),
        )])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        let err = client
            .start_live_mate_push(&golden_account())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("不可用"), "{err}");
        assert!(!err.to_string().contains(TOKEN));
        server.abort();
    }

    #[tokio::test]
    async fn http_error_is_reported_with_platform_message() {
        let (endpoints, state, server) = spawn_mock(vec![(
            "/authStatus",
            json!({"error_msg": "teapot"}),
        )])
        .await;
        state
            .statuses
            .lock()
            .unwrap()
            .insert("/authStatus".to_string(), 500);
        let client = LiveMateClient::with_endpoints(endpoints);
        let err = client
            .start_live_mate_push(&golden_account())
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("HTTP 500"), "{message}");
        assert!(message.contains("teapot"), "{message}");
        assert!(!message.contains(TOKEN));
        server.abort();
    }

    #[tokio::test]
    async fn non_json_response_is_reported() {
        let (endpoints, state, server) = spawn_mock(vec![]).await;
        state
            .raw
            .lock()
            .unwrap()
            .insert("/authStatus".to_string(), "<html>gateway</html>".to_string());
        let client = LiveMateClient::with_endpoints(endpoints);
        let err = client
            .start_live_mate_push(&golden_account())
            .await
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("非 JSON"), "{message}");
        assert!(message.contains("HTTP 200"), "{message}");
        assert!(message.contains("gateway"), "{message}");
        assert!(!message.contains(TOKEN));
        server.abort();
    }

    #[tokio::test]
    async fn stop_push_call_shape() {
        let (endpoints, state, server) = spawn_mock(vec![(
            "/stopPush",
            json!({"result": 1, "liveStreamEndReason": "user-stop"}),
        )])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        let result = client
            .stop_live_mate_push(&golden_account(), "LS-99")
            .await
            .unwrap();
        assert!(result.ok);
        assert_eq!(result.reason.as_deref(), Some("user-stop"));

        let captured = state.captured.lock().unwrap().clone();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, "POST");
        assert!(captured[0].query.contains(&format!("did={GOLDEN_DID}")));
        assert!(captured[0].query.contains("&sig="));
        let body = captured_body_text(&captured[0]);
        assert!(body.contains("name=\"liveStreamId\"\r\n\r\nLS-99"));
        assert!(body.contains("name=\"token\""));
        assert!(body.contains("name=\"kuaishou.live.mate_st\""));
        server.abort();
    }

    #[tokio::test]
    async fn stop_push_falls_back_to_title() {
        let (endpoints, _state, server) = spawn_mock(vec![(
            "/stopPush",
            json!({"result": 1, "title": "直播已结束"}),
        )])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        let result = client
            .stop_live_mate_push(&golden_account(), "LS-99")
            .await
            .unwrap();
        assert_eq!(result.reason.as_deref(), Some("直播已结束"));
        server.abort();
    }

    #[tokio::test]
    async fn heartbeat_call_shape_matches_golden() {
        let (endpoints, state, server) = spawn_mock(vec![(
            "/dynamicIcon",
            json!({"result": 1}),
        )])
        .await;
        let client = LiveMateClient::with_endpoints(endpoints);
        client.send_live_mate_heartbeat(&golden_account()).await;

        let captured = state.captured.lock().unwrap().clone();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, "POST");
        assert_eq!(
            captured[0].content_type.as_deref(),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            captured[0].query,
            format!(
                "kpn=KUAISHOU&kpf=IPHONE&net=%E4%B8%AD%E5%9B%BD%E7%94%B5%E4%BF%A1_5&appver=5.11.3.1368&mod=iPhone8%2C4&ver=5.11&c=a&sh=1136&sys=ios13.3.1&isp=CTCC&did={GOLDEN_HB_DID}"
            )
        );
        assert_eq!(
            captured_body_text(&captured[0]),
            format!("authorId={GOLDEN_UID}&sig={GOLDEN_HB_SIG}")
        );
        server.abort();
    }

    #[tokio::test]
    async fn heartbeat_swallows_failure() {
        // 端点不存在（连接失败）也必须静默返回。
        let endpoints = LiveMateEndpoints {
            auth_status: "http://127.0.0.1:1/a".into(),
            pre_push: "http://127.0.0.1:1/p".into(),
            start_push: "http://127.0.0.1:1/s".into(),
            stop_push: "http://127.0.0.1:1/t".into(),
            dynamic_icon: "http://127.0.0.1:1/d".into(),
        };
        let client = LiveMateClient::with_endpoints(endpoints);
        // 不应 panic，也不应返回错误。
        client.send_live_mate_heartbeat(&golden_account()).await;
    }

    #[tokio::test]
    async fn heartbeat_noop_without_token() {
        let mut account = golden_account();
        account.mate_token = None;
        let (endpoints, state, server) = spawn_mock(vec![("/dynamicIcon", json!({"result": 1}))]).await;
        let client = LiveMateClient::with_endpoints(endpoints);
        client.send_live_mate_heartbeat(&account).await;
        assert!(state.captured.lock().unwrap().is_empty());
        server.abort();
    }

    #[tokio::test]
    async fn cover_falls_back_to_builtin_jpeg_on_bad_avatar() {
        let (endpoints, state, server) = spawn_mock(vec![
            ("/authStatus", json!({"result": 1})),
            (
                "/prePush",
                json!({"result": 1, "liveStreamId": "LS-1", "prePushAttach": "ATT-1"}),
            ),
            (
                "/startPush",
                json!({"result": 1, "liveStreamId": "LS-1", "pushRtmpUrl": "rtmp://a/b/c"}),
            ),
        ])
        .await;
        // avatar 指向不可达地址（连接失败）→ 应回退内置 1×1 JPEG。
        let mut account = golden_account();
        account.avatar_url = Some("http://127.0.0.1:1/avatar.png".to_string());
        let client = LiveMateClient::with_endpoints(endpoints);
        let creds = client.start_live_mate_push(&account).await.unwrap();
        assert_eq!(creds.live_stream_id, "LS-1");

        let captured = state.captured.lock().unwrap().clone();
        let start = captured.iter().find(|r| r.path == "/startPush").unwrap();
        let body = start.body.clone();
        let fallback = fallback_jpeg();
        assert!(
            body.windows(fallback.len()).any(|w| w == fallback.as_slice()),
            "cover 应回退到内置 1×1 JPEG"
        );
        server.abort();
    }

    #[tokio::test]
    async fn cover_uses_avatar_bytes_when_available() {
        let (endpoints, state, server) = spawn_mock(vec![
            ("/authStatus", json!({"result": 1})),
            (
                "/prePush",
                json!({"result": 1, "liveStreamId": "LS-1", "prePushAttach": "ATT-1"}),
            ),
            (
                "/startPush",
                json!({"result": 1, "liveStreamId": "LS-1", "pushRtmpUrl": "rtmp://a/b/c"}),
            ),
        ])
        .await;
        // 真实封面：mock 的 /avatar 返回一段自定义字节（text/plain），应原样作为 cover。
        let avatar_bytes = b"AVATAR-BYTES-42".to_vec();
        state.raw.lock().unwrap().insert(
            "/avatar".to_string(),
            String::from_utf8(avatar_bytes.clone()).unwrap(),
        );
        let base = endpoints
            .start_push
            .trim_end_matches("/startPush")
            .to_string();
        let mut account = golden_account();
        account.avatar_url = Some(format!("{base}/avatar"));
        let client = LiveMateClient::with_endpoints(endpoints);
        client.start_live_mate_push(&account).await.unwrap();

        let captured = state.captured.lock().unwrap().clone();
        let start = captured.iter().find(|r| r.path == "/startPush").unwrap();
        assert!(
            start
                .body
                .windows(avatar_bytes.len())
                .any(|w| w == avatar_bytes.as_slice()),
            "cover 应使用拉取到的头像字节"
        );
        server.abort();
    }

    #[tokio::test]
    async fn credentials_missing_is_reported_without_token() {
        let client = LiveMateClient::with_endpoints(LiveMateEndpoints {
            auth_status: "http://127.0.0.1:1/a".into(),
            pre_push: "http://127.0.0.1:1/p".into(),
            start_push: "http://127.0.0.1:1/s".into(),
            stop_push: "http://127.0.0.1:1/t".into(),
            dynamic_icon: "http://127.0.0.1:1/d".into(),
        });
        let mut account = golden_account();
        account.mate_token = None;
        let err = client.start_live_mate_push(&account).await.unwrap_err();
        assert!(err.to_string().contains("未登录或凭证不完整"), "{err}");

        let mut account = golden_account();
        account.mate_st = Some(String::new());
        let err = client.stop_live_mate_push(&account, "LS-1").await.unwrap_err();
        assert!(err.to_string().contains("未登录或凭证不完整"), "{err}");
    }

    #[tokio::test]
    async fn driver_rejects_account_without_credentials() {
        let (_dir, driver) =
            crate::driver::business_tests::fixture(multizen_core::ChromixSettings::default());
        let account = driver.mate_account_add(Some("伴侣未登录")).await.unwrap();
        // 未扫码登录 → 凭证不完整，且不发任何网络请求。
        let err = driver
            .live_mate_start_push(&account.id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("未登录或凭证不完整"), "{err}");

        let err = driver
            .live_mate_stop_push(&account.id, "LS-1")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("未登录或凭证不完整"), "{err}");

        // 不存在的账号 → NotFound。
        let err = driver.live_mate_start_push("absent").await.unwrap_err();
        assert!(err.to_string().contains("不存在"), "{err}");
        driver.shutdown().await;
    }

    #[tokio::test]
    async fn driver_heartbeat_is_best_effort_for_unknown_account() {
        let (_dir, driver) =
            crate::driver::business_tests::fixture(multizen_core::ChromixSettings::default());
        // 未知账号：读取凭证即失败 → 返回 NotFound（调用方按 best-effort 忽略）。
        let err = driver.live_mate_heartbeat("absent").await.unwrap_err();
        assert!(err.to_string().contains("不存在"), "{err}");
        driver.shutdown().await;
    }
}
