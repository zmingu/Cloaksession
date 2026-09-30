use super::{attachments::AttachmentCache, error, page, Guard, TauriBrowserDriver};
use base64::{engine::general_purpose::STANDARD, Engine};
use cdp_driver::{TaskCancel, TaskPage};
use multizen_core::*;
use profile_manager::KuaishouInitLease;
use std::{collections::{HashMap, HashSet}, path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use KuaishouInitErrorCode as Code;

pub(in crate::driver) struct InitRuntime {
    pub(super) cache: AttachmentCache,
    pub(in crate::driver) stop: TaskCancel,
    started: AtomicBool,
    active: Mutex<HashSet<String>>,
    // None means a create request had unknown completion: never create another automatically.
    pages: Mutex<HashMap<String, Option<String>>>,
    runs: Mutex<HashMap<String, u32>>,
}
impl InitRuntime {
    pub fn new(root: PathBuf) -> Self { Self { cache: AttachmentCache::new(root), stop: TaskCancel::new(), started: AtomicBool::new(false), active: Mutex::new(HashSet::new()), pages: Mutex::new(HashMap::new()), runs: Mutex::new(HashMap::new()) } }
}
struct Active { runtime: Arc<InitRuntime>, id: String }
impl Drop for Active { fn drop(&mut self) { self.runtime.active.lock().unwrap().remove(&self.id); } }
impl TauriBrowserDriver {
    fn init_enter(&self, id: &str) -> Option<Active> {
        if self.account_init.stop.is_cancelled() { return None; }
        let mut active = self.account_init.active.lock().unwrap();
        if active.len() >= 4 || !active.insert(id.into()) { return None; }
        Some(Active { runtime: self.account_init.clone(), id: id.into() })
    }
    async fn init_context(&self, profile_id: &str) -> Result<Guard> {
        let slot = self.registry.slot(profile_id).await.ok_or_else(|| error("环境未运行或会话未就绪"))?;
        let id = profile_id.to_string(); let session_id = slot.id.clone();
        let context = self.init_db(None, move |pm| {
            let o = pm.kuaishou_identity_observation(&id)?.ok_or_else(|| error("当前身份尚未可信检测"))?;
            let fresh = o.snapshot.checked_at.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).is_some_and(|t| {
                let age = chrono::Utc::now().signed_duration_since(t); age.num_seconds() >= 0 && age.num_seconds() <= 30
            });
            if !fresh || o.session_id.as_deref() != Some(session_id.as_str()) || o.snapshot.status != KuaishouIdentityStatus::Detected { return Err(error("当前身份观察不新鲜，请先重新检测")); }
            let context = KuaishouInitContext { platform_user_id: o.snapshot.platform_user_id.ok_or_else(|| error("当前账号不可信"))?, profile_id: id.clone(), session_id, expected_business: pm.business_accounts_profile_state(&id)? };
            super::validate(pm, &context)?; Ok(context)
        }).await?;
        let guard = Guard { context, slot };
        self.validate_init(&guard).await.map_err(|_| error("当前环境已变化"))?;
        // Read all existing shop targets afresh. Conflicting pages must never select an account.
        let session = guard.slot.session().ok_or_else(|| error("会话已关闭"))?;
        let observed = tokio::time::timeout(Duration::from_secs(8), self.current_kuaishou_account(&session, guard.slot.cancel.clone())).await.map_err(|_| error("当前账号重验超时"))??;
        if observed != guard.context.platform_user_id { return Err(error("当前页面身份冲突或已换号")); }
        Ok(guard)
    }
    pub(super) async fn validate_init(&self, guard: &Guard) -> std::result::Result<(), Code> {
        if self.account_init.stop.is_cancelled() || guard.slot.cancel.is_cancelled() { return Err(Code::ContextChanged); }
        self.init_db(Some(guard.clone()), |_| Ok(())).await.map_err(|_| Code::ContextChanged)
    }
    pub async fn kuaishou_init_retry(self: &Arc<Self>, profile_id: String) -> Result<()> {
        // Explicit write-capable entry; the preexisting detect/list remain read-only.
        let guard = self.init_context(&profile_id).await?;
        let active = self.init_enter(&profile_id).ok_or_else(|| error("初始化正在进行或并发已满"))?;
        let id = guard.context.platform_user_id.clone();
        self.init_db(Some(guard.clone()), move |pm| pm.kuaishou_init_retry_failed(&id)).await?;
        self.account_init.runs.lock().unwrap().remove(&guard.slot.id);
        let driver = self.clone();
        tauri::async_runtime::spawn(async move { driver.init_run(guard, active).await; });
        Ok(())
    }
    pub fn start_kuaishou_init_monitor(self: &Arc<Self>) {
        if self.account_init.started.swap(true, Ordering::AcqRel) { return; }
        let weak = Arc::downgrade(self); let stop = self.account_init.stop.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased; _ = stop.cancelled() => break, _ = interval.tick() => {} }
                let Some(driver) = weak.upgrade() else { break; };
                for id in driver.registry.ids().await {
                    let Some(active) = driver.init_enter(&id) else { continue; };
                    let driver = driver.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(guard) = driver.init_context(&id).await { driver.init_run(guard, active).await; }
                    });
                }
            }
        });
    }
    pub fn stop_kuaishou_init_monitor(&self) { self.account_init.stop.cancel(); }
    async fn init_run(&self, guard: Guard, _active: Active) {
        if *self.account_init.runs.lock().unwrap().get(&guard.slot.id).unwrap_or(&0) >= 3 { return; }
        let mut attempted = false; let mut failed = false;
        for step in [KuaishouInitStep::Subject, KuaishouInitStep::Slice] {
            if self.validate_init(&guard).await.is_err() { break; }
            let ctx = guard.context.clone();
            let lease = match self.init_db(Some(guard.clone()), move |pm| pm.kuaishou_init_claim(&ctx, step)).await {
                Ok(Some(lease)) => lease, _ => continue,
            };
            attempted = true;
            let deadline = Instant::now() + Duration::from_secs(100);
            let result = tokio::select! {
                biased;
                _ = self.account_init.stop.cancelled() => Err(Code::InterruptedNeedsVerification),
                _ = guard.slot.cancel.cancelled() => Err(Code::InterruptedNeedsVerification),
                result = tokio::time::timeout_at(deadline.into(), self.init_step(&guard, &lease, deadline)) => result.unwrap_or(Err(Code::TimedOut)),
            };
            if let Err(code) = result {
                failed = true;
                let seconds = 20u32.saturating_mul(1u32 << lease.attempts().min(4));
                // Releasing only the original token is allowed after session invalidation.
                // The operation future and TaskPage are dropped before releasing its DB lease.
                let _ = self.init_db(None, move |pm| pm.kuaishou_init_fail(&lease, code, seconds)).await;
                if matches!(code, Code::InterruptedNeedsVerification | Code::TimedOut | Code::ContextChanged) { break; }
            }
        }
        if attempted { *self.account_init.runs.lock().unwrap().entry(guard.slot.id.clone()).or_default() += 1; }
        if attempted && !failed && self.validate_init(&guard).await.is_ok() {
            let id = guard.context.platform_user_id.clone();
            let done = self.init_db(Some(guard.clone()), move |pm| Ok(pm.kuaishou_init_steps(&id)?.iter().filter(|s| s.state == KuaishouInitState::Done).count() == 2)).await.unwrap_or(false);
            if done {
                let target = self.account_init.pages.lock().unwrap().get(&guard.slot.id).cloned().flatten();
                if let (Some(target), Some(session)) = (target, guard.slot.session()) {
                    // Only this runtime's self-created target can be closed. Never user pages.
                    if tokio::time::timeout(Duration::from_secs(5), session.close_page(&target)).await.is_ok_and(|r| r.is_ok()) { self.account_init.pages.lock().unwrap().remove(&guard.slot.id); }
                }
            }
        }
    }
    async fn init_step(&self, guard: &Guard, lease: &KuaishouInitLease, deadline: Instant) -> std::result::Result<(), Code> {
        let session = guard.slot.session().ok_or(Code::ContextChanged)?;
        let owned = self.account_init.pages.lock().unwrap().get(&guard.slot.id).cloned();
        let target = match owned {
            Some(Some(target)) => target,
            Some(None) => return Err(Code::InterruptedNeedsVerification),
            None => {
                self.validate_init(guard).await?;
                self.account_init.pages.lock().unwrap().insert(guard.slot.id.clone(), None);
                let page = tokio::time::timeout(Duration::from_secs(20), session.new_bound_page(page::SUBJECT_URL)).await.map_err(|_| Code::TimedOut)?.map_err(|_| Code::PageUnsupported)?;
                let target = page.target_id().to_string();
                self.account_init.pages.lock().unwrap().insert(guard.slot.id.clone(), Some(target.clone()));
                target
            }
        };
        let mut task = session.task_page(&target, guard.slot.cancel.clone(), Duration::from_secs(2)).await.map_err(|_| Code::PageUnsupported)?;
        page::wait_identity(self, guard, &mut task).await?;
        match lease.step() {
            KuaishouInitStep::Subject => self.init_subject(guard, lease, &mut task, deadline).await,
            KuaishouInitStep::Slice => {
                let all_four_disabled = page::slice(self, guard, &mut task).await?;
                page::identity(self, guard, &mut task).await?;
                let verification = SliceVerification { platform_user_id: guard.context.platform_user_id.clone(), all_four_disabled, persisted_readback: true };
                let lease = lease.clone();
                self.init_db(Some(guard.clone()), move |pm| pm.kuaishou_init_complete_slice(&lease, &verification)).await.map_err(|_| Code::ContextChanged)
            }
        }
    }
    async fn init_subject(&self, guard: &Guard, lease: &KuaishouInitLease, task: &mut TaskPage<'_>, deadline: Instant) -> std::result::Result<(), Code> {
        // A corrected valid candidate after an earlier OCR failure can finish without recollection.
        let existing = self.kuaishou_subject_detail(guard.context.platform_user_id.clone()).await.map_err(|_| Code::ContextChanged)?;
        if let Some(detail) = &existing {
            if detail.archive.validation.can_confirm() {
                if let Ok(verified) = self.account_init.cache.verify(&detail.archive.attachments, deadline).await {
                    page::identity(self, guard, task).await?;
                    let revision = detail.archive.revision; let lease = lease.clone();
                    return self.init_db(Some(guard.clone()), move |pm| pm.kuaishou_init_complete_subject(&lease, revision, &verified)).await.map_err(|_| Code::ContextChanged);
                }
            }
        }
        page::navigate(self, guard, task, page::SUBJECT_URL).await?;
        let mut selected = None;
        for (tab, source) in [("主体信息", SubjectSource::MainTab), ("达人主体信息", SubjectSource::TalentTabPlaintext)] {
            if let Ok(subject) = page::subject(self, guard, task, tab).await {
                let evidence = SubjectVisibleEvidence { real_name: Some(subject.name.clone()), id_card: Some(subject.card.clone()) };
                let valid = validate_subject_fields(&subject.name, &subject.card, &evidence, &chrono::Utc::now().format("%Y-%m-%d").to_string()).can_confirm();
                selected = Some((subject, source, evidence, valid));
                if valid { break; }
            }
        }
        let (subject, source, evidence, plaintext) = selected.ok_or(Code::AttachmentUnavailable)?;
        let mut attachments = Vec::new();
        for encoded in subject.pictures {
            if encoded.len() > local_ocr::MAX_IMAGE_BYTES.div_ceil(3) * 4 { return Err(Code::AttachmentUnavailable); }
            let bytes = STANDARD.decode(encoded).map_err(|_| Code::AttachmentUnavailable)?;
            let a = self.account_init.cache.store(bytes, deadline).await.map_err(|_| Code::AttachmentUnavailable)?;
            if !attachments.contains(&a) { attachments.push(a); }
        }
        let mut candidate = SubjectCandidate { real_name: subject.name, id_card: subject.card, source, evidence, attachments };
        let mut ocr_failed = false;
        if !plaintext {
            match self.recognize_attachments(&candidate.attachments, deadline).await {
                Ok((name, card)) => { candidate.real_name = name; candidate.id_card = card; candidate.source = SubjectSource::Ocr; },
                Err(_) => ocr_failed = true,
            }
        }
        page::identity(self, guard, task).await?;
        let revision = existing.map(|d| d.archive.revision).unwrap_or(0);
        let saved_lease = lease.clone();
        let archive = self.init_db(Some(guard.clone()), move |pm| pm.kuaishou_subject_save_candidate(&saved_lease, revision, candidate)).await.map_err(|_| Code::ContextChanged)?;
        if ocr_failed { return Err(Code::OcrFailed); }
        if !archive.validation.can_confirm() { return Err(Code::ValidationFailed); }
        let verified = self.account_init.cache.verify(&archive.attachments, deadline).await.map_err(|_| Code::AttachmentUnavailable)?;
        page::identity(self, guard, task).await?;
        let lease = lease.clone();
        self.init_db(Some(guard.clone()), move |pm| pm.kuaishou_init_complete_subject(&lease, archive.revision, &verified)).await.map_err(|_| Code::ContextChanged)
    }
}
