//! Fixed-target page operations for Rust automation, independent of legacy active-page selection.
use chromiumoxide::Page;
use multizen_core::{MultizenError, Result};

use crate::page_ops::{OperationPolicy, PageOperations};
use crate::session::BrowserSession;
use crate::tools::NavResult;

/// A page attached to one session and target. Binding never activates the page.
///
/// This is not an exclusive lease: other bindings, legacy tools and the user may
/// still change the same page. Use `into_task` for cooperative task exclusion.
/// Dropping a binding does not close its page or browser. A closed/detached page
/// returns an error; operations never select another target or retry elsewhere.
/// Keep an `Arc<BrowserSession>` in the task and borrow it to create this handle.
pub struct BoundPage<'session> {
    session: &'session BrowserSession,
    page: Page,
}

impl<'session> BoundPage<'session> {
    pub(crate) fn new(session: &'session BrowserSession, page: Page) -> Self {
        Self { session, page }
    }

    pub fn target_id(&self) -> &str {
        self.page.target_id().as_ref()
    }

    fn operations(&self) -> PageOperations<'_> {
        PageOperations::new(self.session, &self.page, OperationPolicy::Bound)
    }

    /// Consume this binding and queue for its session/target's cooperative lease.
    /// Target existence is rechecked after the lock is acquired.
    pub async fn into_task(
        self,
        cancel: crate::task_page::TaskCancel,
        timeout: std::time::Duration,
    ) -> crate::task_page::TaskResult<crate::task_page::TaskPage<'session>> {
        self.session
            .task_page(self.target_id(), cancel, timeout)
            .await
    }

    pub(crate) async fn read_public_avatar(&self, url: &str) -> Result<String> {
        let lock = self.session.avatar_locks.for_target(self.target_id());
        crate::avatar_resource::read(&self.page, url, lock).await
    }

    pub(crate) async fn navigate_unbounded(&self, url: &str) -> Result<NavResult> {
        self.operations().navigate(url, 0).await
    }

    /// Timeout covers goto and metadata extraction, unlike the legacy session API.
    /// A timeout drops the wait; it does not undo an already-sent navigation.
    pub async fn navigate(&self, url: &str, timeout_ms: u64) -> Result<NavResult> {
        tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            self.operations().navigate(url, timeout_ms),
        )
        .await
        .map_err(|_| {
            MultizenError::Cdp(format!(
                "navigate target {} timed out after {timeout_ms}ms",
                self.target_id()
            ))
        })?
    }

    /// PNG base64, without a data-URI prefix or implicit page activation.
    pub async fn screenshot(&self) -> Result<String> {
        self.operations().screenshot().await
    }

    pub async fn evaluate(&self, expression: &str) -> Result<serde_json::Value> {
        self.operations().evaluate(expression).await
    }

    /// Propagates selector/evaluation/input dispatch errors; not a DOM actionability wait.
    pub async fn click(&self, selector: &str) -> Result<()> {
        self.operations().click(selector).await
    }

    /// Focuses the selector and sends key events. Missing elements and CDP errors fail.
    /// Successful dispatch does not prove the application accepted the resulting text.
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<()> {
        self.operations().type_text(selector, text).await
    }

    /// Returns url/title and at most 8000 innerText characters, not an accessibility tree.
    pub async fn extract(&self) -> Result<serde_json::Value> {
        self.operations().extract().await
    }
}
