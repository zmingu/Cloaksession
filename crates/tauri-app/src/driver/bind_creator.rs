//! Jinniu creator-authorize (bindCreator) — port of jieger
//! `electron/main/tasks/jinniu/bindCreator.ts`.
//!
//! Page: `https://niu.e.kuaishou.com/account/authorize` (jinniu backend,
//! `homeType=new` forced on parameterized URLs — fast-shou naming is
//! inverted, `new` = legacy edition).
//!
//! Flow:
//! - `getAuthorizeList(jinniu_id)`: read persisted records (no browser).
//! - `syncAuthorizeList`: navigate to the authorize page, scrape the
//!   `.ant-table` rows via `evaluate`, `upsertBatch` into
//!   `jinniu_authorize_records`.
//! - `startBasicAuthorize(jinniu_id, kuaishou_id, skip_confirm)`: add-button
//!   → user-select dropdown → search `kuaishou_id` → pick exact match →
//!   agreement checkbox → confirm (or stop after selection for dry-run).
//!
//! Browser work runs on a per-call `TaskPage` lease over a fresh bound page;
//! SQLite stays on the launcher thread via `LauncherCmd`. `jinniu_id` is the
//! jieger business key; `profile_id` resolves the live browser session.

use super::{LauncherCmd, TauriBrowserDriver};
use cdp_driver::{SelectorState, TaskPage};
use multizen_core::{MultizenError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::oneshot;

// ---------------------------------------------------------------------------
// Constants (jieger values, kept verbatim)
// ---------------------------------------------------------------------------

/// Authorize list page (jieger `AUTHORIZE_URL`).
pub const AUTHORIZE_URL: &str = "https://niu.e.kuaishou.com/account/authorize";
/// Jinniu home with legacy-edition lock (shared context entry).
pub const JINNIU_HOME_URL: &str = "https://niu.e.kuaishou.com/home?homeType=new";
/// Search-for-creator wait (jieger `SEARCH_TIMEOUT_MS`).
pub const SEARCH_TIMEOUT_MS: u64 = 10_000;
/// Single UI-action wait (jieger `ACTION_TIMEOUT_MS`).
pub const ACTION_TIMEOUT_MS: u64 = 5_000;
/// Navigation wait for authorize pages.
pub const NAV_TIMEOUT: Duration = Duration::from_secs(15);
/// Poll rounds for the add-button click (the list may render after
/// DOMContentLoaded; each round waits ACTION_TIMEOUT_MS + 500ms).
pub const ADD_BUTTON_ATTEMPTS: u32 = 10;
/// Ant table appearance wait.
pub const TABLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Feedback-modal selector (jieger `FEEDBACK_MODAL_SEL`).
pub const FEEDBACK_MODAL_SEL: &str = "[class*=\"feedbackModal\"]";
/// Authorize table + first-column detail selectors (jieger evaluate).
pub const TABLE_SEL: &str = ".ant-table";
pub const TABLE_ROW_SEL: &str = ".ant-table-tbody tr.ant-table-row";

/// `KS_JINNIU_SELECTORS.authorize` port. Text-button selectors from jieger
/// (`button:has-text(...)`) are Playwright-only and cannot run through
/// `querySelector`; the driver clicks those via `evaluate` text search
/// (see `click_button_by_text_js`). The CSS selectors below are the
/// `querySelector`-compatible subset.
pub struct AuthorizeSelectors {
    pub add_button_text: &'static str,
    pub user_select_trigger: &'static str,
    pub dropdown_search_input: &'static str,
    pub user_item: &'static str,
    pub selected_display: &'static str,
    pub agreement_checkbox_scope: &'static str,
    pub confirm_button_text: &'static str,
}

pub const AUTHORIZE_SELECTORS: AuthorizeSelectors = AuthorizeSelectors {
    add_button_text: "新增授权申请",
    user_select_trigger: ".KwaiUserSelect-module__kwaiUserSelect___IaPxG .ant-select-selector",
    dropdown_search_input: ".KwaiUserSelect-module__searchInput___GzpPn input.ant-input",
    user_item: ".KwaiUserSelect-module__userItem___UF_X1",
    selected_display: ".KwaiUserSelect-module__selectedDisplay___nuR5X",
    agreement_checkbox_scope:
        ".ant-modal-wrap:not([style*=\"display: none\"]) .ant-checkbox-wrapper",
    confirm_button_text: "确定",
};

const CONFIRM_BUTTON_TEXT_FALLBACK: &str = "确认";

// ---------------------------------------------------------------------------
// IPC / storage shapes
// ---------------------------------------------------------------------------

/// One authorize row (jieger `AuthorizeItem`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeItem {
    pub user_id: String,
    pub user_name: String,
    pub status: String,
    pub authorize_time: String,
}

impl From<profile_manager::JinniuAuthorizeItem> for AuthorizeItem {
    fn from(v: profile_manager::JinniuAuthorizeItem) -> Self {
        Self {
            user_id: v.user_id,
            user_name: v.user_name,
            status: v.status,
            authorize_time: v.authorize_time,
        }
    }
}

impl From<AuthorizeItem> for profile_manager::JinniuAuthorizeItem {
    fn from(v: AuthorizeItem) -> Self {
        Self {
            user_id: v.user_id,
            user_name: v.user_name,
            status: v.status,
            authorize_time: v.authorize_time,
        }
    }
}

impl From<profile_manager::JinniuAuthorizeRecord> for AuthorizeItem {
    fn from(r: profile_manager::JinniuAuthorizeRecord) -> Self {
        Self {
            user_id: r.user_id,
            user_name: r.user_name,
            status: r.status,
            authorize_time: r.authorize_time,
        }
    }
}

/// List result (jieger `AuthorizeListResult`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizeListResult {
    pub ok: bool,
    pub data: Vec<AuthorizeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AuthorizeListResult {
    pub fn ok(data: Vec<AuthorizeItem>) -> Self {
        Self {
            ok: true,
            data,
            error: None,
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            ok: false,
            data: Vec::new(),
            error: Some(error),
        }
    }
}

/// Bind result (jieger `BindCreatorResult`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindCreatorResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BindCreatorResult {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn err(error: String) -> Self {
        Self {
            ok: false,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested, no browser)
// ---------------------------------------------------------------------------

fn error(message: impl Into<String>) -> MultizenError {
    MultizenError::Mcp(message.into())
}

pub(crate) fn validate_jinniu_id(jinniu_id: &str) -> Result<String> {
    let id = jinniu_id.trim().to_string();
    if id.is_empty() || id.chars().count() > 128 || id.chars().any(char::is_control) {
        return Err(error("金牛账号ID不能为空，最多128个字符，且不能含控制字符"));
    }
    Ok(id)
}

pub(crate) fn validate_kuaishou_id(kuaishou_id: &str) -> Result<String> {
    let id = kuaishou_id.trim().to_string();
    if id.is_empty() || id.chars().count() > 128 || id.chars().any(char::is_control) {
        return Err(error("快手ID必须为非空字符串，最多128个字符"));
    }
    Ok(id)
}

/// Authorize page URL for an optional sub-account context. `homeType=new`
/// is forced whenever an account id is present (legacy-edition lock).
pub fn build_authorize_url(account_id: Option<&str>) -> String {
    match account_id.map(str::trim).filter(|v| !v.is_empty()) {
        Some(id) => format!("{AUTHORIZE_URL}?__accountId__={id}&homeType=new"),
        None => AUTHORIZE_URL.to_string(),
    }
}

/// JS hiding the jinniu feedback modal (jieger `dismissFeedbackModal`).
pub fn dismiss_feedback_js() -> String {
    format!(
        "(() => {{ const els = document.querySelectorAll({sel}); \
         let n = 0; for (const el of els) {{ el.style.display = 'none'; n++; }} \
         return n; }})()",
        sel = serde_json::to_string(FEEDBACK_MODAL_SEL).unwrap()
    )
}

/// Table-scrape JS (jieger `syncAuthorizeList` evaluate, verbatim logic).
/// Returns a JSON array of `{userId,userName,status,authorizeTime}`.
pub fn authorize_table_js() -> &'static str {
    r#"(() => {
      const rows = Array.from(document.querySelectorAll('.ant-table-tbody tr.ant-table-row'));
      return rows.map((row) => {
        const cells = Array.from(row.querySelectorAll('td'));
        const firstCell = cells[0];
        let userId = '';
        let userName = '';
        if (firstCell) {
          const idEl = firstCell.querySelector('[class*="id_"]');
          if (idEl) {
            userId = (idEl.textContent?.trim() || '').replace(/^ID[:：]\s*/, '');
          }
          const descEl = firstCell.querySelector('[class*="desc"]');
          if (descEl?.firstElementChild) {
            userName = descEl.firstElementChild.textContent?.trim() || '';
          }
          if (!userId) {
            const raw = firstCell.textContent?.trim() || '';
            const match = raw.match(/ID[:：]\s*(\d+)/);
            userId = match ? match[1] : raw;
          }
        }
        return {
          userId,
          userName,
          status: cells[1]?.textContent?.trim() || '',
          authorizeTime: cells[2]?.textContent?.trim() || '',
        };
      });
    })()"#
}

/// Pick the dropdown user whose id contains `kuaishou_id` (jieger logic).
/// Returns `true` when a row was clicked.
pub fn select_user_js(kuaishou_id: &str) -> String {
    format!(
        r#"(targetId => {{
          const items = Array.from(document.querySelectorAll('.KwaiUserSelect-module__userItem___UF_X1'));
          for (const item of items) {{
            const idEl = item.querySelector('.KwaiUserSelect-module__userId___mY_TG');
            if (idEl?.textContent?.includes(targetId)) {{
              item.click();
              return true;
            }}
          }}
          return false;
        }})({})"#,
        serde_json::to_string(kuaishou_id).unwrap()
    )
}

/// Click the first visible `<button>` whose text contains `text`.
/// Playwright `:has-text` has no `querySelector` equivalent, so text
/// matching lives in JS. Returns `true` when clicked.
pub fn click_button_by_text_js(text: &str) -> String {
    format!(
        r#"(wanted => {{
          const btns = Array.from(document.querySelectorAll('button'));
          for (const b of btns) {{
            if (b.textContent?.includes(wanted) && b.offsetParent !== null) {{
              b.click();
              return true;
            }}
          }}
          return false;
        }})({})"#,
        serde_json::to_string(text).unwrap()
    )
}

/// Click the first visible checkbox wrapper inside the open modal.
/// Returns `true` when clicked.
pub fn click_agreement_js() -> String {
    format!(
        r#"(() => {{
          const el = document.querySelector({});
          if (el && el.offsetParent !== null) {{ el.click(); return true; }}
          return false;
        }})()"#,
        serde_json::to_string(AUTHORIZE_SELECTORS.agreement_checkbox_scope).unwrap()
    )
}

/// Parse an `evaluate` payload into authorize items. Missing cells become
/// empty strings (jieger `|| ''`); non-object rows are skipped.
pub fn parse_authorize_items(value: &serde_json::Value) -> Vec<AuthorizeItem> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|row| {
            let obj = row.as_object()?;
            let str_field = |keys: &[&str]| {
                keys.iter()
                    .filter_map(|k| obj.get(*k))
                    .find_map(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            Some(AuthorizeItem {
                user_id: str_field(&["userId", "user_id"]),
                user_name: str_field(&["userName", "user_name"]),
                status: str_field(&["status"]),
                authorize_time: str_field(&["authorizeTime", "authorize_time"]),
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Launcher-thread DB bridge (SQLite never leaves the launcher thread)
// ---------------------------------------------------------------------------

impl TauriBrowserDriver {
    async fn jinniu_db<T: Send + 'static>(
        &self,
        op: impl FnOnce(&profile_manager::ProfileManager) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::BindCreator {
                operation: Box::new(move |pm| {
                    if resp.is_closed() {
                        return;
                    }
                    let _ = resp.send(op(pm));
                }),
            })
            .await
            .map_err(|_| error("达人授权存储线程不可用，请重启应用后重试"))?;
        receive
            .await
            .map_err(|_| error("达人授权存储响应已取消，请重试"))?
    }

    /// Read persisted authorize records (no browser).
    pub async fn bind_creator_list(&self, jinniu_id: &str) -> Result<Vec<AuthorizeItem>> {
        let id = validate_jinniu_id(jinniu_id)?;
        self.jinniu_db(move |pm| {
            Ok(pm
                .jinniu_authorize_get(&id)?
                .into_iter()
                .map(AuthorizeItem::from)
                .collect())
        })
        .await
    }

    async fn bind_creator_persist(
        &self,
        jinniu_id: String,
        items: Vec<AuthorizeItem>,
    ) -> Result<()> {
        let converted: Vec<profile_manager::JinniuAuthorizeItem> =
            items.into_iter().map(Into::into).collect();
        self.jinniu_db(move |pm| pm.jinniu_authorize_upsert_batch(&jinniu_id, &converted))
            .await
    }
}

// ---------------------------------------------------------------------------
// Browser orchestration (TaskPage leases over a fresh bound page)
// ---------------------------------------------------------------------------

/// Acquire an exclusive `TaskPage` on a fresh authorize page. The page is
/// owned by the browser; dropping the lease unlocks it (never closes it).
async fn authorize_task_page<'a>(
    session: &'a cdp_driver::session::BrowserSession,
    cancel: cdp_driver::TaskCancel,
    url: &str,
) -> Result<TaskPage<'a>> {
    let bound = session
        .new_bound_page(url)
        .await
        .map_err(|e| error(format!("达人授权建页失败：{e}")))?;
    bound
        .into_task(cancel, Duration::from_secs(8))
        .await
        .map_err(|e| error(format!("达人授权任务锁失败：{e}")))
}

async fn evaluate(
    page: &mut TaskPage<'_>,
    expression: String,
    timeout: Duration,
    what: &str,
) -> Result<serde_json::Value> {
    page.evaluate(&expression, timeout)
        .await
        .map_err(|e| error(format!("达人授权{what}失败：{e}")))
}

impl TauriBrowserDriver {
    /// Error when no live browser session exists for `profile_id`.
    fn no_session_error(profile_id: &str) -> MultizenError {
        error(format!(
            "浏览器环境 `{profile_id}` 未运行；请先启动对应金牛环境后再同步授权列表"
        ))
    }

    /// Sync the authorize list: navigate → scrape `.ant-table` → persist.
    pub async fn bind_creator_sync(
        &self,
        profile_id: &str,
        jinniu_id: &str,
        account_id: Option<&str>,
    ) -> Result<AuthorizeListResult> {
        let jinniu_key = validate_jinniu_id(jinniu_id)?;
        let slot = self
            .registry
            .slot(profile_id)
            .await
            .ok_or_else(|| Self::no_session_error(profile_id))?;
        let session = slot.session().ok_or_else(|| {
            error(format!(
                "浏览器环境 `{profile_id}` 会话未就绪；请重新启动后再试"
            ))
        })?;
        let url = build_authorize_url(account_id);
        let mut task = authorize_task_page(&session, slot.cancel.clone(), &url).await?;
        task.navigate(&url, NAV_TIMEOUT)
            .await
            .map_err(|e| error(format!("达人授权页导航失败：{e}")))?;
        // The table may already be present; a missing table is non-fatal and
        // yields an empty scrape (jieger `.catch(() => {})`).
        let _ = task
            .wait_for_selector(
                TABLE_SEL,
                SelectorState::Attached,
                TABLE_TIMEOUT,
                Duration::from_millis(200),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(1_000)).await;
        let raw = evaluate(
            &mut task,
            authorize_table_js().to_string(),
            Duration::from_secs(8),
            "授权列表读取",
        )
        .await?;
        let items = parse_authorize_items(&raw);
        self.bind_creator_persist(jinniu_key.clone(), items.clone())
            .await?;
        tracing::info!(jinniu = %jinniu_key, count = items.len(), "bindCreator: 授权列表已同步");
        Ok(AuthorizeListResult::ok(items))
    }

    /// Full bind flow (jieger `startBasicAuthorize`).
    pub async fn bind_creator_authorize(
        &self,
        profile_id: &str,
        jinniu_id: &str,
        kuaishou_id: &str,
        skip_confirm: bool,
        account_id: Option<&str>,
    ) -> Result<BindCreatorResult> {
        let _jinniu_key = validate_jinniu_id(jinniu_id)?;
        let target = validate_kuaishou_id(kuaishou_id)?;
        let slot = self
            .registry
            .slot(profile_id)
            .await
            .ok_or_else(|| Self::no_session_error(profile_id))?;
        let session = slot.session().ok_or_else(|| {
            error(format!(
                "浏览器环境 `{profile_id}` 会话未就绪；请重新启动后再试"
            ))
        })?;
        let url = build_authorize_url(account_id);
        let mut task = authorize_task_page(&session, slot.cancel.clone(), &url).await?;
        let action = Duration::from_millis(ACTION_TIMEOUT_MS);
        let search = Duration::from_millis(SEARCH_TIMEOUT_MS);

        task.navigate(&url, NAV_TIMEOUT)
            .await
            .map_err(|e| error(format!("达人授权页导航失败：{e}")))?;

        // 打开「新增授权申请」弹窗。按钮要等列表数据渲染完才挂载，可能晚于
        // DOMContentLoaded，所以轮询点击（jieger waitForSelector(addButton) +
        // click 的等价物）。该按钮是弹窗打开前页面上的唯一入口。
        let mut clicked = false;
        let mut click_err: Option<MultizenError> = None;
        for _ in 0..ADD_BUTTON_ATTEMPTS {
            let _ = evaluate(
                &mut task,
                dismiss_feedback_js(),
                Duration::from_secs(5),
                "反馈弹窗隐藏",
            )
            .await;
            match evaluate(
                &mut task,
                click_button_by_text_js(AUTHORIZE_SELECTORS.add_button_text),
                action,
                "新增授权申请点击",
            )
            .await
            {
                Ok(v) if v.as_bool().unwrap_or(false) => {
                    clicked = true;
                    break;
                }
                Ok(_) => {}
                Err(e) => click_err = Some(e),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if !clicked {
            return match click_err {
                Some(e) => Err(error(format!("新增授权申请按钮点击失败：{e}"))),
                None => Ok(BindCreatorResult::err(
                    "未找到“新增授权申请”按钮，页面结构可能已变化".to_string(),
                )),
            };
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = evaluate(
            &mut task,
            dismiss_feedback_js(),
            Duration::from_secs(5),
            "反馈弹窗隐藏",
        )
        .await;

        // 达人选择器只存在于弹窗内部：等弹窗挂载完成再点击，消除竞态。
        task.wait_for_selector(
            AUTHORIZE_SELECTORS.user_select_trigger,
            SelectorState::Attached,
            action,
            Duration::from_millis(200),
        )
        .await
        .map_err(|e| error(format!("授权弹窗未就绪（达人选择器缺失）：{e}")))?;
        task.click(AUTHORIZE_SELECTORS.user_select_trigger, action)
            .await
            .map_err(|e| error(format!("达人选择器打开失败：{e}")))?;
        task.wait_for_selector(
            AUTHORIZE_SELECTORS.dropdown_search_input,
            SelectorState::Visible,
            action,
            Duration::from_millis(200),
        )
        .await
        .map_err(|e| error(format!("达人搜索框未出现：{e}")))?;
        task.click(AUTHORIZE_SELECTORS.dropdown_search_input, action)
            .await
            .map_err(|e| error(format!("达人搜索框聚焦失败：{e}")))?;
        // Clear then fill (jieger fills '' first, waits 300ms, fills the id).
        let _ = task
            .type_text(AUTHORIZE_SELECTORS.dropdown_search_input, "", action)
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        task.type_text(AUTHORIZE_SELECTORS.dropdown_search_input, &target, action)
            .await
            .map_err(|e| error(format!("达人ID输入失败：{e}")))?;
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        task.wait_for_selector(
            AUTHORIZE_SELECTORS.user_item,
            SelectorState::Attached,
            search,
            Duration::from_millis(200),
        )
        .await
        .map_err(|_| error(format!("未找到快手ID为 {target} 的达人")))?;
        let picked = evaluate(&mut task, select_user_js(&target), action, "达人选中")
            .await?
            .as_bool()
            .unwrap_or(false);
        if !picked {
            return Ok(BindCreatorResult::err(format!(
                "未找到快手ID为 {target} 的达人"
            )));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;

        let selected = task
            .wait_for_selector(
                AUTHORIZE_SELECTORS.selected_display,
                SelectorState::Attached,
                action,
                Duration::from_millis(200),
            )
            .await
            .is_ok();
        if !selected {
            return Ok(BindCreatorResult::err(
                "未能正确选中目标达人，请检查快手ID是否正确".to_string(),
            ));
        }
        if skip_confirm {
            tracing::info!(kuaishou = %target, "bindCreator: 达人已选中，跳过确认（dryRun）");
            return Ok(BindCreatorResult::ok());
        }

        let agreed = evaluate(&mut task, click_agreement_js(), action, "授权协议勾选")
            .await?
            .as_bool()
            .unwrap_or(false);
        if !agreed {
            return Ok(BindCreatorResult::err(
                "未找到授权协议勾选框，页面结构可能已变化".to_string(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        let confirmed = evaluate(
            &mut task,
            click_button_by_text_js(AUTHORIZE_SELECTORS.confirm_button_text),
            action,
            "授权确认点击",
        )
        .await?
        .as_bool()
        .unwrap_or(false)
            || evaluate(
                &mut task,
                click_button_by_text_js(CONFIRM_BUTTON_TEXT_FALLBACK),
                action,
                "授权确认点击",
            )
            .await?
            .as_bool()
            .unwrap_or(false);
        if !confirmed {
            return Ok(BindCreatorResult::err(
                "未找到确认按钮，页面结构可能已变化".to_string(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(2_000)).await;
        tracing::info!(kuaishou = %target, "bindCreator: 基础授权申请已提交");
        Ok(BindCreatorResult::ok())
    }
}

// ---------------------------------------------------------------------------
// Tests (offline: pure JS/parse helpers + launcher-thread DB roundtrip)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_js_covers_jieger_row_selectors() {
        let js = authorize_table_js();
        for needle in [
            ".ant-table-tbody tr.ant-table-row",
            "[class*=\"id_\"]",
            "[class*=\"desc\"]",
            "authorizeTime",
        ] {
            assert!(js.contains(needle), "{needle}");
        }
    }

    #[test]
    fn dismiss_js_targets_feedback_modal() {
        let js = dismiss_feedback_js();
        // Selector is JSON-escaped into the JS literal; match semantic content.
        assert!(js.contains("feedbackModal"));
        assert!(js.contains("display"));
    }

    #[test]
    fn select_user_js_escapes_target_id() {
        let js = select_user_js("123\"<x>");
        assert!(js.contains(".KwaiUserSelect-module__userItem___UF_X1"));
        assert!(js.contains(".KwaiUserSelect-module__userId___mY_TG"));
        assert!(js.contains(&serde_json::to_string("123\"<x>").unwrap()));
    }

    #[test]
    fn authorize_url_forces_home_type_new_with_account() {
        assert_eq!(build_authorize_url(None), AUTHORIZE_URL);
        assert_eq!(
            build_authorize_url(Some(" 42 ")),
            format!("{AUTHORIZE_URL}?__accountId__=42&homeType=new")
        );
    }

    #[test]
    fn parse_scraped_rows_with_snake_and_camel_keys() {
        let value = serde_json::json!([
            {"userId": "u1", "userName": "n1", "status": "已授权", "authorizeTime": "t1"},
            {"user_id": "u2", "user_name": "n2", "status": "待确认", "authorize_time": "t2"},
            {"userId": "", "userName": "", "status": "", "authorizeTime": ""},
            "not-an-object"
        ]);
        let items = parse_authorize_items(&value);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].user_id, "u1");
        assert_eq!(items[1].user_name, "n2");
        assert!(parse_authorize_items(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn id_validation_rejects_blank_and_control_chars() {
        assert!(validate_jinniu_id(" j1 ").unwrap() == "j1");
        assert!(validate_jinniu_id("").is_err());
        assert!(validate_kuaishou_id("k1").is_ok());
        assert!(validate_kuaishou_id("  ").is_err());
    }

    #[test]
    fn text_button_js_uses_safe_json_escaping() {
        let js = click_button_by_text_js(AUTHORIZE_SELECTORS.add_button_text);
        assert!(js.contains("offsetParent"));
        assert!(js.contains(&serde_json::to_string(AUTHORIZE_SELECTORS.add_button_text).unwrap()));
    }

    #[tokio::test]
    async fn launcher_thread_db_roundtrip_upserts_and_isolates() {
        use crate::driver::business_tests::fixture;
        use multizen_core::ChromixSettings;
        let (_dir, d) = fixture(ChromixSettings::default());
        assert!(d.bind_creator_list("j1").await.unwrap().is_empty());
        d.bind_creator_persist(
            "j1".into(),
            vec![
                AuthorizeItem {
                    user_id: "u1".into(),
                    user_name: "n1".into(),
                    status: "已授权".into(),
                    authorize_time: "t1".into(),
                },
                AuthorizeItem {
                    user_id: "".into(),
                    user_name: "skip".into(),
                    status: "".into(),
                    authorize_time: "".into(),
                },
            ],
        )
        .await
        .unwrap();
        // Update same (jinniu_id, user_id) + add another; other jinniu untouched.
        d.bind_creator_persist(
            "j1".into(),
            vec![AuthorizeItem {
                user_id: "u1".into(),
                user_name: "n1-new".into(),
                status: "待确认".into(),
                authorize_time: "t2".into(),
            }],
        )
        .await
        .unwrap();
        d.bind_creator_persist(
            "j2".into(),
            vec![AuthorizeItem {
                user_id: "u9".into(),
                user_name: "n9".into(),
                status: "s".into(),
                authorize_time: "t".into(),
            }],
        )
        .await
        .unwrap();
        let list = d.bind_creator_list("j1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].user_name, "n1-new");
        assert_eq!(list[0].status, "待确认");
        assert_eq!(d.bind_creator_list("j2").await.unwrap().len(), 1);
        assert!(d.bind_creator_list("").await.is_err());
        d.shutdown().await;
    }

    #[test]
    fn commands_are_registered_in_actual_tauri_handler() {
        let source = include_str!("../lib.rs");
        let handler = source
            .split(".invoke_handler(tauri::generate_handler![")
            .nth(1)
            .unwrap()
            .split("])")
            .next()
            .unwrap();
        for command in [
            "bind_creator_get_authorize_list",
            "bind_creator_sync_authorize_list",
            "bind_creator_start_authorize",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
        let adapter = include_str!("../commands/bind_creator.rs");
        assert_eq!(adapter.matches("#[tauri::command]").count(), 3);
    }
}
