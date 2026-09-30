//! Narrow public-avatar GET. Not a general fetch/raw-CDP capability.
use crate::TaskCancel;
use base64::Engine;
use chromiumoxide::{
    cdp::{
        browser_protocol::{fetch, io, network, page::CreateIsolatedWorldParams},
        js_protocol::runtime::{EvaluateParams, ExecutionContextId},
    },
    Page,
};
use futures::StreamExt;
use multizen_core::{MultizenError, Result};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

pub const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(1);
const WAIT: Duration = Duration::from_secs(3);

pub fn trusted_avatar_url(raw: &str) -> bool {
    if raw.split_once("://").is_some_and(|(_, v)| {
        v.split(['/', '?', '#'])
            .next()
            .is_some_and(|v| v.contains('@'))
    }) {
        return false;
    }
    raw.len() <= 2048
        && reqwest::Url::parse(raw).is_ok_and(|u| {
            u.scheme() == "https"
                && u.port_or_known_default() == Some(443)
                && u.username().is_empty()
                && u.password().is_none()
                && u.fragment().is_none()
                && !u.path().to_ascii_lowercase().ends_with(".svg")
                && !u.path().to_ascii_lowercase().ends_with(".svgz")
                && u.host_str()
                    .is_some_and(|h| h == "yximgs.com" || h.ends_with(".yximgs.com"))
        })
}
fn error(_: impl std::fmt::Display) -> MultizenError {
    MultizenError::Cdp("public avatar browser GET unavailable".into())
}
fn pattern(url: &str) -> String {
    url.replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('?', "\\?")
}
fn eval(expression: String, context: ExecutionContextId) -> EvaluateParams {
    let mut p = EvaluateParams::new(expression);
    p.context_id = Some(context);
    p.return_by_value = Some(true);
    p
}

// Independent cleanup continues when the caller drops its controlled future.
struct CancelOnDrop(TaskCancel);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(crate) async fn read(
    page: &Page,
    url: &str,
    lock: std::sync::Arc<tokio::sync::Mutex<()>>,
) -> Result<String> {
    if !trusted_avatar_url(url) {
        return Err(error("untrusted"));
    }
    let cancel = TaskCancel::new();
    let guard = CancelOnDrop(cancel.clone());
    let page = page.clone();
    let url = url.to_owned();
    // A canceled TaskPage can be released before cleanup completes. Keep this
    // separate session/target lock in the worker until Fetch cleanup has finished.
    let lease = lock.lock_owned().await;
    let result = tokio::spawn(async move {
        let _lease = lease;
        worker(page, url, cancel).await
    })
    .await
    .map_err(error)?;
    drop(guard);
    result
}

async fn worker(page: Page, url: String, cancel: TaskCancel) -> Result<String> {
    // Creating an isolated world does not grant universal access or change CORS.
    let mut context = None;
    let mut request = None;
    let mut stream = None;
    let mut enabled = false;
    let marker = format!(
        "cloaksession-avatar-{}",
        SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let key = serde_json::to_string(&marker).map_err(error)?;
    let work = async {
        let frame = page
            .mainframe()
            .await
            .map_err(error)?
            .ok_or_else(|| error("frame"))?;
        let mut world = CreateIsolatedWorldParams::new(frame.clone());
        world.world_name = Some("cloaksession-public-avatar".into());
        world.grant_univeral_access = Some(false);
        let ctx = page
            .execute(world)
            .await
            .map_err(error)?
            .result
            .execution_context_id;
        context = Some(ctx);
        let mut requests = page
            .event_listener::<network::EventRequestWillBeSent>()
            .await
            .map_err(error)?;
        let mut responses = page
            .event_listener::<fetch::EventRequestPaused>()
            .await
            .map_err(error)?;
        // Only Fetch requests to the exact escaped URL, never Document/Image/XHR.
        let opts = fetch::EnableParams::builder()
            .pattern(
                fetch::RequestPattern::builder()
                    .url_pattern(pattern(&url))
                    .resource_type(network::ResourceType::Fetch)
                    .request_stage(fetch::RequestStage::Response)
                    .build(),
            )
            .build();
        enabled = true; // cleanup even when the reply is lost
        page.execute(opts).await.map_err(error)?;
        let script = format!("(() => {{ const c = new AbortController(); globalThis[{key}] = c; void fetch({}, {{method:'GET', mode:'cors', credentials:'omit', redirect:'error', referrerPolicy:'no-referrer', signal:AbortSignal.any([c.signal, AbortSignal.timeout(3000)])}}).catch(()=>{{}}); return true; }})()\n//# sourceURL={marker}", serde_json::to_string(&url).map_err(error)?);
        let started = page.execute(eval(script, ctx)).await.map_err(error)?.result;
        if started.exception_details.is_some() {
            return Err(error("start"));
        }
        let net_id = loop {
            let event = requests.next().await.ok_or_else(|| error("events"))?;
            if event.request.url == url
                && event.request.method == "GET"
                && event.frame_id.as_ref() == Some(&frame)
                && event
                    .initiator
                    .stack
                    .as_ref()
                    .is_some_and(|s| s.call_frames.iter().any(|c| c.url == marker))
            {
                if event.redirect_response.is_some() {
                    return Err(error("redirect"));
                }
                break event.request_id.clone();
            }
        };
        let response = loop {
            let event = responses.next().await.ok_or_else(|| error("responses"))?;
            if event.network_id.as_ref() == Some(&net_id) {
                break event;
            }
            // Preserve independently initiated requests, even to the same avatar URL.
            page.execute(fetch::ContinueResponseParams::new(event.request_id.clone()))
                .await
                .map_err(error)?;
        };
        request = Some(response.request_id.clone());
        if response.request.url != url
            || response.frame_id != frame
            || response.request.method != "GET"
            || response.redirected_request_id.is_some()
            || response.response_error_reason.is_some()
            || response.response_status_code != Some(200)
        {
            return Err(error("response"));
        }
        for h in response.response_headers.as_deref().unwrap_or_default() {
            if h.name.eq_ignore_ascii_case("location") {
                return Err(error("redirect"));
            }
            if h.name.eq_ignore_ascii_case("content-length")
                && h.value
                    .parse::<usize>()
                    .map_or(true, |n| n > MAX_AVATAR_BYTES)
            {
                return Err(error("size"));
            }
        }
        let handle = page
            .execute(fetch::TakeResponseBodyAsStreamParams::new(
                response.request_id.clone(),
            ))
            .await
            .map_err(error)?
            .result
            .stream;
        stream = Some(handle.clone());
        let mut bytes = Vec::new();
        loop {
            let remaining = MAX_AVATAR_BYTES + 1 - bytes.len();
            let mut read = io::ReadParams::new(handle.clone());
            read.size = Some(remaining.min(32 * 1024) as i64);
            let part = page.execute(read).await.map_err(error)?.result;
            if part.data.len() > (32_usize * 1024).div_ceil(3) * 4 {
                return Err(error("chunk"));
            }
            let chunk = if part.base64_encoded == Some(true) {
                base64::engine::general_purpose::STANDARD
                    .decode(&part.data)
                    .map_err(error)?
            } else {
                part.data.into_bytes()
            };
            if bytes.len().saturating_add(chunk.len()) > MAX_AVATAR_BYTES {
                return Err(error("size"));
            }
            bytes.extend_from_slice(&chunk);
            if part.eof {
                break;
            }
        }
        if bytes.is_empty() {
            return Err(error("empty"));
        }
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    };
    let result = tokio::select! { biased;
        _ = cancel.cancelled() => Err(error("cancelled")),
        result = tokio::time::timeout(WAIT, work) => result.map_err(error).and_then(|r| r),
    };
    // The original fetch always has redirect:error; lost cleanup can never cause a redirect GET.
    // Abort only our isolated-world controller, never navigation or page-authored requests.
    let cleanup_wait = Duration::from_millis(250);
    if let Some(ctx) = context {
        let _ = tokio::time::timeout(
            cleanup_wait,
            page.execute(eval(
                format!("globalThis[{key}]?.abort(); delete globalThis[{key}];"),
                ctx,
            )),
        )
        .await;
    }
    if let Some(handle) = stream {
        let _ =
            tokio::time::timeout(cleanup_wait, page.execute(io::CloseParams::new(handle))).await;
    }
    if let Some(id) = request {
        let _ = tokio::time::timeout(
            cleanup_wait,
            page.execute(fetch::FailRequestParams::new(
                id,
                network::ErrorReason::Aborted,
            )),
        )
        .await;
    }
    // Always attempt disable, even if abort/close lost their replies. Never claim
    // a successful read if the interceptor's cleanup could not be confirmed.
    if enabled
        && !matches!(
            tokio::time::timeout(cleanup_wait, page.execute(fetch::DisableParams::default())).await,
            Ok(Ok(_))
        )
    {
        return Err(error("cleanup"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_pattern_and_strict_allowlist() {
        assert_eq!(
            pattern("https://x.yximgs.com/a?x=*"),
            "https://x.yximgs.com/a\\?x=\\*"
        );
        assert!(trusted_avatar_url("https://x.yximgs.com/a.png"));
        for url in [
            "http://yximgs.com/a",
            "https://yximgs.com.evil/a",
            "https://u@yximgs.com/a",
            "https://yximgs.com:444/a",
            "https://yximgs.com/a.svg",
            "https://yximgs.com/a#x",
        ] {
            assert!(!trusted_avatar_url(url));
        }
    }
}
