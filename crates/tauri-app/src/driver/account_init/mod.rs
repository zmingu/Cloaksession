//! Typed launcher-thread bridge and local archive operations.
mod attachments;
mod runtime;
mod page;
use super::{LauncherCmd, TauriBrowserDriver};
use crate::registry::{ProfileRegistry, SessionSlot};
use multizen_core::*;
use profile_manager::ProfileManager;
use std::{sync::Arc, time::{Duration, Instant}};
use tokio::sync::oneshot;
pub(super) use runtime::InitRuntime;

#[derive(Clone)]
struct Guard { context: KuaishouInitContext, slot: Arc<SessionSlot> }
/// Runs on the launcher thread; the bool is "context still valid and within deadline".
type InitOperation = Box<dyn FnOnce(&ProfileManager, bool) + Send>;
pub(super) struct InitCmd {
    guard: Option<Guard>,
    deadline: Instant,
    operation: InitOperation,
}
fn error(message: &str) -> MultizenError { MultizenError::Mcp(message.into()) }
fn validate(pm: &ProfileManager, context: &KuaishouInitContext) -> Result<()> {
    if pm.get(&context.profile_id)?.is_none() || pm.business_accounts_profile_state(&context.profile_id)? != context.expected_business || !kuaishou_identity_allowed(&context.expected_business) { return Err(error("环境或业务绑定已变化")); }
    let observed = pm.kuaishou_identity_observation(&context.profile_id)?.ok_or_else(|| error("当前身份尚未可信检测"))?;
    if observed.session_id.as_deref() != Some(context.session_id.as_str()) || observed.snapshot.status != KuaishouIdentityStatus::Detected || observed.snapshot.platform_user_id.as_deref() != Some(context.platform_user_id.as_str()) || context.expected_business.account.as_ref().and_then(|a| a.platform_user_id.as_deref()).is_some_and(|id| id != context.platform_user_id) { return Err(error("当前身份已变化或存在冲突")); }
    Ok(())
}
pub(super) async fn handle(cmd: InitCmd, pm: &ProfileManager, registry: &ProfileRegistry, launcher: &browser_launcher::BrowserLauncher) {
    let InitCmd { guard, deadline, operation } = cmd;
    if let Some(g) = guard {
        if !launcher.is_running_async(&g.context.profile_id).await { registry.remove(&g.context.profile_id).await; }
        let mut operation = Some(operation);
        registry.with_current(&g.context.profile_id, &g.slot, || {
            let good = Instant::now() < deadline && validate(pm, &g.context).is_ok();
            operation.take().unwrap()(pm, good);
        }).await;
        if let Some(operation) = operation { operation(pm, false); }
    } else { operation(pm, Instant::now() < deadline); }
}
impl TauriBrowserDriver {
    async fn init_db<T: Send + 'static>(&self, guard: Option<Guard>, operation: impl FnOnce(&ProfileManager) -> Result<T> + Send + 'static) -> Result<T> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let (resp, receive) = oneshot::channel();
        let command = InitCmd { guard, deadline, operation: Box::new(move |pm, valid| {
            if resp.is_closed() { return; }
            let result = if valid { operation(pm) } else { Err(error("初始化上下文已失效或存储等待超时")) };
            let _ = resp.send(result);
        }) };
        tokio::time::timeout_at(deadline.into(), async {
            self.launcher_tx.send(LauncherCmd::Init(command)).await.map_err(|_| error("初始化存储线程不可用"))?;
            receive.await.map_err(|_| error("初始化存储响应已取消"))?
        }).await.map_err(|_| error("初始化存储等待超时"))?
    }
    pub async fn kuaishou_subject_list(&self, query: KuaishouSubjectQuery) -> Result<KuaishouSubjectPage> { self.init_db(None, move |pm| pm.kuaishou_subject_list(&query)).await }
    pub async fn kuaishou_subject_detail(&self, id: String) -> Result<Option<KuaishouSubjectDetail>> { self.init_db(None, move |pm| pm.kuaishou_subject_detail(&id)).await }
    pub async fn kuaishou_subject_correct(&self, input: CorrectSubjectInput) -> Result<KuaishouSubjectArchive> { self.init_db(None, move |pm| pm.kuaishou_subject_correct(input)).await }
    pub async fn kuaishou_init_steps(&self, id: String) -> Result<Vec<KuaishouInitStepRecord>> { self.init_db(None, move |pm| pm.kuaishou_init_steps(&id)).await }
    async fn subject_revision(&self, id: String, revision: u64) -> Result<KuaishouSubjectArchive> {
        let archive = self.kuaishou_subject_detail(id).await?.ok_or_else(|| error("主体档案不存在"))?.archive;
        if archive.revision != revision { return Err(error("资料版本已变化，请刷新后重试")); }
        Ok(archive)
    }
    pub async fn kuaishou_subject_confirm(&self, input: ConfirmSubjectInput) -> Result<KuaishouSubjectArchive> {
        let archive = self.subject_revision(input.platform_user_id.clone(), input.expected_revision).await?;
        let verified = self.account_init.cache.verify(&archive.attachments, Instant::now() + Duration::from_secs(30)).await.map_err(error)?;
        self.init_db(None, move |pm| pm.kuaishou_subject_confirm(input, &verified)).await
    }
    pub async fn kuaishou_subject_attachment(&self, key: String) -> Result<String> {
        let check = key.clone();
        if !self.init_db(None, move |pm| pm.kuaishou_subject_attachment_referenced(&check)).await? { return Err(error("附件未被档案引用")); }
        let (meta, bytes) = self.account_init.cache.read(&key, Instant::now() + Duration::from_secs(15)).await.map_err(error)?;
        Ok(attachments::AttachmentCache::data_url(&meta, &bytes))
    }
    pub async fn kuaishou_subject_reocr(&self, id: String, revision: u64) -> Result<KuaishouSubjectArchive> {
        let archive = self.subject_revision(id.clone(), revision).await?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let (name, card) = self.recognize_attachments(&archive.attachments, deadline).await?;
        let verified = self.account_init.cache.verify(&archive.attachments, deadline).await.map_err(error)?;
        self.init_db(None, move |pm| pm.kuaishou_subject_update_ocr(CorrectSubjectInput { platform_user_id: id, expected_revision: revision, real_name: name, id_card: card }, &verified)).await
    }
    async fn recognize_attachments(&self, attachments: &[SubjectAttachment], deadline: Instant) -> Result<(String, String)> {
        self.account_init.cache.verify(attachments, deadline).await.map_err(error)?;
        let mut names = std::collections::BTreeSet::new(); let mut cards = std::collections::BTreeSet::new();
        for attachment in attachments {
            let (actual, bytes) = self.account_init.cache.read(&attachment.key, deadline).await.map_err(error)?;
            if actual != *attachment { return Err(error("证件附件版本变化")); }
            match local_ocr::recognize(bytes, deadline).await {
                Ok(output) => { names.extend(output.candidates.names); cards.extend(output.candidates.id_numbers); }
                Err(local_ocr::OcrError::EmptyRecognition) => {},
                Err(_) => return Err(error("本地OCR不可用或识别失败，请检查系统中文资源")),
            }
        }
        if names.len() != 1 || cards.len() != 1 { return Err(error("OCR候选为空或存在歧义，请人工核对修正")); }
        Ok((names.into_iter().next().unwrap(), cards.into_iter().next().unwrap()))
    }
}
