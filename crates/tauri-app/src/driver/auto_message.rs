//! jieger 主播互动（时间轴弹幕）的 Rust 移植。
//!
//! Port of `electron/main/tasks/autoMessage/{index.ts, variables.ts}`
//! (jieger @ `8a8d8a2`). The original runs one timeline of danmaku lines
//! per sub-account: each line fires at `started_at + offset_sec` with
//! variable interpolation applied at dispatch time, optionally with random
//! spaces inserted to dodge duplicate-content detection.
//!
//! # Adaptations from the Electron original
//!
//! - `setTimeout` per line becomes one `tokio::spawn` per line guarded by
//!   `tokio::time::sleep_until` on an **absolute** deadline (no cumulative
//!   `sleep` drift). `tokio::time::Instant` has no `from_millis`
//!   constructor (epoch millis are not monotonic), so the epoch-millis
//!   `trigger_at` is mapped onto a monotonic deadline anchored at the
//!   run's start instant (see [`deadline_for`]).
//! - Timers are cancelled through a per-run [`TaskCancel`](cdp_driver::TaskCancel):
//!   each line task `select!`s (biased, cancel first) between cancellation
//!   and its deadline, so `stop` suppresses pending sends immediately.
//! - Runs are keyed by a generated `run_id` (uuid), not by account id: a
//!   single [`MessageLine`] carries its own `account_id`, so one timeline
//!   may fan out across several sub-accounts. Starting twice therefore
//!   creates two independent runs (the original stopped the previous task
//!   for the same account on restart).
//! - Page interaction goes through the [`send_danmaku`] seam, which only
//!   uses high-level [`BrowserSession`](cdp_driver::session::BrowserSession)
//!   methods (`click` / `type_text`). Scheduling code never acquires a
//!   `TaskPage` lease directly.
//! - Running runs live in a module-level map (mirroring the original's
//!   module-level `runningTasks`), because `TauriBrowserDriver`'s fields
//!   are owned by `driver.rs` and this module only adds behavior.
//! - The original's `pinned` (置顶) message is intentionally not ported:
//!   jieger itself marks it placeholder (`TODO: 真实快手中控台的置顶交互流程需实地测绘`).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cdp_driver::session::BrowserSession;
use cdp_driver::TaskCancel;
use multizen_core::{MultizenError, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

/// Probability of inserting a space between two characters.
/// Matches the original `insertRandomSpaces` default (`0.15`).
pub const DEFAULT_RANDOM_SPACE_PROBABILITY: f64 = 0.15;

/// Upper bound for a line offset (24h). Live-stream timelines never exceed
/// hours; the cap keeps `Instant + Duration` arithmetic safely away from
/// overflow. The original only rejected negative/non-finite offsets.
pub const MAX_OFFSET_SEC: f64 = 86_400.0;

/// Frontend push event carrying an [`AutoMessageState`] snapshot.
/// Colon-style event name per the `profiles:running-changed` convention.
pub const AUTO_MESSAGE_PROGRESS_EVENT: &str = "auto-message:progress";
/// Frontend push event emitted when a run stops (`{ run_id, reason }`).
pub const AUTO_MESSAGE_STOPPED_EVENT: &str = "auto-message:stopped";

/// Comment input selector for the Kuaishou control room.
///
/// Reduced to CSS-compatible hooks: the original Playwright selectors also
/// contained `button:has-text("…")` / `textarea[placeholder*=…]` forms,
/// which are not valid CSS for the CDP path, so only the `data-e2e` /
/// placeholder-attribute hooks are kept here.
pub const COMMENT_INPUT_SELECTOR: &str =
    "textarea[placeholder*=\"说点什么\"], [data-e2e=\"comment-input\"]";
/// Send-button selector for the Kuaishou control room (CSS-compatible
/// subset of the original Playwright selector).
pub const COMMENT_SEND_SELECTOR: &str = "[data-e2e=\"comment-send\"]";

/// Variable interpolation context. Mirrors `VariableContext` in
/// jieger `variables.ts`: `nickname` is only populated in comment-reply
/// scenarios (always `None` for timeline dispatch), `anchor` is the
/// current account's display name (empty when unknown).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VariableContext {
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub anchor: Option<String>,
}

/// One timeline entry: send `message` at `offset_sec` seconds after the
/// run starts, using the session bound to `account_id`.
///
/// `account_id` names a running profile session (the Cloaksession analog
/// of jieger's sub-account session key).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageLine {
    /// Seconds after run start (float, like the original `videoTimeSec`).
    pub offset_sec: f64,
    /// Message template; variables are interpolated at dispatch time.
    pub message: String,
    /// Sub-account / profile whose session sends this line.
    pub account_id: String,
}

/// A line with its absolute fire time resolved.
#[derive(Clone, Debug, Serialize)]
pub struct ScheduledLine {
    pub offset_sec: f64,
    /// Epoch millis: `started_at + offset_sec * 1000`.
    pub trigger_at: u64,
    pub message: String,
    pub account_id: String,
}

/// Options for [`TauriBrowserDriver::auto_message_start`].
#[derive(Clone, Debug, Default)]
pub struct AutoMessageOptions {
    /// Insert random spaces between characters (anti-duplication).
    pub insert_random_space: bool,
    /// Override for `{用户名}` / `{nickname}` (default: empty).
    pub nickname: Option<String>,
    /// Override for `{主播名称}` / `{anchor}` (default: empty).
    pub anchor: Option<String>,
    /// Shared start epoch-ms (multi-script alignment); defaults to now.
    pub start_at: Option<u64>,
}

/// Snapshot emitted to the frontend on start and after every dispatch.
#[derive(Clone, Debug, Serialize)]
pub struct AutoMessageState {
    /// Run start, epoch millis (`Date.now()` analog).
    pub started_at: u64,
    pub total_count: usize,
    pub sent_count: usize,
    pub schedule: Vec<ScheduledLine>,
}

/// Return value of the start command.
#[derive(Clone, Debug, Serialize)]
pub struct AutoMessageStarted {
    pub run_id: String,
    pub started_at: u64,
    pub scheduled_count: usize,
}

/// Payload of [`AUTO_MESSAGE_STOPPED_EVENT`].
#[derive(Clone, Debug, Serialize)]
pub struct AutoMessageStopped {
    pub run_id: String,
    pub reason: String,
}

/// A line with its template rendered, ready to send.
#[derive(Clone, Debug)]
pub struct ResolvedLine {
    pub account_id: String,
    pub text: String,
    pub offset_sec: f64,
}

/// Outcome of one send attempt.
#[derive(Clone, Debug)]
pub struct SendOutcome {
    pub ok: bool,
    pub error: Option<String>,
    /// The session for `account_id` is gone; the caller should stop the
    /// whole run (mirrors the original `session_lost` path).
    pub session_lost: bool,
}

impl SendOutcome {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            session_lost: false,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(error.into()),
            session_lost: false,
        }
    }

    pub fn session_lost(account_id: &str) -> Self {
        Self {
            ok: false,
            error: Some(format!("account `{account_id}` has no running session")),
            session_lost: true,
        }
    }
}

/// Injectable send function so the timeline scheduler is unit-testable
/// without a browser: production passes a closure over [`send_danmaku`],
/// tests pass a mock recorder.
pub type SendDanmakuFn = Arc<
    dyn Fn(ResolvedLine) -> Pin<Box<dyn Future<Output = SendOutcome> + Send>> + Send + Sync,
>;

/// Interpolate `template` with `ctx`. Supported variables (jieger parity):
///
/// - `{用户名}` / `{nickname}` → comment-author nickname (empty here)
/// - `{主播名称}` / `{anchor}` → current account display name
/// - `{当前时间}` → local `HH:mm`
/// - `{当前日期}` → local `YYYY-MM-DD`
/// - `{随机数字}` → random integer 1–100
/// - `{随机|A|B|C}` → one random candidate (trimmed, empties dropped)
pub fn interpolate(template: &str, ctx: &VariableContext) -> String {
    interpolate_with_rng(template, ctx, &mut rand::thread_rng())
}

/// [`interpolate`] with an injectable RNG (deterministic under a seed).
pub fn interpolate_with_rng(
    template: &str,
    ctx: &VariableContext,
    rng: &mut impl rand::Rng,
) -> String {
    if template.is_empty() {
        return String::new();
    }
    let mut out = template.to_string();
    out = out.replace("{用户名}", ctx.nickname.as_deref().unwrap_or(""));
    out = out.replace("{nickname}", ctx.nickname.as_deref().unwrap_or(""));
    out = out.replace("{主播名称}", ctx.anchor.as_deref().unwrap_or(""));
    out = out.replace("{anchor}", ctx.anchor.as_deref().unwrap_or(""));
    let now = chrono::Local::now();
    out = out.replace("{当前时间}", &now.format("%H:%M").to_string());
    out = out.replace("{当前日期}", &now.format("%Y-%m-%d").to_string());
    // `str::replace` with `&str` is literal (no `$`-expansion hazards),
    // so a plain value suffices where the original needed a replacer fn.
    while out.contains("{随机数字}") {
        let n = rng.gen_range(1..=100);
        out = out.replacen("{随机数字}", &n.to_string(), 1);
    }
    replace_candidates(&out, rng)
}

/// Expand `{随机|A|B|C}` candidate expressions. Single pass, mirroring the
/// original regex `\{随机\|([^}]+)\}`: the choice list ends at the first
/// `}`, so nested `{…}` inside choices is not re-expanded afterwards.
fn replace_candidates(input: &str, rng: &mut impl rand::Rng) -> String {
    const PREFIX: &str = "{随机|";
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find(PREFIX) {
        out.push_str(&rest[..start]);
        let after = &rest[start + PREFIX.len()..];
        match after.find('}') {
            Some(end) => {
                let choices: Vec<&str> = after[..end]
                    .split('|')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(picked) = (!choices.is_empty())
                    .then(|| choices[rng.gen_range(0..choices.len())])
                {
                    out.push_str(picked);
                }
                rest = &after[end + 1..];
            }
            // Unclosed `{随机|` is kept verbatim (the regex would not match).
            None => {
                out.push_str(&rest[start..]);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Insert random spaces between characters to dodge duplicate-content
/// detection. Iterates over `.chars()` (Unicode scalar values — the Rust
/// analog of the original's `[...message]` code-point spread), so no
/// multi-byte character is ever split. Like the original, no space is
/// appended after the final character.
pub fn insert_random_spaces(message: &str, probability: f64) -> String {
    insert_random_spaces_with_rng(message, probability, &mut rand::thread_rng())
}

/// [`insert_random_spaces`] with an injectable RNG.
pub fn insert_random_spaces_with_rng(
    message: &str,
    probability: f64,
    rng: &mut impl rand::Rng,
) -> String {
    if message.is_empty() || probability <= 0.0 {
        return message.to_string();
    }
    let chars: Vec<char> = message.chars().collect();
    let mut out = String::with_capacity(message.len());
    for (i, c) in chars.iter().enumerate() {
        out.push(*c);
        if i + 1 < chars.len() && rng.gen::<f64>() < probability {
            out.push(' ');
        }
    }
    out
}

/// Validate timeline lines. Mirrors the original `validateConfig`: at
/// least one line, finite non-negative offsets within [`MAX_OFFSET_SEC`],
/// non-blank message per line.
pub fn validate_lines(lines: &[MessageLine]) -> Result<()> {
    if lines.is_empty() {
        return Err(MultizenError::Config("必须提供至少一条时间轴发言".into()));
    }
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        if !line.offset_sec.is_finite() || line.offset_sec < 0.0 {
            return Err(MultizenError::Config(format!("第 {n} 行时间无效")));
        }
        if line.offset_sec > MAX_OFFSET_SEC {
            return Err(MultizenError::Config(format!(
                "第 {n} 行时间超出上限 ({MAX_OFFSET_SEC} 秒)"
            )));
        }
        if line.message.trim().is_empty() {
            return Err(MultizenError::Config(format!("第 {n} 行发言内容不能为空")));
        }
        if line.account_id.trim().is_empty() {
            return Err(MultizenError::Config(format!("第 {n} 行缺少账号")));
        }
    }
    Ok(())
}

/// Sort lines by offset and resolve absolute `trigger_at` epoch-millis.
/// Pure computation — the unit-testable half of timeline planning.
pub fn plan_schedule(started_at_ms: u64, lines: &[MessageLine]) -> Vec<ScheduledLine> {
    let mut sorted: Vec<&MessageLine> = lines.iter().collect();
    sorted.sort_by(|a, b| {
        a.offset_sec
            .partial_cmp(&b.offset_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
        .into_iter()
        .map(|line| {
            let delay_ms = (line.offset_sec * 1000.0).round().max(0.0) as u64;
            ScheduledLine {
                offset_sec: line.offset_sec,
                trigger_at: started_at_ms.saturating_add(delay_ms),
                message: line.message.clone(),
                account_id: line.account_id.clone(),
            }
        })
        .collect()
}

/// Map an epoch-millis `trigger_at` onto a monotonic [`tokio::time::Instant`]
/// deadline anchored at the run's start instant.
///
/// `tokio::time::Instant` (like `std::time::Instant`) has no `from_millis`
/// constructor — epoch time is wall-clock, not monotonic — so the trigger
/// is expressed as an offset from the start anchor. The deadline stays
/// absolute per line (never accumulated `sleep` chains), so long timelines
/// do not drift.
pub fn deadline_for(
    started_mono: tokio::time::Instant,
    started_at_ms: u64,
    trigger_at_ms: u64,
) -> tokio::time::Instant {
    started_mono + Duration::from_millis(trigger_at_ms.saturating_sub(started_at_ms))
}

/// Spawn one timeline task: wait for `deadline` via `sleep_until`, then run
/// `work`. `cancel` wins over a ready deadline (`biased` first branch), so
/// stopping a run suppresses pending sends even when their time has come.
pub fn spawn_line_task(
    deadline: tokio::time::Instant,
    cancel: TaskCancel,
    work: impl Future<Output = ()> + Send + 'static,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {}
            _ = tokio::time::sleep_until(deadline) => {
                work.await;
            }
        }
    })
}

/// Send one danmaku through a live browser session.
///
/// The seam the scheduler calls. Only high-level [`BrowserSession`]
/// methods (`click` / `type_text` on the active page) are used — never a
/// `TaskPage` lease. Port of jieger `sendComment`: focus the comment
/// input, type the message, click send.
pub async fn send_danmaku(session: &BrowserSession, text: &str) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        return Err(MultizenError::Config("消息内容为空".into()));
    }
    session.click(COMMENT_INPUT_SELECTOR).await?;
    session.type_text(COMMENT_INPUT_SELECTOR, text).await?;
    session.click(COMMENT_SEND_SELECTOR).await?;
    Ok(())
}

fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

struct RunHandle {
    cancel: TaskCancel,
    state: Arc<AsyncMutex<AutoMessageState>>,
}

fn runs() -> &'static AsyncMutex<HashMap<String, Arc<RunHandle>>> {
    static RUNS: std::sync::OnceLock<AsyncMutex<HashMap<String, Arc<RunHandle>>>> =
        std::sync::OnceLock::new();
    RUNS.get_or_init(|| AsyncMutex::new(HashMap::new()))
}

async fn finish_if_complete(
    driver: &Arc<super::TauriBrowserDriver>,
    run_id: &str,
    run: &Arc<RunHandle>,
) {
    let snapshot = run.state.lock().await.clone();
    driver.emit(AUTO_MESSAGE_PROGRESS_EVENT, &snapshot);
    if snapshot.sent_count >= snapshot.total_count {
        stop_run_inner(driver, run_id, "finished").await;
    }
}

async fn stop_run_inner(driver: &super::TauriBrowserDriver, run_id: &str, reason: &str) {
    let removed = runs().lock().await.remove(run_id);
    if let Some(run) = removed {
        run.cancel.cancel();
    }
    // Best-effort push event (no-op before `set_app`, e.g. unit tests).
    driver.emit(
        AUTO_MESSAGE_STOPPED_EVENT,
        &AutoMessageStopped {
            run_id: run_id.to_string(),
            reason: reason.to_string(),
        },
    );
    tracing::info!(run_id, reason, "auto-message stopped");
}

impl super::TauriBrowserDriver {
    /// Start a timeline run. Validates, plans absolute triggers, emits the
    /// initial state, then spawns one task per line. Variables are
    /// interpolated at dispatch time (so `{当前时间}` reflects the fire
    /// moment, as in the original `dispatchLine`).
    pub async fn auto_message_start(
        self: &Arc<Self>,
        lines: Vec<MessageLine>,
        options: AutoMessageOptions,
    ) -> Result<AutoMessageStarted> {
        validate_lines(&lines)?;
        let started_at = match options.start_at {
            Some(t) if t > 0 => t,
            _ => now_millis(),
        };
        let started_mono = tokio::time::Instant::now();
        let schedule = plan_schedule(started_at, &lines);
        let total_count = schedule.len();
        let run_id = uuid::Uuid::new_v4().to_string();
        let ctx = VariableContext {
            nickname: options.nickname.filter(|s| !s.trim().is_empty()),
            anchor: options.anchor.filter(|s| !s.trim().is_empty()),
        };
        let state = Arc::new(AsyncMutex::new(AutoMessageState {
            started_at,
            total_count,
            sent_count: 0,
            schedule: schedule.clone(),
        }));
        let run = Arc::new(RunHandle {
            cancel: TaskCancel::new(),
            state: state.clone(),
        });
        runs().lock().await.insert(run_id.clone(), run.clone());
        self.emit(AUTO_MESSAGE_PROGRESS_EVENT, &state.lock().await.clone());
        tracing::info!(
            run_id,
            total_count,
            "auto-message timeline started"
        );

        for line in schedule {
            let driver = self.clone();
            let run_id = run_id.clone();
            let run = run.clone();
            let ctx = ctx.clone();
            let cancel = run.cancel.clone();
            let deadline = deadline_for(started_mono, started_at, line.trigger_at);
            // Production sender: resolve the account's live session at
            // fire time, then send through the `send_danmaku` seam.
            let send_driver = driver.clone();
            let send: SendDanmakuFn = Arc::new(move |resolved: ResolvedLine| {
                let driver = send_driver.clone();
                Box::pin(async move {
                    match driver.require_session(&resolved.account_id).await {
                        Err(_) => SendOutcome::session_lost(&resolved.account_id),
                        Ok(session) => match send_danmaku(&session, &resolved.text).await {
                            Ok(()) => {
                                tracing::info!(
                                    account = %resolved.account_id,
                                    offset_sec = resolved.offset_sec,
                                    "auto-message sent: {:.30}",
                                    resolved.text
                                );
                                SendOutcome::ok()
                            }
                            Err(e) => {
                                tracing::warn!(
                                    account = %resolved.account_id,
                                    offset_sec = resolved.offset_sec,
                                    error = %e,
                                    "auto-message send failed"
                                );
                                SendOutcome::failed(e.to_string())
                            }
                        },
                    }
                })
                    as Pin<Box<dyn Future<Output = SendOutcome> + Send>>
            });
            let work_cancel = cancel.clone();
            let work = async move {
                // Stale task of a stopped/finished run: drop silently.
                if work_cancel.is_cancelled() {
                    return;
                }
                let mut text = interpolate(&line.message, &ctx);
                if options.insert_random_space {
                    text = insert_random_spaces(&text, DEFAULT_RANDOM_SPACE_PROBABILITY);
                }
                let outcome = send(ResolvedLine {
                    account_id: line.account_id.clone(),
                    text,
                    offset_sec: line.offset_sec,
                })
                .await;
                if outcome.session_lost {
                    stop_run_inner(&driver, &run_id, "session_lost").await;
                    return;
                }
                if !outcome.ok {
                    tracing::warn!(
                        account = %line.account_id,
                        offset_sec = line.offset_sec,
                        error = outcome.error.as_deref().unwrap_or("unknown"),
                        "auto-message line failed; counted as sent"
                    );
                }
                {
                    let mut state = run.state.lock().await;
                    state.sent_count += 1;
                }
                finish_if_complete(&driver, &run_id, &run).await;
            };
            spawn_line_task(deadline, cancel, work);
        }

        Ok(AutoMessageStarted {
            run_id,
            started_at,
            scheduled_count: total_count,
        })
    }

    /// Stop a run: pending line tasks are cancelled via [`TaskCancel`]
    /// (their `select!` prefers cancel over a ready deadline) and the run
    /// is removed. Idempotent — stopping an unknown/finished run is `Ok`
    /// (mirrors the original `stop`).
    pub async fn auto_message_stop(&self, run_id: &str) -> Result<()> {
        stop_run_inner(self, run_id, "manual").await;
        Ok(())
    }

    /// Snapshot of a running run. Finished/stopped runs are removed, so
    /// querying them returns `NotFound` (mirrors `isRunning() == false`).
    pub async fn auto_message_status(&self, run_id: &str) -> Result<AutoMessageState> {
        let run = runs().lock().await.get(run_id).cloned();
        match run {
            Some(run) => Ok(run.state.lock().await.clone()),
            None => Err(MultizenError::NotFound(format!(
                "auto-message run `{run_id}` not running"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn seeded() -> rand::rngs::StdRng {
        rand::rngs::StdRng::seed_from_u64(0xA17E_5A9E)
    }

    fn ctx() -> VariableContext {
        VariableContext {
            nickname: Some("小粉".into()),
            anchor: Some("主播桃".into()),
        }
    }

    #[test]
    fn interpolate_nickname_and_anchor() {
        let out = interpolate_with_rng(
            "{用户名}给{主播名称}点了赞，{nickname}来了，{anchor}你好",
            &ctx(),
            &mut seeded(),
        );
        assert_eq!(out, "小粉给主播桃点了赞，小粉来了，主播桃你好");
    }

    #[test]
    fn interpolate_missing_ctx_renders_empty() {
        let out = interpolate_with_rng(
            "嗨{用户名}，{主播名称}欢迎你",
            &VariableContext::default(),
            &mut seeded(),
        );
        assert_eq!(out, "嗨，欢迎你");
    }

    #[test]
    fn interpolate_time_and_date_match_local_clock() {
        let now = chrono::Local::now();
        let out = interpolate_with_rng(
            "{当前时间}|{当前日期}",
            &VariableContext::default(),
            &mut seeded(),
        );
        assert_eq!(
            out,
            format!("{}|{}", now.format("%H:%M"), now.format("%Y-%m-%d"))
        );
    }

    #[test]
    fn interpolate_random_number_stays_in_range() {
        let mut rng = seeded();
        for _ in 0..200 {
            let out = interpolate_with_rng("{随机数字}", &VariableContext::default(), &mut rng);
            let n: u32 = out.parse().expect("must be a number");
            assert!((1..=100).contains(&n), "out of range: {n}");
        }
    }

    #[test]
    fn interpolate_candidate_syntax_picks_member() {
        let mut rng = seeded();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..60 {
            let out = interpolate_with_rng(
                "{随机|欢迎|来了| 撒花 }",
                &VariableContext::default(),
                &mut rng,
            );
            assert!(
                ["欢迎", "来了", "撒花"].contains(&out.as_str()),
                "unexpected pick: {out}"
            );
            seen.insert(out);
        }
        // Seeded RNG must exercise more than one branch over 60 draws.
        assert!(seen.len() > 1, "candidate pick looks stuck: {seen:?}");
    }

    #[test]
    fn interpolate_candidate_empty_choice_list_renders_empty() {
        let out = interpolate_with_rng(
            "a{随机| |  }b",
            &VariableContext::default(),
            &mut seeded(),
        );
        assert_eq!(out, "ab");
    }

    #[test]
    fn interpolate_unclosed_candidate_kept_verbatim() {
        let out = interpolate_with_rng(
            "a{随机|AB",
            &VariableContext::default(),
            &mut seeded(),
        );
        assert_eq!(out, "a{随机|AB");
    }

    #[test]
    fn interpolate_unicode_template_kept_intact() {
        let out = interpolate_with_rng(
            "🎉{用户名}✨{主播名称}🎊{当前日期}",
            &ctx(),
            &mut seeded(),
        );
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(out, format!("🎉小粉✨主播桃🎊{date}"));
    }

    #[test]
    fn insert_random_spaces_zero_probability_is_identity() {
        let msg = "你好🎉abc";
        assert_eq!(insert_random_spaces_with_rng(msg, 0.0, &mut seeded()), msg);
        assert_eq!(insert_random_spaces_with_rng("", 1.0, &mut seeded()), "");
    }

    #[test]
    fn insert_random_spaces_full_probability_separates_every_char() {
        // Mirrors the original: spaces only *between* chars, never trailing.
        assert_eq!(
            insert_random_spaces_with_rng("ab", 1.0, &mut seeded()),
            "a b"
        );
        assert_eq!(
            insert_random_spaces_with_rng("你好🎉x", 1.0, &mut seeded()),
            "你 好 🎉 x"
        );
    }

    #[test]
    fn insert_random_spaces_never_splits_unicode() {
        let msg = "弹幕🎉护体abc✨";
        let mut rng = seeded();
        for _ in 0..50 {
            let out = insert_random_spaces_with_rng(msg, 0.15, &mut rng);
            // Removing inserted ASCII spaces must restore the original exactly.
            assert_eq!(out.replace(' ', ""), msg);
            // Same char count modulo inserted spaces: no char was split.
            assert_eq!(
                out.chars().filter(|c| *c != ' ').count(),
                msg.chars().count()
            );
        }
    }

    #[test]
    fn validate_lines_rejects_bad_configs() {
        let good = vec![MessageLine {
            offset_sec: 1.5,
            message: "嗨".into(),
            account_id: "a".into(),
        }];
        assert!(validate_lines(&good).is_ok());
        assert!(validate_lines(&[]).is_err());
        for bad in [
            MessageLine {
                offset_sec: -1.0,
                message: "x".into(),
                account_id: "a".into(),
            },
            MessageLine {
                offset_sec: f64::NAN,
                message: "x".into(),
                account_id: "a".into(),
            },
            MessageLine {
                offset_sec: 1.0,
                message: "   ".into(),
                account_id: "a".into(),
            },
            MessageLine {
                offset_sec: 1.0,
                message: "x".into(),
                account_id: "  ".into(),
            },
        ] {
            assert!(validate_lines(std::slice::from_ref(&bad)).is_err());
        }
    }

    #[test]
    fn plan_schedule_sorts_and_resolves_absolute_triggers() {
        let lines = vec![
            MessageLine {
                offset_sec: 10.0,
                message: "b".into(),
                account_id: "a".into(),
            },
            MessageLine {
                offset_sec: 0.5,
                message: "a".into(),
                account_id: "a".into(),
            },
        ];
        let schedule = plan_schedule(1_000_000, &lines);
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule[0].message, "a");
        assert_eq!(schedule[0].trigger_at, 1_000_500);
        assert_eq!(schedule[1].message, "b");
        assert_eq!(schedule[1].trigger_at, 1_010_000);
    }

    #[test]
    fn send_outcome_constructors() {
        let ok = SendOutcome::ok();
        assert!(ok.ok && ok.error.is_none() && !ok.session_lost);
        let failed = SendOutcome::failed("boom");
        assert!(!failed.ok && failed.error.as_deref() == Some("boom"));
        let lost = SendOutcome::session_lost("acc-1");
        assert!(lost.session_lost && !lost.ok);
        assert!(lost.error.unwrap().contains("acc-1"));
    }

    #[test]
    fn deadline_for_maps_epoch_trigger_onto_start_anchor() {
        let anchor = tokio::time::Instant::now();
        let d = deadline_for(anchor, 1_000_000, 1_002_500);
        assert_eq!(d.duration_since(anchor), Duration::from_millis(2500));
        // Past-due triggers clamp to the anchor (fire immediately).
        let past = deadline_for(anchor, 1_000_000, 999_000);
        assert_eq!(past, anchor);
    }

    #[tokio::test]
    async fn timeline_fires_in_offset_order_with_mock_sender() {
        // Mock send seam behind the real `SendDanmakuFn` type: records
        // rendered dispatches, no browser involved.
        let fired = Arc::new(AsyncMutex::new(Vec::<ResolvedLine>::new()));
        let fired_reader = fired.clone();
        let send: SendDanmakuFn = Arc::new(move |resolved: ResolvedLine| {
            let fired = fired.clone();
            Box::pin(async move {
                fired.lock().await.push(resolved);
                SendOutcome::ok()
            }) as Pin<Box<dyn Future<Output = SendOutcome> + Send>>
        });
        let started_mono = tokio::time::Instant::now();
        let started_at = now_millis();
        let lines = vec![
            MessageLine {
                offset_sec: 0.06,
                message: "第三{anchor}".into(),
                account_id: "acc-c".into(),
            },
            MessageLine {
                offset_sec: 0.0,
                message: "第一".into(),
                account_id: "acc-a".into(),
            },
            MessageLine {
                offset_sec: 0.02,
                message: "第二{用户名}".into(),
                account_id: "acc-b".into(),
            },
        ];
        let schedule = plan_schedule(started_at, &lines);
        assert_eq!(schedule[0].account_id, "acc-a");
        assert_eq!(schedule[2].account_id, "acc-c");

        let cancel = TaskCancel::new();
        let mut handles = Vec::new();
        for line in schedule {
            let send = send.clone();
            let cancel = cancel.clone();
            let deadline = deadline_for(started_mono, started_at, line.trigger_at);
            handles.push(spawn_line_task(deadline, cancel, async move {
                // Render at dispatch time, as the driver does.
                let text = interpolate(&line.message, &ctx());
                let outcome = send(ResolvedLine {
                    account_id: line.account_id.clone(),
                    text,
                    offset_sec: line.offset_sec,
                })
                .await;
                assert!(outcome.ok, "mock send must succeed");
            }));
        }
        for h in handles {
            tokio::time::timeout(Duration::from_secs(5), h)
                .await
                .expect("line task hung")
                .expect("line task panicked");
        }
        let fired = fired_reader.lock().await.clone();
        let summary: Vec<(String, String, f64)> = fired
            .into_iter()
            .map(|r| (r.account_id, r.text, r.offset_sec))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("acc-a".to_string(), "第一".to_string(), 0.0),
                ("acc-b".to_string(), "第二小粉".to_string(), 0.02),
                ("acc-c".to_string(), "第三主播桃".to_string(), 0.06),
            ]
        );
    }

    #[tokio::test]
    async fn cancel_suppresses_pending_line_before_deadline() {
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ran_task = ran.clone();
        let cancel = TaskCancel::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let handle = spawn_line_task(deadline, cancel.clone(), async move {
            ran_task.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("cancelled task hung")
            .expect("cancelled task panicked");
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cancel_wins_over_already_ready_deadline() {
        // Both branches ready: `biased` must prefer cancellation.
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ran_task = ran.clone();
        let cancel = TaskCancel::new();
        cancel.cancel();
        let deadline = tokio::time::Instant::now() - Duration::from_secs(1);
        let handle = spawn_line_task(deadline, cancel, async move {
            ran_task.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("task hung")
            .expect("task panicked");
        assert!(
            !ran.load(std::sync::atomic::Ordering::SeqCst),
            "ready deadline must not beat cancellation"
        );
    }
}
