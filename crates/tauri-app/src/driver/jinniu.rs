//! 磁力金牛（Jinniu）多大户管理驱动。
//!
//! 移植自 jieger `electron/main/tasks/jinniu/index.ts` +
//! `electron/main/services/jinniu/accountStore.ts`。
//!
//! # 与快手小店/小号的关键差异
//!
//! - **多大户但单选切换**：同时只允许一个金牛会话存活；切换大户 = 关闭旧会话 +
//!   设置新活跃。运行时单会话约束由 [`JinniuRuntime`] 的 `session` 槽位承载，
//!   持久化单活跃约束由 `jinniu_account_state.is_active` 的部分唯一索引兜底。
//! - **账号即环境**：一个大户 = 一行 `business_accounts(kind='jinniu')` + 一个
//!   专属 Profile。`storageStatePath` 在 Cloaksession 中等价于该 Profile 的数据
//!   目录，不单独存列。
//! - **状态机**：`disconnected → connecting → awaiting-sub-account → connected → error`。
//!   扫码完成 ≠ connected：必须用户在弹窗手动选子户、URL 出现 `__accountId__`
//!   才算 connected（**纯 URL 判定，无 DOM 选择器**）。
//! - **登录即识别**：离开登录页后即轮询页面文本识别右上角主账号并自动命名
//!   （占位别名 [`JINNIU_PLACEHOLDER_LABEL`]），识别不依赖选子户。
//!
//! # 离线可测部分
//!
//! URL 判定、状态机迁移、目标子户解析、页面文本解析、单活跃 CRUD 都是纯函数或
//! 只依赖 SQLite，均被下面的单测覆盖；真实的启动/扫码/选子户流程需要活的浏览器
//! 会话，仅做编译检查，测试中永不执行真实登录。

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex as StdMutex,
};
use std::time::Duration;

use cdp_driver::platforms::kuaishou::{self, KS_CONNECT_TIMEOUT};
use cdp_driver::session::BrowserSession;
use cdp_driver::{TaskCancel, TaskError};
use mcp_server::driver::BrowserDriver;
use multizen_core::{
    BusinessAccount, BusinessAccountKind, MultizenError, Result, SaveBusinessAccountInput,
};
use profile_manager::{JinniuAccountRecord, JinniuAccountState};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::oneshot;

use super::{LauncherCmd, TauriBrowserDriver};

/// 状态变化推送事件（payload = [`JinniuStatePayload`]）。
pub const JINNIU_STATUS_CHANGED: &str = "jinniu-status-changed";
/// 大户列表变化推送事件（无 payload，渲染端收到后刷新列表）。
pub const JINNIU_ACCOUNTS_CHANGED: &str = "jinniu-accounts-changed";

/// 扫码登录预算（jieger `KS_LOGIN_MS`）。
pub const JINNIU_LOGIN_TIMEOUT_MS: u64 = 5 * 60_000;
/// 扫码阶段 URL 轮询间隔（jieger `KS_LOGIN_POLL_MS`）。
pub const JINNIU_LOGIN_POLL_MS: u64 = 2_000;
/// 等待用户手动选子户的总超时（jieger `KS_AWAIT_SUB_MS`）。
pub const JINNIU_AWAIT_SUB_TIMEOUT_MS: u64 = 5 * 60_000;
/// 等待选子户阶段的 URL 轮询间隔（jieger `KS_AWAIT_POLL_MS`）。
pub const JINNIU_AWAIT_POLL_MS: u64 = 1_500;

/// 新建账户时的占位别名：前端「添加账户」不再预填名字，先落占位行，待登录
/// 识别出右上角主账号后由 [`TauriBrowserDriver::jinniu_autoname`] 覆盖。
pub const JINNIU_PLACEHOLDER_LABEL: &str = "未命名金牛";

/// 金牛状态机，wire 值与 jieger `JinniuStatus` 一致（kebab-case）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JinniuStatus {
    Disconnected,
    Connecting,
    AwaitingSubAccount,
    Connected,
    Error,
}

impl JinniuStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::AwaitingSubAccount => "awaiting-sub-account",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

/// 主账号（大户）信息，登录后从弹窗/顶栏捕获。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuMaster {
    pub name: String,
    pub id: String,
    pub avatar_url: Option<String>,
}

/// 状态推送 payload（字段与 jieger `JinniuStatePayload` 对齐，用 accountId 替代
/// jinniuId）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuStatePayload {
    pub account_id: String,
    pub status: JinniuStatus,
    pub target_account_id: Option<String>,
    pub error: Option<String>,
    pub master: Option<JinniuMaster>,
    pub current_sub_account_id: Option<String>,
    pub current_sub_account_name: Option<String>,
    pub balance_text: Option<String>,
}

/// 登录选项（jieger `JinniuLoginOptions`）。
///
/// 注：Cloaksession 无「登录中无头↔有头」切换能力，`headless` 仍被接受以保持
/// 契约兼容，但真实登录始终以可见窗口打开——扫码与手动选子户都需要用户看到页面。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuLoginOptions {
    /// 本次希望直接进入的子户 ID（拼到 URL）；缺省回退 `lastSubAccountId`。
    #[serde(default)]
    pub target_sub_account_id: Option<String>,
    #[serde(default)]
    pub headless: bool,
    /// 仅复用已保存登录态；需要扫码或手动选子户则直接失败。
    #[serde(default)]
    pub restore_only: bool,
}

/// 列表项：通用业务账号字段 + 运行时状态（jieger
/// `JinniuAccountWithStatus` 的扁平化版本）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JinniuAccountWithStatus {
    pub id: String,
    pub profile_id: Option<String>,
    pub label: String,
    pub status: JinniuStatus,
    pub is_active: bool,
    pub error: Option<String>,
    pub target_account_id: Option<String>,
    pub master_name: Option<String>,
    pub master_id: Option<String>,
    pub master_avatar_url: Option<String>,
    pub current_sub_account_id: Option<String>,
    pub current_sub_account_name: Option<String>,
    pub balance_text: Option<String>,
}

// ── 纯函数：URL 判定与状态迁移 ────────────────────────────────────────────────

/// URL 是否已进入子户（含 `__accountId__`）。纯 URL 判定，无 DOM 选择器。
pub fn url_has_account_id(url: &str) -> bool {
    kuaishou::extract_jinniu_account_id_from_url(url).is_some()
}

/// URL 是否仍停留在金牛登录页。
pub fn url_is_login_page(url: &str) -> bool {
    kuaishou::is_jinniu_login_page(url)
}

/// URL 判定结果：`None` = 仍需扫码；`Some(Some(id))` = 已进入子户；`Some(None)`
/// = 已离开登录页但尚无 `__accountId__`（等待用户手动选子户）。
pub fn verdict_from_url(url: &str) -> Option<Option<String>> {
    if url_is_login_page(url) {
        return None;
    }
    Some(kuaishou::extract_jinniu_account_id_from_url(url))
}

/// 登录入参目标子户优先级：显式入参 → 上次进入的子户 → 无。
pub fn target_sub_account_for_login(
    option: Option<&str>,
    last: Option<&str>,
) -> Option<String> {
    option
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .or_else(|| last.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string))
}

/// 登录完成后的状态迁移（纯函数，jieger `transitionAfterLogin` 的判定内核）：
/// - 已含 `__accountId__` → `connected`
/// - 已离开登录页但无子户 → `awaiting-sub-account`
/// - 仍在登录页 → `None`（调用方继续等待扫码，不应迁移）
pub fn transition_after_login(url: &str) -> Option<(JinniuStatus, Option<String>)> {
    match verdict_from_url(url) {
        None => None,
        Some(Some(id)) => Some((JinniuStatus::Connected, Some(id))),
        Some(None) => Some((JinniuStatus::AwaitingSubAccount, None)),
    }
}

// ── 纯函数：页面文本解析（无 regex 依赖，best-effort）─────────────────────────

/// 读取页面前若干标记后的连续数字（如 `快手ID 12345` → `12345`）。
pub fn digits_after(text: &str, marker: &str) -> Option<String> {
    let idx = text.find(marker)?;
    let rest = &text[idx + marker.len()..];
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

/// 取标记前最后一个空白/冒号分隔的词元（如 `用户名 张三 快手ID 12345` → `张三`）。
pub fn token_before(text: &str, marker: &str) -> Option<String> {
    let idx = text.find(marker)?;
    let before = text[..idx].trim_end();
    let token = before
        .rsplit(|c: char| c.is_whitespace() || c == '：' || c == ':')
        .next()?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// 从页面文本解析余额（`账户可用余额 ¥123.45` → `¥123.45`）。
pub fn parse_balance(text: &str) -> Option<String> {
    let idx = text
        .find("账户可用余额")
        .or_else(|| text.find("可用余额"))
        .or_else(|| text.find("账户余额"))?;
    let rest = &text[idx..];
    let start = rest.find(|c: char| c.is_ascii_digit())?;
    let number: String = rest[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    let number = number.trim_end_matches('.').to_string();
    if number.is_empty() {
        None
    } else {
        Some(format!("¥{number}"))
    }
}

/// 从页面文本解析大户（主账号）名 + ID（`用户名 张三 快手ID 12345`）。
pub fn parse_master_info(text: &str) -> Option<(String, String)> {
    let id = digits_after(text, "快手ID")?;
    let name = token_before(text, "快手ID")?;
    Some((name, id))
}

/// 从页面文本解析当前子户名（`账户名 甲子户 账户ID 999`）。
pub fn parse_sub_info(text: &str) -> Option<(String, String)> {
    let id = digits_after(text, "账户ID")?;
    let name = token_before(text, "账户ID")?;
    Some((name, id))
}

/// 读取页面可见文本（top document，best-effort）。
pub const JINNIU_PAGE_TEXT_JS: &str = "(() => { const b = document.body; \
     return b ? ((b.innerText || '') + '\\n' + (b.textContent || '')) : ''; })()";

// ── 运行时 ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct JinniuRuntimeState {
    status: JinniuStatus,
    error: Option<String>,
    target_account_id: Option<String>,
    master: Option<JinniuMaster>,
    current_sub_account_id: Option<String>,
    current_sub_account_name: Option<String>,
    balance_text: Option<String>,
}

impl JinniuRuntimeState {
    fn disconnected() -> Self {
        Self {
            status: JinniuStatus::Disconnected,
            error: None,
            target_account_id: None,
            master: None,
            current_sub_account_id: None,
            current_sub_account_name: None,
            balance_text: None,
        }
    }
}

/// 单活跃会话槽位（同时只允许一个大户的连接）。
#[derive(Debug, Clone)]
struct ActiveSession {
    account_id: String,
    profile_id: String,
    target_id: String,
}

/// 金牛运行时：状态表 + 单会话槽位 + 取消令牌 + 事件出口。
pub struct JinniuRuntime {
    pub(crate) stop: TaskCancel,
    states: StdMutex<HashMap<String, JinniuRuntimeState>>,
    cancels: StdMutex<HashMap<String, (u64, TaskCancel)>>,
    session: StdMutex<Option<ActiveSession>>,
    generation: AtomicU64,
    app: StdMutex<Option<tauri::AppHandle>>,
}

impl JinniuRuntime {
    pub fn new() -> Self {
        Self {
            stop: TaskCancel::new(),
            states: StdMutex::new(HashMap::new()),
            cancels: StdMutex::new(HashMap::new()),
            session: StdMutex::new(None),
            generation: AtomicU64::new(0),
            app: StdMutex::new(None),
        }
    }

    pub fn set_app(&self, app: tauri::AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }

    /// 应用退出：中止所有在途登录与等待轮询。
    pub fn cancel_all(&self) {
        for (_, cancel) in self.cancels.lock().unwrap().values() {
            cancel.cancel();
        }
        *self.session.lock().unwrap() = None;
    }

    fn emit(&self, event: &str, payload: &impl Serialize) {
        let guard = self.app.lock().unwrap();
        if let Some(app) = guard.as_ref() {
            if let Err(e) = app.emit(event, payload) {
                tracing::warn!(event, error = %e, "tauri emit failed");
            }
        }
    }

    fn state_of(&self, id: &str) -> JinniuRuntimeState {
        self.states
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_else(JinniuRuntimeState::disconnected)
    }

    fn put(&self, id: &str, state: JinniuRuntimeState) {
        self.states.lock().unwrap().insert(id.to_string(), state);
    }

    /// 更新状态并广播。终态（disconnected/error）清掉该大户的取消令牌。
    fn set_status(&self, id: &str, status: JinniuStatus, error: Option<String>) {
        let mut state = self.state_of(id);
        state.status = status;
        state.error = error;
        if matches!(status, JinniuStatus::Disconnected | JinniuStatus::Error) {
            self.cancels.lock().unwrap().remove(id);
        }
        self.put(id, state);
    }

    fn patch(&self, id: &str, apply: impl FnOnce(&mut JinniuRuntimeState)) {
        let mut state = self.state_of(id);
        apply(&mut state);
        self.put(id, state);
    }

    fn is_login_in_flight(&self, id: &str) -> bool {
        self.cancels
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|(_, c)| !c.is_cancelled())
    }

    /// 登记/清除在途登录令牌，返回本次运行的 generation。
    fn begin_login(&self, id: &str) -> u64 {
        if let Some((_, previous)) = self.cancels.lock().unwrap().get(id) {
            previous.cancel();
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.cancels
            .lock()
            .unwrap()
            .insert(id.to_string(), (generation, TaskCancel::new()));
        generation
    }

    fn clear_login(&self, id: &str) {
        self.cancels.lock().unwrap().remove(id);
    }

    fn cancel_login(&self, id: &str) {
        if let Some((_, cancel)) = self.cancels.lock().unwrap().get(id) {
            cancel.cancel();
        }
    }

    fn current_generation(&self, id: &str) -> Option<u64> {
        self.cancels
            .lock()
            .unwrap()
            .get(id)
            .map(|(generation, _)| *generation)
    }

    /// 记录当前单活跃会话（覆盖旧值，旧会话由调用方负责关闭）。
    fn set_session(&self, session: ActiveSession) -> Option<ActiveSession> {
        self.session.lock().unwrap().replace(session)
    }

    /// 仅当槽位属于 `account_id` 时清空，返回被清空的会话。
    fn take_session_of(&self, account_id: &str) -> Option<ActiveSession> {
        let mut guard = self.session.lock().unwrap();
        if guard.as_ref().is_some_and(|s| s.account_id == account_id) {
            guard.take()
        } else {
            None
        }
    }

    fn clear_session(&self) {
        *self.session.lock().unwrap() = None;
    }
}

impl Default for JinniuRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn task_err(error: TaskError) -> MultizenError {
    MultizenError::Cdp(format!("金牛页面任务失败：{error}"))
}

// ── 驱动方法 ──────────────────────────────────────────────────────────────────

impl TauriBrowserDriver {
    /// 在 launcher 线程上执行一次 ProfileManager 操作（SQLite 不离开该线程）。
    pub(super) async fn jinniu_store<T: Send + 'static>(
        &self,
        op: impl FnOnce(&profile_manager::ProfileManager) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (resp, receive) = oneshot::channel();
        self.launcher_tx
            .send(LauncherCmd::JinniuDb {
                operation: Box::new(move |pm| {
                    if resp.is_closed() {
                        return;
                    }
                    let _ = resp.send(op(pm));
                }),
            })
            .await
            .map_err(|_| MultizenError::Mcp("金牛存储线程已关闭，请重启应用后重试".into()))?;
        receive
            .await
            .map_err(|_| MultizenError::Mcp("金牛存储响应已取消，请重试".into()))?
    }

    async fn jinniu_record(&self, id: &str) -> Result<JinniuAccountRecord> {
        let id = id.to_string();
        let records = self.jinniu_store(move |pm| pm.jinniu_accounts_list()).await?;
        records
            .into_iter()
            .find(|r| r.account.id == id)
            .ok_or_else(|| MultizenError::NotFound(format!("金牛大户 {id} 不存在，请刷新后重试")))
    }

    /// 列表（含运行时状态与活跃标记），按创建时间升序。
    pub async fn jinniu_list_accounts(&self) -> Result<Vec<JinniuAccountWithStatus>> {
        let records = self.jinniu_store(|pm| pm.jinniu_accounts_list()).await?;
        Ok(records
            .into_iter()
            .map(|record| self.project_jinniu_account(record))
            .collect())
    }

    /// 当前活跃大户 ID。
    pub async fn jinniu_get_active(&self) -> Result<Option<String>> {
        self.jinniu_store(|pm| pm.jinniu_account_active()).await
    }

    /// 当前大户状态 payload（未连接大户返回 disconnected 快照）。
    pub async fn jinniu_status(&self, id: &str) -> Result<JinniuStatePayload> {
        let record = self.jinniu_record(id).await?;
        Ok(self.jinniu_payload(id, &record.state))
    }

    /// 添加账户：不要求手输名称——缺省落「未命名金牛」占位行，登录识别到右上角
    /// 主账号后自动命名。一个大户 = 一个新 Profile + 一行
    /// `business_accounts(kind=jinniu)` + 一行金牛状态。第一个账户自动设为活跃。
    pub async fn jinniu_add_account(&self, label: Option<&str>) -> Result<JinniuAccountWithStatus> {
        let label = match label.map(str::trim) {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => JINNIU_PLACEHOLDER_LABEL.to_string(),
        };
        if label.chars().count() > 100 {
            return Err(MultizenError::Config("账户备注名最多100个字符".into()));
        }
        let profile = self
            .create_profile(multizen_core::CreateProfileInput {
                name: label.to_string(),
                ..Default::default()
            })
            .await?;
        let saved = match self
            .business_accounts_save(SaveBusinessAccountInput {
                id: None,
                profile_id: profile.id.clone(),
                kind: BusinessAccountKind::Jinniu,
                display_name: label.to_string(),
                platform_user_id: None,
            })
            .await
        {
            Ok(account) => account,
            Err(e) => {
                // 回滚：业务账号登记失败则删除刚建的 Profile，避免孤儿环境。
                let _ = self.delete_profile(&profile.id).await;
                return Err(e);
            }
        };
        let account_id = saved.id.clone();
        self.jinniu_store(move |pm| {
            pm.jinniu_account_ensure_state(&account_id)?;
            if pm.jinniu_account_active()?.is_none() {
                pm.jinniu_account_set_active(Some(&account_id))?;
            }
            Ok(())
        })
        .await?;
        self.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
        tracing::info!(account = %saved.id, label = %saved.display_name, "jinniu: 大户已添加");
        let account_id = saved.id.clone();
        let records = self.jinniu_store(move |pm| pm.jinniu_accounts_list()).await?;
        records
            .into_iter()
            .find(|r| r.account.id == account_id)
            .map(|record| self.project_jinniu_account(record))
            .ok_or_else(|| MultizenError::NotFound(format!("金牛大户 {account_id} 写入后未找到")))
    }

    /// 识别出主账号后的自动命名（best-effort，对齐伴侣账号「扫码后自动命名」）：
    /// 仅当显示名仍是占位别名时，用主账号名覆盖备注名并登记平台 ID；已命名过的
    /// 行保持不变。直改别名（不碰 profile 绑定），因此会话存活时也可命名；失败
    /// 只记日志，不影响连接状态。
    pub(crate) async fn jinniu_autoname(&self, id: &str, master_name: &str, master_id: &str) {
        let name = master_name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return;
        }
        let Ok(record) = self.jinniu_record(id).await else {
            return;
        };
        if record.account.display_name != JINNIU_PLACEHOLDER_LABEL {
            return;
        }
        let account_id = record.account.id;
        let name_for_db = name.to_string();
        let mid_for_db = master_id.trim().to_string();
        match self
            .jinniu_store(move |pm| {
                pm.business_account_rename(&account_id, &name_for_db, Some(&mid_for_db))
            })
            .await
        {
            Ok(_) => {
                self.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
                tracing::info!(account = %id, label = %name, "jinniu: 已用主账号名自动命名");
            }
            Err(e) => {
                tracing::warn!(account = %id, error = %e, "jinniu: 自动命名失败，保持占位名");
            }
        }
    }

    /// 删除大户：断开连接 → 删除业务账号 + 状态行 → 删除专属环境 → 活跃补位。
    pub async fn jinniu_remove_account(&self, id: &str) -> Result<()> {
        let record = self.jinniu_record(id).await?;
        // 先断开会话（关闭浏览器进程）。
        let _ = self.jinniu_disconnect(id).await;
        // 删除业务账号行（状态行随 FK 级联删除）。
        self.delete_business_account(id).await?;
        if let Some(profile_id) = record.account.profile_id {
            let _ = self.delete_profile(&profile_id).await;
        }
        self.jinniu.states.lock().unwrap().remove(id);
        self.jinniu_store(|pm| {
            pm.jinniu_account_ensure_active()?;
            Ok(())
        })
        .await?;
        self.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
        tracing::info!(account = %id, "jinniu: 大户已删除");
        Ok(())
    }

    /// 单选切换：断开旧活跃 → 设置新活跃（`None` 清空活跃）。
    pub async fn jinniu_set_active(&self, id: Option<String>) -> Result<Option<String>> {
        let current = self.jinniu_get_active().await?;
        if current == id {
            return Ok(current);
        }
        if let Some(old) = current.as_deref() {
            let _ = self.jinniu_disconnect(old).await;
        }
        let target = id.clone();
        self.jinniu_store(move |pm| pm.jinniu_account_set_active(target.as_deref()))
            .await?;
        self.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
        tracing::info!(from = ?current, to = ?id, "jinniu: active 已切换");
        Ok(id)
    }

    /// 断开当前会话（若属于该大户），状态归 `disconnected`。
    pub async fn jinniu_disconnect(&self, id: &str) -> Result<JinniuStatePayload> {
        self.jinniu.cancel_login(id);
        if let Some(session) = self.jinniu.take_session_of(id) {
            // 先尽力关掉本次登录打开的标签页，再关闭整个环境进程。
            if let Some(live) = self.registry.get(&session.profile_id).await {
                let _ = live.close_page(&session.target_id).await;
            }
            let _ = BrowserDriver::close(self, &session.profile_id).await;
        }
        self.jinniu.patch(id, |state| {
            state.master = None;
            state.current_sub_account_id = None;
            state.current_sub_account_name = None;
            state.balance_text = None;
        });
        self.jinniu.set_status(id, JinniuStatus::Disconnected, None);
        let record = self.jinniu_record(id).await?;
        let payload = self.jinniu_payload(id, &record.state);
        self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
        tracing::info!(account = %id, "jinniu: 已断开会话");
        Ok(payload)
    }

    /// 启动指定账户的金牛会话。状态迁移：connecting → (connected | awaiting-sub-account | error)。
    ///
    /// wire 状态机沿用 jieger 五态，但对外（UI）只有 启动中/已启动——connected 与
    /// awaiting-sub-account 都呈现为已启动：扫码完成即识别主账号并自动命名，
    /// 选子户不再是可见阶段（后台仍轮询 URL 捕获子户信息）。
    /// 启动即单选：其它存活的账户会话会被先停止，活跃标记切到本账户。
    pub async fn jinniu_login(
        self: &Arc<Self>,
        id: &str,
        options: JinniuLoginOptions,
    ) -> Result<JinniuStatePayload> {
        let record = self.jinniu_record(id).await?;
        let profile_id = record.account.profile_id.clone().ok_or_else(|| {
            MultizenError::Config(format!("金牛账户 {id} 未绑定环境，请删除后重新添加"))
        })?;
        if self.jinniu.is_login_in_flight(id) {
            return Err(MultizenError::Config("正在启动中，请稍候".into()));
        }

        // 启动即单选：停止其它存活的账户会话，并把活跃标记切到本账户。
        let current_active = self.jinniu_get_active().await?;
        if current_active.as_deref() != Some(id) {
            if let Some(old) = current_active.as_deref() {
                let _ = self.jinniu_disconnect(old).await;
            }
            let target = Some(id.to_string());
            self.jinniu_store(move |pm| pm.jinniu_account_set_active(target.as_deref()))
                .await?;
            self.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
        }

        let target_sub = target_sub_account_for_login(
            options.target_sub_account_id.as_deref(),
            record.state.last_sub_account_id.as_deref(),
        );
        let generation = self.jinniu.begin_login(id);
        // 入口重置上次连接残留的派生状态。
        self.jinniu.patch(id, |state| {
            state.status = JinniuStatus::Connecting;
            state.error = None;
            state.target_account_id = target_sub.clone();
            state.master = None;
            state.current_sub_account_id = None;
            state.current_sub_account_name = None;
            state.balance_text = None;
        });
        let payload = self.jinniu_payload(id, &record.state);
        self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);

        let outcome = self
            .jinniu_login_flow(id, &profile_id, target_sub, &options, &record.state)
            .await;
        // 仅清理在途标记；awaiting 阶段的轮询与单会话槽位需要保活。
        if self.jinniu.current_generation(id) == Some(generation) {
            self.jinniu.clear_login(id);
        }
        outcome
    }

    /// 登录主流程：启动 → 复用校验 → 扫码等待 → 登录后迁移。
    async fn jinniu_login_flow(
        self: &Arc<Self>,
        id: &str,
        profile_id: &str,
        target_sub: Option<String>,
        options: &JinniuLoginOptions,
        fallback: &JinniuAccountState,
    ) -> Result<JinniuStatePayload> {
        let startup_url = kuaishou::build_startup_url(target_sub.as_deref());
        tracing::info!(account = %id, target = ?target_sub, url = %startup_url, "jinniu: 开始登录");

        // 已在运行时复用现有会话（如 awaiting → 重新登录），否则启动环境。
        let session = match self.registry.get(profile_id).await {
            Some(session) => session,
            None => {
                // 真实登录始终以可见窗口打开：扫码与手动选子户都需要用户看到页面。
                if let Err(e) = BrowserDriver::launch(self.as_ref(), profile_id, false).await {
                    self.jinniu.set_status(id, JinniuStatus::Error, Some(e.to_string()));
                    let payload = self.jinniu_payload(id, fallback);
                    self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                    return Err(e);
                }
                self.registry.get(profile_id).await.ok_or_else(|| {
                    self.jinniu
                        .set_status(id, JinniuStatus::Error, Some("金牛环境会话不可用".into()));
                    MultizenError::Mcp("金牛环境会话不可用，请重试".into())
                })?
            }
        };

        // 打开金牛入口页（新标签；新启动浏览器残留的空白起始页会被顺带关闭）。
        let bound = session
            .new_bound_page_drop_blank(&startup_url)
            .await
            .map_err(|e| MultizenError::Cdp(format!("金牛入口页打开失败：{e}")))?;
        let target_id = bound.target_id().to_string();
        self.jinniu.set_session(ActiveSession {
            account_id: id.to_string(),
            profile_id: profile_id.to_string(),
            target_id: target_id.clone(),
        });
        // 把入口页带到前台：启动 / 扫码都需要用户看到页面。
        let _ = session.activate_page(&target_id).await;

        // 阶段一：storageState 复用校验（URL 判定）。
        let cancel = self
            .jinniu
            .cancels
            .lock()
            .unwrap()
            .get(id)
            .map(|(_, c)| c.clone())
            .unwrap_or_else(TaskCancel::new);
        let reused = kuaishou::connect_jinniu(
            &session,
            &target_id,
            &startup_url,
            cancel.clone(),
            KS_CONNECT_TIMEOUT,
        )
        .await
        .map_err(task_err)?;

        if reused {
            let url = Self::jinniu_current_url(&session, &target_id).await;
            if options.restore_only && !url.as_deref().is_some_and(url_has_account_id) {
                let _ = BrowserDriver::close(self.as_ref(), profile_id).await;
                self.jinniu.clear_session();
                self.jinniu.set_status(
                    id,
                    JinniuStatus::Disconnected,
                    Some("已保存登录态未进入子户".into()),
                );
                let payload = self.jinniu_payload(id, fallback);
                self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                return Ok(payload);
            }
            return Ok(self
                .jinniu_after_login(id, profile_id, &session, &target_id, fallback)
                .await);
        }

        if options.restore_only {
            let _ = BrowserDriver::close(self.as_ref(), profile_id).await;
            self.jinniu.clear_session();
            self.jinniu.set_status(
                id,
                JinniuStatus::Disconnected,
                Some("已保存登录态不可用".into()),
            );
            let payload = self.jinniu_payload(id, fallback);
            self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
            return Ok(payload);
        }

        // 阶段二：等待用户扫码（URL 离开登录页即视为扫码完成）。
        if let Err(e) = self
            .jinniu_wait_for_login(id, &session, &target_id, &cancel)
            .await
        {
            let (status, message) = classify_login_error(&e);
            self.jinniu.set_status(id, status, Some(message));
            let payload = self.jinniu_payload(id, fallback);
            self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
            return Ok(payload);
        }

        Ok(self
            .jinniu_after_login(id, profile_id, &session, &target_id, fallback)
            .await)
    }

    /// 扫码等待：轮询 URL，离开登录页即返回。
    ///
    /// 存活由取消令牌保证（新登录 begin_login 会取消旧令牌）；generation 守卫
    /// 不再需要——在途标记在登录返回后即被清理。
    async fn jinniu_wait_for_login(
        &self,
        id: &str,
        session: &Arc<BrowserSession>,
        target_id: &str,
        cancel: &TaskCancel,
    ) -> Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(JINNIU_LOGIN_TIMEOUT_MS);
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(MultizenError::Config("已取消".into())),
                _ = self.jinniu.stop.cancelled() => return Err(MultizenError::Config("已取消".into())),
                _ = tokio::time::sleep(Duration::from_millis(JINNIU_LOGIN_POLL_MS)) => {}
            }
            let Some(url) = Self::jinniu_current_url(session, target_id).await else {
                return Err(MultizenError::Cdp("浏览器已关闭".into()));
            };
            if !url_is_login_page(&url) {
                tracing::info!(account = %id, %url, "jinniu: 扫码完成（离开登录页）");
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MultizenError::Config("扫码登录超时".into()));
            }
        }
    }

    /// 登录后迁移：URL 判定 → connected / awaiting-sub-account（+ 后台轮询）。
    async fn jinniu_after_login(
        self: &Arc<Self>,
        id: &str,
        profile_id: &str,
        session: &Arc<BrowserSession>,
        target_id: &str,
        fallback: &JinniuAccountState,
    ) -> JinniuStatePayload {
        // 给页面一点稳定时间（导航/弹窗渲染较慢）。
        tokio::time::sleep(Duration::from_millis(500)).await;
        let url = Self::jinniu_current_url(session, target_id)
            .await
            .unwrap_or_default();
        match transition_after_login(&url) {
            Some((JinniuStatus::Connected, Some(sub_id))) => {
                self.jinniu_persist_sub(id, &sub_id).await;
                self.jinniu.patch(id, |state| {
                    state.status = JinniuStatus::Connected;
                    state.error = None;
                    state.current_sub_account_id = Some(sub_id.clone());
                });
                let payload = self.jinniu_payload(id, fallback);
                self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                self.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
                // connected 后 best-effort 捕获大户/子户信息（不阻塞）。
                self.spawn_jinniu_capture(id, session.clone(), target_id.to_string());
                // 登录即识别右上角主账号并自动命名（不依赖选子户）。
                self.spawn_jinniu_recognize(id, profile_id, session.clone(), target_id.to_string());
                tracing::info!(account = %id, sub = %sub_id, "jinniu: 已进入子户 → connected");
                payload
            }
            Some((JinniuStatus::AwaitingSubAccount, _)) | None => {
                self.jinniu.patch(id, |state| {
                    state.status = JinniuStatus::AwaitingSubAccount;
                    state.error = None;
                });
                let payload = self.jinniu_payload(id, fallback);
                self.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                self.spawn_jinniu_await_sub(id, profile_id, session.clone(), target_id.to_string());
                // 扫码完成即开始识别主账号（无需等用户选子户）。
                self.spawn_jinniu_recognize(id, profile_id, session.clone(), target_id.to_string());
                tracing::info!(account = %id, "jinniu: 进入 awaiting-sub-account，等待用户手动选子户");
                payload
            }
            Some((status, _)) => {
                // 不可达（transition 仅产出 connected / awaiting）。
                self.jinniu.patch(id, |state| state.status = status);
                self.jinniu_payload(id, fallback)
            }
        }
    }

    /// awaiting-sub-account 后台轮询：URL 出现 `__accountId__` → connected。
    ///
    /// 不挂在登录 generation 上：登录返回时在途标记会被清理，轮询的存活由
    /// 状态守卫（非 awaiting 即退出）+ 浏览器存活检查保证。
    fn spawn_jinniu_await_sub(
        self: &Arc<Self>,
        id: &str,
        profile_id: &str,
        session: Arc<BrowserSession>,
        target_id: String,
    ) {
        let driver = self.clone();
        let id = id.to_string();
        let profile_id = profile_id.to_string();
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now()
                + Duration::from_millis(JINNIU_AWAIT_SUB_TIMEOUT_MS);
            loop {
                tokio::select! {
                    biased;
                    _ = driver.jinniu.stop.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(JINNIU_AWAIT_POLL_MS)) => {}
                }
                if driver.jinniu.state_of(&id).status != JinniuStatus::AwaitingSubAccount {
                    return;
                }
                if driver.registry.get(&profile_id).await.is_none() {
                    driver.jinniu.set_status(&id, JinniuStatus::Disconnected, Some("浏览器已关闭".into()));
                    return;
                }
                let Some(url) = Self::jinniu_current_url(&session, &target_id).await else {
                    driver.jinniu.set_status(&id, JinniuStatus::Disconnected, Some("浏览器已关闭".into()));
                    return;
                };
                if let Some(sub_id) = kuaishou::extract_jinniu_account_id_from_url(&url) {
                    driver.jinniu_persist_sub(&id, &sub_id).await;
                    driver.jinniu.patch(&id, |state| {
                        state.status = JinniuStatus::Connected;
                        state.error = None;
                        state.current_sub_account_id = Some(sub_id.clone());
                    });
                    if let Ok(record) = driver.jinniu_record(&id).await {
                        let payload = driver.jinniu_payload(&id, &record.state);
                        driver.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                    }
                    driver.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
                    driver.spawn_jinniu_capture(&id, session.clone(), target_id.clone());
                    tracing::info!(account = %id, sub = %sub_id, "jinniu: 用户已进入子户 → connected");
                    return;
                }
                if tokio::time::Instant::now() >= deadline {
                    driver.jinniu.set_status(&id, JinniuStatus::Error, Some("等待手动进入子户超时".into()));
                    if let Ok(record) = driver.jinniu_record(&id).await {
                        let payload = driver.jinniu_payload(&id, &record.state);
                        driver.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                    }
                    return;
                }
            }
        });
    }

    /// 登录后识别：轮询页面文本直到解析出右上角主账号（用户名 + 快手ID），回写
    /// 缓存并自动命名。扫码完成 ≠ 页面渲染完成，首页顶栏加载慢时多试几次；
    /// 识别失败不影响连接状态机，子户/余额由 connected 后的
    /// [`Self::spawn_jinniu_capture`] 负责。
    fn spawn_jinniu_recognize(
        self: &Arc<Self>,
        id: &str,
        profile_id: &str,
        session: Arc<BrowserSession>,
        target_id: String,
    ) {
        let driver = self.clone();
        let id = id.to_string();
        let profile_id = profile_id.to_string();
        tokio::spawn(async move {
            // ~30 × 2s = 60s 预算；浏览器关闭 / 会话终止则提前退出。
            for attempt in 0..30u32 {
                if attempt > 0 {
                    tokio::select! {
                        biased;
                        _ = driver.jinniu.stop.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_millis(2_000)) => {}
                    }
                }
                if matches!(
                    driver.jinniu.state_of(&id).status,
                    JinniuStatus::Disconnected | JinniuStatus::Error
                ) {
                    return;
                }
                let text = match Self::jinniu_page_text(&session, &target_id).await {
                    Some(text) => text,
                    None => {
                        if driver.registry.get(&profile_id).await.is_none() {
                            return;
                        }
                        continue;
                    }
                };
                let Some((name, mid)) = parse_master_info(&text) else {
                    continue;
                };
                let account_id = id.clone();
                let name_for_db = name.clone();
                let mid_for_db = mid.clone();
                let _ = driver
                    .jinniu_store(move |pm| {
                        pm.jinniu_account_update_master(&account_id, &name_for_db, &mid_for_db, None)
                    })
                    .await;
                driver.jinniu_autoname(&id, &name, &mid).await;
                driver.jinniu.patch(&id, |state| {
                    state.master = Some(JinniuMaster {
                        name: name.clone(),
                        id: mid.clone(),
                        avatar_url: None,
                    });
                });
                if let Ok(record) = driver.jinniu_record(&id).await {
                    let payload = driver.jinniu_payload(&id, &record.state);
                    driver.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
                }
                tracing::info!(account = %id, master = %name, "jinniu: 已识别右上角主账号");
                return;
            }
            tracing::warn!(account = %id, "jinniu: 主账号识别超时（不影响连接状态）");
        });
    }

    /// connected 后 best-effort 捕获大户/子户信息并回写 DB（失败不影响状态）。
    fn spawn_jinniu_capture(self: &Arc<Self>, id: &str, session: Arc<BrowserSession>, target_id: String) {
        let driver = self.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            let Some(bound) = session.bind_page(&target_id).await.ok() else {
                return;
            };
            let Ok(value) = bound.evaluate(JINNIU_PAGE_TEXT_JS).await else {
                return;
            };
            let Some(text) = value.as_str() else { return };
            let master = parse_master_info(text);
            let sub = parse_sub_info(text);
            let balance = parse_balance(text);
            let account_id = id.clone();
            let _ = driver
                .jinniu_store(move |pm| {
                    if let Some((name, mid)) = master {
                        pm.jinniu_account_update_master(&account_id, &name, &mid, None)?;
                    }
                    if let Some((name, sid)) = sub {
                        pm.jinniu_account_update_sub(&account_id, &sid, Some(&name))?;
                    }
                    Ok(())
                })
                .await;
            driver.jinniu.patch(&id, |state| {
                if let Some((name, mid)) = parse_master_info(text) {
                    state.master = Some(JinniuMaster {
                        name,
                        id: mid,
                        avatar_url: None,
                    });
                }
                if let Some((name, sid)) = parse_sub_info(text) {
                    state.current_sub_account_id = Some(sid);
                    state.current_sub_account_name = Some(name);
                }
                state.balance_text = balance;
            });
            if let Ok(record) = driver.jinniu_record(&id).await {
                let payload = driver.jinniu_payload(&id, &record.state);
                driver.jinniu.emit(JINNIU_STATUS_CHANGED, &payload);
            }
            driver.jinniu.emit(JINNIU_ACCOUNTS_CHANGED, &serde_json::json!({}));
        });
    }

    async fn jinniu_persist_sub(&self, id: &str, sub_id: &str) {
        let id = id.to_string();
        let sub_id = sub_id.to_string();
        let _ = self
            .jinniu_store(move |pm| pm.jinniu_account_update_sub(&id, &sub_id, None))
            .await;
    }

    /// 读取页面当前 URL（best-effort；失败返回 `None`）。
    async fn jinniu_current_url(session: &Arc<BrowserSession>, target_id: &str) -> Option<String> {
        let bound = session.bind_page(target_id).await.ok()?;
        let value = bound.evaluate("location.href").await.ok()?;
        value.as_str().map(str::to_string)
    }

    /// 读取页面可见文本（best-effort；失败返回 `None`）。
    async fn jinniu_page_text(session: &Arc<BrowserSession>, target_id: &str) -> Option<String> {
        let bound = session.bind_page(target_id).await.ok()?;
        let value = bound.evaluate(JINNIU_PAGE_TEXT_JS).await.ok()?;
        value.as_str().map(str::to_string)
    }

    /// 组装运行时 + DB 缓存合并后的状态 payload。
    fn jinniu_payload(&self, id: &str, fallback: &JinniuAccountState) -> JinniuStatePayload {
        let state = self.jinniu.state_of(id);
        let master = state.master.or_else(|| match (&fallback.master_name, &fallback.master_id) {
            (Some(name), Some(mid)) => Some(JinniuMaster {
                name: name.clone(),
                id: mid.clone(),
                avatar_url: fallback.master_avatar_url.clone(),
            }),
            _ => None,
        });
        JinniuStatePayload {
            account_id: id.to_string(),
            status: state.status,
            target_account_id: state.target_account_id,
            error: state.error,
            master,
            current_sub_account_id: state
                .current_sub_account_id
                .or_else(|| fallback.last_sub_account_id.clone()),
            current_sub_account_name: state
                .current_sub_account_name
                .or_else(|| fallback.last_sub_account_name.clone()),
            balance_text: state.balance_text,
        }
    }

    fn project_jinniu_account(&self, record: JinniuAccountRecord) -> JinniuAccountWithStatus {
        let payload = self.jinniu_payload(&record.account.id, &record.state);
        self.merge_status(record.account, record.state.is_active, payload)
    }

    fn merge_status(
        &self,
        account: BusinessAccount,
        is_active: bool,
        payload: JinniuStatePayload,
    ) -> JinniuAccountWithStatus {
        JinniuAccountWithStatus {
            id: account.id,
            profile_id: account.profile_id,
            label: account.display_name,
            status: payload.status,
            is_active,
            error: payload.error,
            target_account_id: payload.target_account_id,
            master_name: payload.master.as_ref().map(|m| m.name.clone()),
            master_id: payload.master.as_ref().map(|m| m.id.clone()),
            master_avatar_url: payload.master.as_ref().and_then(|m| m.avatar_url.clone()),
            current_sub_account_id: payload.current_sub_account_id,
            current_sub_account_name: payload.current_sub_account_name,
            balance_text: payload.balance_text,
        }
    }

    /// 应用退出：中止金牛登录/轮询。
    pub fn stop_jinniu(&self) {
        self.jinniu.stop.cancel();
        self.jinniu.cancel_all();
    }
}

/// 把登录流程错误映射为状态机终态（jieger catch 分支语义）。
fn classify_login_error(error: &MultizenError) -> (JinniuStatus, String) {
    let message = error.to_string();
    if message.contains("已取消") {
        return (JinniuStatus::Disconnected, "已取消".into());
    }
    if message.contains("浏览器") || message.contains("page has been closed") {
        return (JinniuStatus::Disconnected, "浏览器已关闭".into());
    }
    if message.contains("超时") {
        return (JinniuStatus::Error, "登录超时，请重试".into());
    }
    (JinniuStatus::Error, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn status_wire_values_match_jieger() {
        let cases = [
            (JinniuStatus::Disconnected, "disconnected"),
            (JinniuStatus::Connecting, "connecting"),
            (JinniuStatus::AwaitingSubAccount, "awaiting-sub-account"),
            (JinniuStatus::Connected, "connected"),
            (JinniuStatus::Error, "error"),
        ];
        for (status, wire) in cases {
            assert_eq!(serde_json::to_value(status).unwrap(), serde_json::json!(wire));
            assert_eq!(
                serde_json::from_value::<JinniuStatus>(serde_json::json!(wire)).unwrap(),
                status
            );
            assert_eq!(status.as_str(), wire);
        }
    }

    #[test]
    fn url_verdict_separates_scan_from_connected() {
        let login = "https://niu.e.kuaishou.com/login";
        let home = "https://niu.e.kuaishou.com/home?homeType=new";
        let sub = "https://niu.e.kuaishou.com/home?__accountId__=999&homeType=new";

        // 登录页：仍需扫码，不迁移。
        assert!(url_is_login_page(login));
        assert_eq!(verdict_from_url(login), None);
        assert_eq!(transition_after_login(login), None);

        // 已离开登录页但无子户：awaiting-sub-account（扫码完成 ≠ connected）。
        assert!(!url_has_account_id(home));
        assert_eq!(verdict_from_url(home), Some(None));
        assert_eq!(
            transition_after_login(home),
            Some((JinniuStatus::AwaitingSubAccount, None))
        );

        // 用户手动选子户后 URL 含 __accountId__：connected。
        assert!(url_has_account_id(sub));
        assert_eq!(
            transition_after_login(sub),
            Some((JinniuStatus::Connected, Some("999".into())))
        );
    }

    #[test]
    fn target_sub_account_priority_and_trimming() {
        assert_eq!(target_sub_account_for_login(Some(" 5 "), Some("9")), Some("5".into()));
        assert_eq!(target_sub_account_for_login(Some("   "), Some("9")), Some("9".into()));
        assert_eq!(target_sub_account_for_login(None, Some(" 9 ")), Some("9".into()));
        assert_eq!(target_sub_account_for_login(None, None), None);
        assert_eq!(target_sub_account_for_login(Some(""), Some("")), None);
    }

    #[test]
    fn page_text_parsers_extract_master_sub_and_balance() {
        let text = "登录用户信息 用户名 张三 快手ID 123456 当前操作的广告账户信息 \
                    账户名 甲子户 账户ID 999 账户可用余额 ¥1,234.56";
        assert_eq!(parse_master_info(text), Some(("张三".into(), "123456".into())));
        assert_eq!(parse_sub_info(text), Some(("甲子户".into(), "999".into())));
        assert_eq!(parse_balance(text), Some("¥1,234.56".into()));
        assert_eq!(parse_master_info("没有账号信息"), None);
        assert_eq!(parse_balance("没有余额"), None);
    }

    #[test]
    fn login_options_deserialize_with_defaults() {
        let options: JinniuLoginOptions =
            serde_json::from_value(serde_json::json!({"targetSubAccountId": "9"})).unwrap();
        assert_eq!(options.target_sub_account_id.as_deref(), Some("9"));
        assert!(!options.headless);
        assert!(!options.restore_only);
        let empty: JinniuLoginOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.target_sub_account_id.is_none());
    }

    #[test]
    fn runtime_single_session_slot_is_exclusive_and_scoped() {
        let runtime = JinniuRuntime::new();
        assert!(runtime.take_session_of("a").is_none());
        let replaced = runtime.set_session(ActiveSession {
            account_id: "a".into(),
            profile_id: "pa".into(),
            target_id: "t1".into(),
        });
        assert!(replaced.is_none());
        // 切换大户：覆盖旧会话槽位（旧会话由调用方关闭）。
        let replaced = runtime.set_session(ActiveSession {
            account_id: "b".into(),
            profile_id: "pb".into(),
            target_id: "t2".into(),
        });
        assert_eq!(replaced.unwrap().account_id, "a");
        // 只有槽位归属者可清空。
        assert!(runtime.take_session_of("a").is_none());
        assert_eq!(runtime.take_session_of("b").unwrap().target_id, "t2");
        runtime.clear_session();
    }

    #[test]
    fn runtime_status_transitions_and_generation_guard() {
        let runtime = JinniuRuntime::new();
        assert_eq!(runtime.state_of("a").status, JinniuStatus::Disconnected);
        runtime.set_status("a", JinniuStatus::Connecting, None);
        assert_eq!(runtime.state_of("a").status, JinniuStatus::Connecting);
        runtime.set_status("a", JinniuStatus::AwaitingSubAccount, None);
        runtime.set_status("a", JinniuStatus::Connected, None);
        assert_eq!(runtime.state_of("a").status, JinniuStatus::Connected);
        // 终态清掉在途登录令牌。
        let generation = runtime.begin_login("a");
        assert!(runtime.is_login_in_flight("a"));
        assert_eq!(runtime.current_generation("a"), Some(generation));
        runtime.set_status("a", JinniuStatus::Error, Some("boom".into()));
        assert!(!runtime.is_login_in_flight("a"));
        assert_eq!(runtime.state_of("a").error.as_deref(), Some("boom"));
    }

    #[test]
    fn classify_login_error_maps_terminal_states() {
        assert_eq!(
            classify_login_error(&MultizenError::Config("已取消".into())).0,
            JinniuStatus::Disconnected
        );
        assert_eq!(
            classify_login_error(&MultizenError::Cdp("浏览器已关闭".into())).0,
            JinniuStatus::Disconnected
        );
        assert_eq!(
            classify_login_error(&MultizenError::Config("扫码登录超时".into())),
            (JinniuStatus::Error, "登录超时，请重试".into())
        );
        assert_eq!(
            classify_login_error(&MultizenError::Mcp("其它错误".into())),
            (JinniuStatus::Error, "mcp error: 其它错误".into())
        );
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
            "jinniu_accounts_list",
            "jinniu_account_add",
            "jinniu_account_remove",
            "jinniu_account_set_active",
            "jinniu_account_get_active",
            "jinniu_login",
            "jinniu_disconnect",
            "jinniu_status",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
        let adapter = include_str!("../commands/jinniu.rs");
        assert_eq!(adapter.matches("\n#[tauri::command]").count(), 8);
    }

    /// 离线 CRUD：不启动浏览器、不执行真实登录。
    #[tokio::test]
    async fn crud_add_list_switch_remove_offline() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        assert!(driver.jinniu_list_accounts().await.unwrap().is_empty());
        assert!(driver.jinniu_get_active().await.unwrap().is_none());

        let first = driver.jinniu_add_account(Some("大户甲")).await.unwrap();
        assert_eq!(first.label, "大户甲");
        assert!(first.is_active, "第一个大户自动活跃");
        assert_eq!(first.status, JinniuStatus::Disconnected);
        assert!(first.profile_id.is_some(), "一个大户 = 一个专属环境");

        let second = driver.jinniu_add_account(Some("大户乙")).await.unwrap();
        assert!(!second.is_active, "第二个大户不自动活跃");

        let listed = driver.jinniu_list_accounts().await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed.iter().filter(|a| a.is_active).count(), 1);

        // 单选切换：旧活跃自动失活。
        driver
            .jinniu_set_active(Some(second.id.clone()))
            .await
            .unwrap();
        let listed = driver.jinniu_list_accounts().await.unwrap();
        let active: Vec<_> = listed.iter().filter(|a| a.is_active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, second.id);
        assert_eq!(
            driver.jinniu_get_active().await.unwrap().as_deref(),
            Some(second.id.as_str())
        );

        // 未登录状态下 status 为 disconnected，且不触发任何浏览器动作。
        let status = driver.jinniu_status(&first.id).await.unwrap();
        assert_eq!(status.status, JinniuStatus::Disconnected);
        assert!(status.current_sub_account_id.is_none());

        // 删除活跃大户 → 自动补位到剩余大户。
        driver.jinniu_remove_account(&second.id).await.unwrap();
        let listed = driver.jinniu_list_accounts().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, first.id);
        assert!(listed[0].is_active, "删除活跃大户后自动补位");

        // 删除最后一个 → 活跃清空。
        driver.jinniu_remove_account(&first.id).await.unwrap();
        assert!(driver.jinniu_list_accounts().await.unwrap().is_empty());
        assert!(driver.jinniu_get_active().await.unwrap().is_none());

        driver.shutdown().await;
    }

    /// 未知大户 / 非金牛账号的登录与状态请求必须被拒绝（不做任何浏览器动作）。
    #[tokio::test]
    async fn login_and_status_reject_unknown_accounts() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let driver = Arc::new(driver);
        assert!(driver.jinniu_status("ghost").await.is_err());
        assert!(driver
            .jinniu_login("ghost", JinniuLoginOptions::default())
            .await
            .is_err());
        // 删除未知大户同样报错。
        assert!(driver.jinniu_remove_account("ghost").await.is_err());

        let profile = driver
            .create_profile(multizen_core::CreateProfileInput {
                name: "shop".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let shop = driver
            .business_accounts_save(SaveBusinessAccountInput {
                id: None,
                profile_id: profile.id.clone(),
                kind: BusinessAccountKind::KuaishouShop,
                display_name: "Shop".into(),
                platform_user_id: None,
            })
            .await
            .unwrap();
        // 非金牛账号不能作为大户登录，也不会出现在金牛列表。
        assert!(driver.jinniu_login(&shop.id, JinniuLoginOptions::default()).await.is_err());
        assert!(driver.jinniu_list_accounts().await.unwrap().is_empty());
        driver.shutdown().await;
    }

    /// 不手输名称 → 落「未命名金牛」占位行（对齐伴侣账号）；超长备注名仍拒绝
    /// 且不留孤儿环境。
    #[tokio::test]
    async fn add_without_label_uses_placeholder_and_rejects_oversize() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let created = driver.jinniu_add_account(None).await.unwrap();
        assert_eq!(created.label, JINNIU_PLACEHOLDER_LABEL);
        assert_eq!(created.label, "未命名金牛");
        assert!(created.profile_id.is_some(), "一个账户 = 一个专属环境");
        assert!(driver
            .jinniu_add_account(Some(&"x".repeat(101)))
            .await
            .is_err());
        assert_eq!(driver.list_profiles().await.unwrap().len(), 1);
        driver.shutdown().await;
    }

    /// 自动命名：仅覆盖占位行（识别到主账号名后）；已命名的行保持不变。
    #[tokio::test]
    async fn autoname_overwrites_placeholder_only() {
        let (_dir, driver) =
            super::super::business_tests::fixture(multizen_core::ChromixSettings::default());
        let first = driver.jinniu_add_account(None).await.unwrap();
        driver.jinniu_autoname(&first.id, "张三", "123456").await;
        let listed = driver.jinniu_list_accounts().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "张三");
        // 二次识别不覆盖已命名的行。
        driver.jinniu_autoname(&first.id, "李四", "654321").await;
        let listed = driver.jinniu_list_accounts().await.unwrap();
        assert_eq!(listed[0].label, "张三");
        driver.shutdown().await;
    }
}
