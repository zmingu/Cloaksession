//! jieger live-room comment listener singleton.
//!
//! One background task per listening account (`account_id`, normally the
//! profile id whose browser session shows the live room). Each task:
//!
//! 1. Registers the `__ksLiveOnEvent__` bridge via `Runtime.addBinding`
//!    (page JS calls `window.__ksLiveOnEvent__(JSON.stringify(event))`).
//! 2. Injects a `MutationObserver` script with `TaskPage::evaluate` that
//!    watches the comment list and forwards new nodes through the bridge.
//! 3. Re-injects after every `Page.frameNavigated` (navigation wipes the
//!    injected observer; the binding itself persists on the target).
//! 4. Falls back to periodic DOM polling (`FALLBACK_INTERVAL_MS`) when the
//!    observer cannot be installed, diffing against a bounded `seen` set.
//!
//! Fresh events fan out three ways: an in-memory recent ring (for IPC
//! reads), a `tokio::sync::broadcast` channel (for auto-reply consumers),
//! and the `comment-event` frontend event, plus durable persistence into the
//! independent `live_events` table (never the danmaku-pipeline store, never
//! the send path — out of scope for this module).
//!
//! The module is declared from `crate` root via `#[path]` so
//! `crates/tauri-app/src/driver.rs` stays untouched.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use cdp_driver::{session::BrowserSession, TaskCancel};
use chromiumoxide::cdp::browser_protocol::page::EventFrameNavigated;
use chromiumoxide::cdp::browser_protocol::target::TargetId;
use chromiumoxide::cdp::js_protocol::runtime::{AddBindingParams, EventBindingCalled};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

/// Bridge name registered with `Runtime.addBinding` and called from page JS
/// as `window.__ksLiveOnEvent__(JSON.stringify(event))`.
pub const BINDING_NAME: &str = "__ksLiveOnEvent__";
/// Frontend push event emitted for every fresh comment event.
pub const COMMENT_EVENT_NAME: &str = "comment-event";
/// Polling period (ms) for the degraded DOM-scrape path.
pub const FALLBACK_INTERVAL_MS: u64 = 2000;
/// Broadcast channel capacity for auto-reply consumers.
pub const BROADCAST_CAPACITY: usize = 512;
/// Per-account in-memory recent ring capacity.
pub const RECENT_CAPACITY: usize = 500;
/// Upper bound for the dedupe `seen` set before oldest ids are evicted.
pub const SEEN_CAPACITY: usize = 5000;

// ---------------------------------------------------------------------------
// Event contract
// ---------------------------------------------------------------------------

/// Live-room event kinds observed by the listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CommentEventType {
    Comment,
    RoomEnter,
    RoomLike,
    RoomFollow,
    LiveOrder,
    LivePaid,
}

impl<'de> Deserialize<'de> for CommentEventType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw))
    }
}

impl CommentEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "Comment",
            Self::RoomEnter => "RoomEnter",
            Self::RoomLike => "RoomLike",
            Self::RoomFollow => "RoomFollow",
            Self::LiveOrder => "LiveOrder",
            Self::LivePaid => "LivePaid",
        }
    }

    /// Lenient parse: unknown type strings degrade to `Comment` so a
    /// comment is never dropped because of a label mismatch.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "Comment" => Self::Comment,
            "RoomEnter" => Self::RoomEnter,
            "RoomLike" => Self::RoomLike,
            "RoomFollow" => Self::RoomFollow,
            "LiveOrder" => Self::LiveOrder,
            "LivePaid" => Self::LivePaid,
            _ => Self::Comment,
        }
    }
}

/// One live-room event.
///
/// Core field names/types mirror
/// `tools/danmaku-pipeline/schema/danmaku-script.schema.json#/$defs/event`
/// (`id: string`, `type: string`, `send_at: number`, `text: string`) so the
/// listener and the pipeline share one contract without sharing code.
/// `send_at` is the authoritative send-time base, expressed as Unix epoch
/// seconds for live capture (the replay/send side maps it onto its own
/// timeline). Extra live-only fields (`user_id`, `nickname`, `account_id`,
/// `time`) ride along and are also persisted to `live_events`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: CommentEventType,
    pub send_at: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(default)]
    pub account_id: String,
    /// Wall-clock observation time (RFC 3339).
    #[serde(default)]
    pub time: String,
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

fn now_epoch_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn str_field(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| obj.get(*k))
        .filter_map(|v| v.as_str())
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Normalize one raw JSON object into a `CommentEvent`. Returns `None` for
/// non-objects. Missing `id`s are synthesized deterministically from
/// `(account_id, user, text)` so polling rounds dedupe instead of
/// re-emitting; missing `text` falls back to the event-type label so the
/// schema-aligned `text` field is never empty.
pub fn normalize_raw_event(
    value: &serde_json::Value,
    account_id: &str,
) -> Option<CommentEvent> {
    let obj = value.as_object()?;
    let event_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .map(CommentEventType::parse)
        .unwrap_or(CommentEventType::Comment);
    let mut id = str_field(obj, &["id", "msg_id"]);
    let user = str_field(obj, &["user_id", "userId", "user"]);
    let nickname = str_field(obj, &["nickname", "nick", "name"]);
    let mut text = str_field(obj, &["text", "content"]);
    if text.is_empty() {
        text = event_type.as_str().to_string();
    }
    if id.is_empty() {
        id = format!("{account_id}:{user}:{nickname}:{text}");
    }
    let send_at = obj
        .get("send_at")
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|n| n as f64)))
        .unwrap_or_else(now_epoch_secs);
    let time = obj
        .get("time")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(now_rfc3339);
    let account = obj
        .get("account_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| account_id.to_string());
    Some(CommentEvent {
        id,
        event_type,
        send_at,
        text,
        user_id: if user.is_empty() { None } else { Some(user) },
        nickname: if nickname.is_empty() {
            None
        } else {
            Some(nickname)
        },
        account_id: account,
        time,
    })
}

/// Parse a bridge payload: either one event object or an array of them.
pub fn parse_payload_events(
    payload: &str,
    account_id: &str,
) -> Vec<CommentEvent> {
    let value: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| normalize_raw_event(v, account_id))
            .collect(),
        single => normalize_raw_event(&single, account_id)
            .map(|e| vec![e])
            .unwrap_or_default(),
    }
}

/// Bounded dedupe set: remembers up to `cap` ids, evicting oldest first.
pub struct SeenSet {
    set: HashSet<String>,
    order: VecDeque<String>,
    cap: usize,
}

impl SeenSet {
    pub fn new(cap: usize) -> Self {
        Self {
            set: HashSet::new(),
            order: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    /// Returns `true` when `id` is fresh (and records it).
    pub fn insert(&mut self, id: &str) -> bool {
        if !self.set.insert(id.to_string()) {
            return false;
        }
        self.order.push_back(id.to_string());
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }
}

/// Mock-friendly `Runtime.bindingCalled` dispatch: ignore foreign binding
/// names, parse the payload, and drop ids already in `seen`.
pub fn handle_binding_event(
    seen: &mut SeenSet,
    name: &str,
    payload: &str,
    account_id: &str,
) -> Vec<CommentEvent> {
    if name != BINDING_NAME {
        return Vec::new();
    }
    parse_payload_events(payload, account_id)
        .into_iter()
        .filter(|e| seen.insert(&e.id))
        .collect()
}

// ---------------------------------------------------------------------------
// Injected page scripts
// ---------------------------------------------------------------------------

/// Installs a `MutationObserver` on the first matching comment list.
/// Returns `{ok:true}` or `{ok:false, reason}` (the Rust side falls back to
/// polling on any non-ok result). Idempotent per document: a previous
/// observer is disconnected first.
pub const INJECT_SCRIPT: &str = r###"(() => {
  const LIST_SEL = ["comment-list", "CommentList", "chat-list", "ChatList", "danmaku", "Danmaku", "message-list", "comments"].map(s => '[class*="' + s + '"]').concat(['#comments', '.comments', '[id*="comment"]']).join(",");
  const TEXT_SEL = ['[class*="content"]', '[class*="text"]', '[class*="Text"]', '[class*="msg"]', '[class*="Msg"]', 'span', 'p'].join(",");
  const USER_SEL = ['[class*="user"]', '[class*="name"]', '[class*="Name"]', '[class*="nick"]', '[class*="Nick"]'].join(",");
  try {
    if (window.__ksLiveObserverInstalled && window.__ksLiveObserverInstalled.disconnect) {
      try { window.__ksLiveObserverInstalled.disconnect(); } catch (_) {}
    }
    window.__ksLiveObserverInstalled = null;
    if (typeof window.__ksLiveOnEvent__ !== "function") return { ok: false, reason: "no-bridge" };
    const list = document.querySelector(LIST_SEL);
    if (!list) return { ok: false, reason: "no-comment-list" };
    const pick = (node, sel) => {
      try {
        const el = node.querySelector ? node.querySelector(sel) : null;
        const t = el && el.innerText ? el.innerText.trim() : "";
        if (t) return t.slice(0, 200);
      } catch (_) {}
      return "";
    };
    const emit = (node) => {
      let text = pick(node, TEXT_SEL);
      if (!text && node.innerText) text = node.innerText.trim().slice(0, 200);
      if (!text) return;
      const user = pick(node, USER_SEL);
      let id = (node.getAttribute && (node.getAttribute("data-id") || node.getAttribute("data-msg-id") || node.id)) || "";
      if (!id) id = "c_" + Date.now() + "_" + Math.floor(Math.random() * 1e9);
      try { window.__ksLiveOnEvent__(JSON.stringify({ id: id, type: "Comment", send_at: Date.now() / 1000, text: text, user_id: user, nickname: user })); } catch (_) {}
    };
    const mo = new MutationObserver((mutations) => {
      for (const m of mutations) {
        for (const node of m.addedNodes) {
          if (node.nodeType !== 1) continue;
          emit(node);
        }
      }
    });
    mo.observe(list, { childList: true, subtree: true });
    window.__ksLiveObserverInstalled = mo;
    return { ok: true };
  } catch (e) {
    return { ok: false, reason: String((e && e.message) || e) };
  }
})()"###;

/// Degraded path: scrape up to 60 candidate comment nodes. The Rust side
/// dedupes against `seen`, so re-scraped nodes are cheap.
pub const EXTRACT_SCRIPT: &str = r###"(() => {
  const LIST_SEL = ["comment-list", "CommentList", "chat-list", "ChatList", "danmaku", "Danmaku", "message-list", "comments"].map(s => '[class*="' + s + '"]').concat(['#comments', '.comments', '[id*="comment"]']).join(",");
  const TEXT_SEL = ['[class*="content"]', '[class*="text"]', '[class*="Text"]', '[class*="msg"]', '[class*="Msg"]', 'span', 'p'].join(",");
  const USER_SEL = ['[class*="user"]', '[class*="name"]', '[class*="Name"]', '[class*="nick"]', '[class*="Nick"]'].join(",");
  const out = [];
  try {
    const lists = Array.from(document.querySelectorAll(LIST_SEL)).slice(0, 3);
    for (const list of lists) {
      const kids = Array.from(list.children).slice(-60);
      for (const node of kids) {
        let text = "";
        try {
          const el = node.querySelector(TEXT_SEL);
          text = (el && el.innerText ? el.innerText.trim() : "") || (node.innerText || "").trim();
        } catch (_) {}
        text = text.slice(0, 200);
        if (!text) continue;
        let user = "";
        try {
          const u = node.querySelector(USER_SEL);
          user = (u && u.innerText ? u.innerText.trim() : "").slice(0, 80);
        } catch (_) {}
        const id = (node.getAttribute && (node.getAttribute("data-id") || node.getAttribute("data-msg-id") || node.id)) || "";
        out.push({ id: id, type: "Comment", text: text, user_id: user, nickname: user });
        if (out.length >= 60) return out;
      }
    }
  } catch (_) {}
  return out;
})()"###;

// ---------------------------------------------------------------------------
// Listener runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerMode {
    Starting,
    Observer,
    Polling,
}

impl ListenerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Observer => "observer",
            Self::Polling => "polling",
        }
    }
}

/// Snapshot for IPC / status queries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStatus {
    pub account_id: String,
    pub running: bool,
    pub mode: String,
    pub target_id: String,
    pub seen_count: usize,
    pub recent_count: usize,
    pub started_at: String,
    pub last_event_at: Option<String>,
    pub last_error: Option<String>,
}

struct ListenerState {
    account_id: String,
    target_id: String,
    db_path: PathBuf,
    cancel: TaskCancel,
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    running: std::sync::atomic::AtomicBool,
    mode: Mutex<ListenerMode>,
    seen: Mutex<SeenSet>,
    recent: Mutex<VecDeque<CommentEvent>>,
    started_at: String,
    last_event_at: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
}

impl ListenerState {
    fn status(&self) -> ListenerStatus {
        ListenerStatus {
            account_id: self.account_id.clone(),
            running: self.running.load(std::sync::atomic::Ordering::SeqCst),
            mode: self.mode.lock().map(|m| m.as_str()).unwrap_or("starting").to_string(),
            target_id: self.target_id.clone(),
            seen_count: self.seen.lock().map(|s| s.len()).unwrap_or(0),
            recent_count: self.recent.lock().map(|r| r.len()).unwrap_or(0),
            started_at: self.started_at.clone(),
            last_event_at: self.last_event_at.lock().ok().and_then(|g| g.clone()),
            last_error: self.last_error.lock().ok().and_then(|g| g.clone()),
        }
    }
}

/// Per-account singleton registry plus the shared broadcast channel.
pub struct CommentListener {
    states: Mutex<HashMap<String, Arc<ListenerState>>>,
    tx: broadcast::Sender<CommentEvent>,
    app: Mutex<Option<AppHandle>>,
}

impl CommentListener {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            states: Mutex::new(HashMap::new()),
            tx,
            app: Mutex::new(None),
        }
    }

    /// Subscribe to the internal fan-out channel (for auto-reply consumers).
    pub fn subscribe(&self) -> broadcast::Receiver<CommentEvent> {
        self.tx.subscribe()
    }

    /// Start (or reattach to) the singleton listener for `account_id`.
    /// A running entry is returned as-is; a stopped entry is replaced.
    pub fn start(
        self: &'static Self,
        account_id: &str,
        session: Arc<BrowserSession>,
        target_id: String,
        db_path: PathBuf,
        app: Option<AppHandle>,
    ) -> Result<ListenerStatus, String> {
        if let Some(state) = self.states.lock().unwrap().get(account_id) {
            if state.running.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(state.status());
            }
        }
        if let Some(handle) = app {
            *self.app.lock().unwrap() = Some(handle);
        }
        let state = Arc::new(ListenerState {
            account_id: account_id.to_string(),
            target_id: target_id.clone(),
            db_path,
            cancel: TaskCancel::new(),
            handle: Mutex::new(None),
            running: std::sync::atomic::AtomicBool::new(true),
            mode: Mutex::new(ListenerMode::Starting),
            seen: Mutex::new(SeenSet::new(SEEN_CAPACITY)),
            recent: Mutex::new(VecDeque::new()),
            started_at: now_rfc3339(),
            last_event_at: Mutex::new(None),
            last_error: Mutex::new(None),
        });
        let task_state = state.clone();
        let task_account = account_id.to_string();
        let handle = tokio::spawn(run_loop(self, task_account, session, target_id, task_state));
        *state.handle.lock().unwrap() = Some(handle);
        self.states
            .lock()
            .unwrap()
            .insert(account_id.to_string(), state.clone());
        Ok(state.status())
    }

    /// Stop the listener for `account_id`. Returns `true` when a running
    /// listener was stopped; the (stopped) entry stays queryable via
    /// `status`.
    pub fn stop(&self, account_id: &str) -> bool {
        let state = match self.states.lock().unwrap().get(account_id).cloned() {
            Some(s) => s,
            None => return false,
        };
        if !state
            .running
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return false;
        }
        state.cancel.cancel();
        if let Some(handle) = state.handle.lock().unwrap().take() {
            handle.abort();
        }
        true
    }

    pub fn status(&self, account_id: &str) -> Option<ListenerStatus> {
        self.states
            .lock()
            .unwrap()
            .get(account_id)
            .map(|s| s.status())
    }

    pub fn status_all(&self) -> Vec<ListenerStatus> {
        self.states
            .lock()
            .unwrap()
            .values()
            .map(|s| s.status())
            .collect()
    }

    /// Newest-first recent events (in-memory ring) for `account_id`.
    pub fn recent(&self, account_id: &str, limit: usize) -> Vec<CommentEvent> {
        self.states
            .lock()
            .unwrap()
            .get(account_id)
            .map(|s| {
                s.recent
                    .lock()
                    .map(|r| r.iter().rev().take(limit).cloned().collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    fn set_mode(&self, account_id: &str, mode: ListenerMode) {
        if let Some(state) = self.states.lock().unwrap().get(account_id) {
            *state.mode.lock().unwrap() = mode;
        }
    }

    fn set_error(&self, account_id: &str, error: String) {
        if let Some(state) = self.states.lock().unwrap().get(account_id) {
            *state.last_error.lock().unwrap() = Some(error);
        }
    }

    fn mark_stopped(&self, account_id: &str) {
        if let Some(state) = self.states.lock().unwrap().get(account_id) {
            state
                .running
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Dedupe + record payload events for `account_id`.
    fn ingest_values(
        &self,
        account_id: &str,
        events: Vec<CommentEvent>,
    ) -> Vec<CommentEvent> {
        let state = match self.states.lock().unwrap().get(account_id).cloned() {
            Some(s) => s,
            None => return Vec::new(),
        };
        let mut seen = state.seen.lock().unwrap();
        let fresh: Vec<CommentEvent> = events
            .into_iter()
            .filter(|e| seen.insert(&e.id))
            .collect();
        if let Some(last) = fresh.last() {
            *state.last_event_at.lock().unwrap() = Some(last.time.clone());
        }
        fresh
    }

    /// Fan out one fresh event: recent ring, broadcast, frontend emit,
    /// durable `live_events` row. Persistence failures are logged and
    /// swallowed — delivery must not break on a locked DB.
    fn publish(&self, account_id: &str, event: CommentEvent) {
        let state = match self.states.lock().unwrap().get(account_id).cloned() {
            Some(s) => s,
            None => return,
        };
        {
            let mut recent = state.recent.lock().unwrap();
            recent.push_back(event.clone());
            while recent.len() > RECENT_CAPACITY {
                recent.pop_front();
            }
        }
        let _ = self.tx.send(event.clone());
        if let Some(app) = self.app.lock().unwrap().as_ref().cloned() {
            if let Err(e) = app.emit(COMMENT_EVENT_NAME, &event) {
                tracing::warn!(error = %e, "emit comment-event failed");
            }
        }
        let row = profile_manager::live_events::LiveEventRow {
            msg_id: event.id.clone(),
            event_type: event.event_type.as_str().to_string(),
            user_id: event.user_id.clone(),
            nickname: event.nickname.clone(),
            content: event.text.clone(),
            time: event.time.clone(),
            account_id: event.account_id.clone(),
        };
        if let Err(e) = profile_manager::live_events::append_event(&state.db_path, &row) {
            tracing::warn!(error = %e, "persist live event failed");
        }
    }
}

static GLOBAL: LazyLock<CommentListener> = LazyLock::new(CommentListener::new);

/// Process-wide listener singleton.
pub fn global_listener() -> &'static CommentListener {
    &GLOBAL
}

/// Install the `MutationObserver` on `target_id` via a cooperative
/// `TaskPage::evaluate`. Returns `Ok(true)` when the page reports
/// `{ok:true}`; any other outcome means the caller should use polling.
async fn inject_observer(
    session: &Arc<BrowserSession>,
    target_id: &str,
    cancel: &TaskCancel,
) -> Result<bool, String> {
    let mut task = session
        .task_page(target_id, cancel.clone(), Duration::from_secs(5))
        .await
        .map_err(|e| format!("task page: {e}"))?;
    let value = task
        .evaluate(INJECT_SCRIPT, Duration::from_secs(10))
        .await
        .map_err(|e| format!("inject evaluate: {e}"))?;
    Ok(value
        .get("ok")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

/// One degraded polling round: scrape candidate comment nodes and
/// normalize them (dedupe happens in `ingest_values`).
async fn poll_once(
    session: &Arc<BrowserSession>,
    target_id: &str,
    cancel: &TaskCancel,
    account_id: &str,
) -> Result<Vec<CommentEvent>, String> {
    let mut task = session
        .task_page(target_id, cancel.clone(), Duration::from_secs(5))
        .await
        .map_err(|e| format!("task page: {e}"))?;
    let value = task
        .evaluate(EXTRACT_SCRIPT, Duration::from_secs(10))
        .await
        .map_err(|e| format!("poll evaluate: {e}"))?;
    let items = value.as_array().cloned().unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|v| normalize_raw_event(v, account_id))
        .collect())
}

async fn run_loop(
    listener: &'static CommentListener,
    account_id: String,
    session: Arc<BrowserSession>,
    target_id: String,
    state: Arc<ListenerState>,
) {
    let result = run_loop_inner(listener, &account_id, &session, &target_id, &state).await;
    if let Err(e) = result {
        tracing::warn!(account = %account_id, error = %e, "comment listener exited");
        listener.set_error(&account_id, e);
    }
    listener.mark_stopped(&account_id);
}

async fn run_loop_inner(
    listener: &'static CommentListener,
    account_id: &str,
    session: &Arc<BrowserSession>,
    target_id: &str,
    state: &Arc<ListenerState>,
) -> Result<(), String> {
    let page = session
        .browser
        .get_page(TargetId::new(target_id))
        .await
        .map_err(|e| format!("bind live page: {e}"))?;
    // Register the JS→Rust bridge. A repeated registration (e.g. after a
    // previous run on the same target) errors; the bridge persists, so any
    // outcome lets us continue.
    if let Err(e) = page.execute(AddBindingParams::new(BINDING_NAME)).await {
        tracing::debug!(error = %e, "addBinding (may pre-exist)");
    }
    match inject_observer(session, target_id, &state.cancel).await {
        Ok(true) => listener.set_mode(account_id, ListenerMode::Observer),
        Ok(false) => {
            tracing::warn!("observer inject reported not-ok; using polling fallback");
            listener.set_mode(account_id, ListenerMode::Polling);
        }
        Err(e) => {
            tracing::warn!(error = %e, "observer inject failed; using polling fallback");
            listener.set_mode(account_id, ListenerMode::Polling);
        }
    }
    let mut bindings = page
        .event_listener::<EventBindingCalled>()
        .await
        .map_err(|e| format!("binding listener: {e}"))?;
    let mut navs = page
        .event_listener::<EventFrameNavigated>()
        .await
        .map_err(|e| format!("navigation listener: {e}"))?;
    let mut poll = tokio::time::interval(Duration::from_millis(FALLBACK_INTERVAL_MS));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the immediate first tick: observer mode must not scrape on start.
    poll.tick().await;

    loop {
        tokio::select! {
            biased;
            _ = state.cancel.cancelled() => break,
            event = bindings.next() => {
                let Some(event) = event else { break };
                if event.name != BINDING_NAME {
                    continue;
                }
                let fresh = {
                    let parsed = parse_payload_events(&event.payload, account_id);
                    listener.ingest_values(account_id, parsed)
                };
                for e in fresh {
                    listener.publish(account_id, e);
                }
            }
            nav = navs.next() => {
                if nav.is_none() {
                    break;
                }
                // Navigation wipes the injected observer (the binding
                // persists on the target): re-inject. On failure the
                // run degrades to polling from here on.
                match inject_observer(session, target_id, &state.cancel).await {
                    Ok(true) => listener.set_mode(account_id, ListenerMode::Observer),
                    _ => listener.set_mode(account_id, ListenerMode::Polling),
                }
            }
            _ = poll.tick() => {
                let polling = state.mode.lock().map(|m| *m == ListenerMode::Polling).unwrap_or(false);
                if !polling {
                    continue;
                }
                match poll_once(session, target_id, &state.cancel, account_id).await {
                    Ok(events) => {
                        for e in listener.ingest_values(account_id, events) {
                            listener.publish(account_id, e);
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "comment poll round failed");
                    }
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (no browser required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn comment_event_serde_aligns_with_danmaku_script_schema() {
        // Core names/types mirror
        // tools/danmaku-pipeline/schema/danmaku-script.schema.json#/$defs/event:
        // id (string), type (string), send_at (number, authoritative), text.
        let evt = CommentEvent {
            id: "evt-0001".into(),
            event_type: CommentEventType::Comment,
            send_at: 12.5,
            text: "好看".into(),
            user_id: Some("u1".into()),
            nickname: Some("nick".into()),
            account_id: "a".into(),
            time: "2026-10-03T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&evt).unwrap();
        assert_eq!(v["id"], json!("evt-0001"));
        assert_eq!(v["type"], json!("Comment"));
        assert_eq!(v["text"], json!("好看"));
        assert!(v["send_at"].is_number());
        assert!((v["send_at"].as_f64().unwrap() - 12.5).abs() < f64::EPSILON);

        // Schema-shaped JSON (live extras alongside) deserializes; every
        // declared live kind parses under the "type" key.
        for (raw, expected) in [
            ("Comment", CommentEventType::Comment),
            ("RoomEnter", CommentEventType::RoomEnter),
            ("RoomLike", CommentEventType::RoomLike),
            ("RoomFollow", CommentEventType::RoomFollow),
            ("LiveOrder", CommentEventType::LiveOrder),
            ("LivePaid", CommentEventType::LivePaid),
        ] {
            let back: CommentEvent = serde_json::from_value(json!({
                "id": "evt-0002", "type": raw, "send_at": 3.0,
                "text": raw, "account_id": "a", "time": "2026-10-03T00:00:00Z",
            }))
            .unwrap();
            assert_eq!(back.event_type, expected);
        }

        // Unknown type labels degrade to Comment rather than failing.
        let back: CommentEvent = serde_json::from_value(json!({
            "id": "evt-0003", "type": "read", "send_at": 1.0, "text": "x",
        }))
        .unwrap();
        assert_eq!(back.event_type, CommentEventType::Comment);
    }

    #[test]
    fn seen_set_dedupes_repeated_payloads() {
        let mut seen = SeenSet::new(8);
        let payload = r#"{"id":"m1","type":"Comment","send_at":10.0,"text":"hi","user_id":"u"}"#;
        let first = handle_binding_event(&mut seen, BINDING_NAME, payload, "a");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "m1");
        // Exact redelivery: swallowed.
        assert!(handle_binding_event(&mut seen, BINDING_NAME, payload, "a").is_empty());
        // Array with one fresh + one duplicate: only the fresh survives.
        let batch = r#"[{"id":"m1","type":"Comment","send_at":10.0,"text":"hi"},{"id":"m2","type":"RoomLike","send_at":11.0,"text":"RoomLike"}]"#;
        let fresh = handle_binding_event(&mut seen, BINDING_NAME, batch, "a");
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].id, "m2");
        assert_eq!(fresh[0].event_type, CommentEventType::RoomLike);
        // Bounded eviction: capacity 8 was not exceeded here.
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn mock_binding_called_dispatch_filters_and_normalizes() {
        // Simulates what run_loop does with a real
        // `Runtime.bindingCalled` event (name + string payload): foreign
        // binding names are ignored, malformed payloads yield nothing, and
        // id-less polling-style objects get deterministic content keys.
        let mut seen = SeenSet::new(64);
        assert!(handle_binding_event(&mut seen, "otherBinding", "{}", "a").is_empty());
        assert!(handle_binding_event(&mut seen, BINDING_NAME, "not-json", "a").is_empty());
        assert!(handle_binding_event(&mut seen, BINDING_NAME, "[1,2]", "a").is_empty());

        let payload = r#"{"type":"Comment","text":"hello","user_id":"u7"}"#;
        let fresh = handle_binding_event(&mut seen, BINDING_NAME, payload, "acct");
        assert_eq!(fresh.len(), 1);
        let evt = &fresh[0];
        assert_eq!(evt.id, "acct:u7::hello");
        assert_eq!(evt.account_id, "acct");
        assert_eq!(evt.user_id.as_deref(), Some("u7"));
        // Same content re-polled: deduped via the deterministic key.
        assert!(handle_binding_event(&mut seen, BINDING_NAME, payload, "acct").is_empty());

        // Empty text falls back to the type label (schema text is non-empty).
        let enter = handle_binding_event(
            &mut seen,
            BINDING_NAME,
            r#"{"id":"e1","type":"RoomEnter","send_at":5.0}"#,
            "acct",
        );
        assert_eq!(enter.len(), 1);
        assert_eq!(enter[0].text, "RoomEnter");
    }

    #[test]
    fn seen_set_evicts_oldest_beyond_capacity() {
        let mut seen = SeenSet::new(2);
        assert!(seen.insert("a"));
        assert!(seen.insert("b"));
        assert!(seen.insert("c")); // evicts "a"
        assert_eq!(seen.len(), 2);
        assert!(seen.insert("a")); // "a" reads as fresh again
    }
}
