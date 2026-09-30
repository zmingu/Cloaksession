//! Cooperative, fixed-target task leases. These are not browser transactions.
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex, Weak};
use std::time::Duration;

use multizen_core::{MultizenError, Result};
use tokio::sync::{watch, Mutex, OwnedMutexGuard};
use tokio::time::{sleep, sleep_until, Instant};

use crate::{session::BrowserSession, tools::NavResult, BoundPage};

pub type TaskResult<T> = std::result::Result<T, TaskError>;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("task cancelled (already dispatched CDP commands are not withdrawn)")]
    Cancelled,
    #[error("task deadline expired (browser completion is unknown)")]
    TimedOut,
    #[error("an operation future was dropped; acquire a new lease before more actions")]
    Interrupted,
    #[error("invalid task options: {0}")]
    InvalidOptions(&'static str),
    #[error(transparent)]
    Driver(#[from] MultizenError),
}

/// Cloneable, one-shot cancellation shared by the queue and all lease operations.
/// Dropping a handle does not cancel; call `cancel`. There is no reset.
#[derive(Clone, Debug)]
pub struct TaskCancel {
    sender: watch::Sender<bool>,
}

impl Default for TaskCancel {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskCancel {
    pub fn new() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }

    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    /// Wait for cancellation in an enclosing read-only task (including network work).
    pub async fn cancelled(&self) {
        // subscribe + wait_for checks current state as well as future changes.
        // `self` retains the sender, so the channel cannot close while waiting.
        let mut receiver = self.sender.subscribe();
        let _ = receiver.wait_for(|cancelled| *cancelled).await;
    }
}

/// Session-local weak table: owners/waiters keep their lock alive, not the table.
#[derive(Default)]
pub(crate) struct TaskLocks(StdMutex<HashMap<String, Weak<Mutex<()>>>>);

impl TaskLocks {
    pub(crate) fn for_target(&self, target: &str) -> Arc<Mutex<()>> {
        let mut locks = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(target).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(target.to_owned(), Arc::downgrade(&lock));
        lock
    }
}

#[derive(Clone, Copy)]
enum Stopped {
    Cancelled,
    TimedOut,
    Interrupted,
}

impl Stopped {
    fn error(self) -> TaskError {
        match self {
            Self::Cancelled => TaskError::Cancelled,
            Self::TimedOut => TaskError::TimedOut,
            Self::Interrupted => TaskError::Interrupted,
        }
    }
}

async fn controlled<T>(
    stopped: &mut Option<Stopped>,
    cancel: &TaskCancel,
    timeout: Duration,
    operation: impl Future<Output = Result<T>>,
) -> TaskResult<T> {
    if let Some(reason) = *stopped {
        return Err(reason.error());
    }
    if cancel.is_cancelled() {
        *stopped = Some(Stopped::Cancelled);
        return Err(TaskError::Cancelled);
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(TaskError::InvalidOptions("timeout exceeds clock range"))?;
    if timeout.is_zero() {
        *stopped = Some(Stopped::TimedOut);
        return Err(TaskError::TimedOut);
    }
    // Pessimistic latch survives external drop/abort of this future. Clear only
    // after a completed operation. No unsafe custom Future/Drop machinery needed.
    *stopped = Some(Stopped::Interrupted);
    tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            *stopped = Some(Stopped::Cancelled);
            Err(TaskError::Cancelled)
        }
        _ = sleep_until(deadline) => {
            *stopped = Some(Stopped::TimedOut);
            Err(TaskError::TimedOut)
        }
        result = operation => {
            *stopped = None;
            result.map_err(TaskError::Driver)
        }
    }
}

impl BrowserSession {
    /// Queue for one target in this session. Timeout covers queue + target lookup.
    /// Only TaskPage users participate; legacy/BoundPage/user/other sessions do not.
    pub async fn task_page(
        &self,
        target_id: &str,
        cancel: TaskCancel,
        timeout: Duration,
    ) -> TaskResult<TaskPage<'_>> {
        let (page, guard) = controlled(&mut None, &cancel, timeout, async {
            let guard = self.task_locks.for_target(target_id).lock_owned().await;
            // A queued target may have closed. Revalidate only after owning its lock.
            let page = self.bind_page(target_id).await?;
            Ok((page, guard))
        })
        .await?;
        Ok(TaskPage {
            page,
            _guard: guard,
            cancel,
            stopped: None,
        })
    }
}

/// Exclusive cooperative lease on one session/target; never activates/falls back.
///
/// Methods require &mut self, so one lease cannot run concurrent actions. No raw
/// BoundPage or Deref escape hatch is exposed. release/drop unlocks, not closes.
/// Cancellation/timeout poisons the lease but retains its lock until release/drop.
/// Dropping an in-flight action future also poisons it (Interrupted).
///
/// Cancellation stops local waiting/future dispatch, NOT already queued CDP or
/// browser work. Input may stop after keyDown/mousePressed without release. There
/// is no rollback or automatic retry; unlocking does not prove the browser stopped.
///
/// ```compile_fail,E0499
/// use cdp_driver::TaskPage;
/// use std::time::Duration;
/// async fn concurrent_on_one_lease(page: &mut TaskPage<'_>) {
///     let _ = tokio::join!(
///         page.evaluate("1", Duration::from_secs(1)),
///         page.evaluate("2", Duration::from_secs(1)),
///     );
/// }
/// ```
pub struct TaskPage<'session> {
    page: BoundPage<'session>,
    _guard: OwnedMutexGuard<()>,
    cancel: TaskCancel,
    stopped: Option<Stopped>,
}

impl TaskPage<'_> {
    pub fn target_id(&self) -> &str {
        self.page.target_id()
    }

    pub fn release(self) {}

    pub async fn navigate(&mut self, url: &str, timeout: Duration) -> TaskResult<NavResult> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.navigate_unbounded(url),
        )
        .await
    }

    pub async fn evaluate(
        &mut self,
        expression: &str,
        timeout: Duration,
    ) -> TaskResult<serde_json::Value> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.evaluate(expression),
        )
        .await
    }

    /// One bounded public yximgs avatar GET via the bound browser's own network.
    /// No redirects, credentials, navigation, active switching or host HTTP fallback.
    pub async fn read_public_avatar(&mut self, url: &str, timeout: Duration) -> TaskResult<String> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.read_public_avatar(url),
        )
        .await
    }

    pub async fn screenshot(&mut self, timeout: Duration) -> TaskResult<String> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.screenshot(),
        )
        .await
    }

    pub async fn click(&mut self, selector: &str, timeout: Duration) -> TaskResult<()> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.click(selector),
        )
        .await
    }

    pub async fn type_text(
        &mut self,
        selector: &str,
        text: &str,
        timeout: Duration,
    ) -> TaskResult<()> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.type_text(selector, text),
        )
        .await
    }

    pub async fn extract(&mut self, timeout: Duration) -> TaskResult<serde_json::Value> {
        controlled(
            &mut self.stopped,
            &self.cancel,
            timeout,
            self.page.extract(),
        )
        .await
    }

    /// Poll querySelector on the bound page. Hidden includes detached.
    /// Visible only checks CSS display/visibility and positive bounding-box size;
    /// it does not prove viewport presence, lack of occlusion, opacity, stability,
    /// enabled state or actionability. Invalid selectors/JS/CDP/payloads fail fast.
    pub async fn wait_for_selector(
        &mut self,
        selector: &str,
        state: SelectorState,
        timeout: Duration,
        poll_interval: Duration,
    ) -> TaskResult<()> {
        if poll_interval.is_zero() {
            return Err(TaskError::InvalidOptions("poll_interval must be positive"));
        }
        let expression = selector_expression(selector);
        controlled(&mut self.stopped, &self.cancel, timeout, async {
            loop {
                let observation: SelectorObservation =
                    serde_json::from_value(self.page.evaluate(&expression).await?)
                        .map_err(|e| MultizenError::Cdp(format!("selector observation: {e}")))?;
                if state.matches(observation) {
                    return Ok(());
                }
                sleep(poll_interval).await;
            }
        })
        .await
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorState {
    Attached,
    Detached,
    Visible,
    Hidden,
}

#[derive(serde::Deserialize)]
struct SelectorObservation {
    attached: bool,
    visible: bool,
}

impl SelectorState {
    fn matches(self, observed: SelectorObservation) -> bool {
        match self {
            Self::Attached => observed.attached,
            Self::Detached => !observed.attached,
            Self::Visible => observed.attached && observed.visible,
            Self::Hidden => !observed.attached || !observed.visible,
        }
    }
}

fn selector_expression(selector: &str) -> String {
    // JSON strings are JS literals; Rust Debug escapes are not (e.g. \u{...}).
    let selector = serde_json::to_string(selector).expect("serializing a string cannot fail");
    format!(
        r#"(() => {{ /* task-selector */
        const el = document.querySelector({selector});
        if (!el) return {{attached:false, visible:false}};
        const style = getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return {{attached:true, visible:style.display !== 'none' &&
            style.visibility !== 'hidden' && style.visibility !== 'collapse' &&
            rect.width > 0 && rect.height > 0}};
    }})()"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weak_lock_table_reuses_live_locks_and_prunes_dead_targets() {
        let locks = TaskLocks::default();
        let a = locks.for_target("a");
        assert!(Arc::ptr_eq(&a, &locks.for_target("a")));
        assert!(!Arc::ptr_eq(&a, &locks.for_target("b")));
        for index in 0..1000 {
            drop(locks.for_target(&index.to_string()));
        }
        assert!(locks.0.lock().unwrap().len() <= 2);
        drop(a);
        let _last = locks.for_target("last");
        assert_eq!(locks.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn selector_states_and_json_escaping() {
        for (attached, visible, expected) in [
            (false, false, [false, true, false, true]),
            (true, false, [true, false, false, true]),
            (true, true, [true, false, true, false]),
        ] {
            for (state, expected) in [
                SelectorState::Attached,
                SelectorState::Detached,
                SelectorState::Visible,
                SelectorState::Hidden,
            ]
            .into_iter()
            .zip(expected)
            {
                assert_eq!(
                    state.matches(SelectorObservation { attached, visible }),
                    expected
                );
            }
        }
        let selector = "[data-value=\"中\\\n\u{1b}\"]";
        assert!(selector_expression(selector).contains(&serde_json::to_string(selector).unwrap()));
    }

    #[tokio::test]
    async fn cancellation_is_sticky_and_wakes_all_clones() {
        let cancel = TaskCancel::new();
        let a = cancel.clone();
        let b = cancel.clone();
        let ((), (), ()) = tokio::join!(a.cancelled(), b.cancelled(), async {
            cancel.cancel();
        });
        cancel.cancel();
        assert!(cancel.is_cancelled());
        cancel.clone().cancelled().await;
    }
}
