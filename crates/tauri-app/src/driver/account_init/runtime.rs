use super::{attachments::AttachmentCache, error, page, Guard, TauriBrowserDriver};
use base64::{engine::general_purpose::STANDARD, Engine};
use cdp_driver::{TaskCancel, TaskPage};
use multizen_core::*;
use profile_manager::KuaishouInitLease;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use KuaishouInitErrorCode as Code;
#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

/// The database owns eligibility. Never erase its backoff to implement automatic retries.
/// A running lease (including one with a lost reply) is not safe to take over.
fn retry_delay(
    steps: &[KuaishouInitStepRecord],
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Duration> {
    if steps.iter().any(|s| {
        s.state == KuaishouInitState::Running
            || (s.state == KuaishouInitState::Failed
                && s.last_error_code == Some(Code::ContextChanged))
    }) {
        return None;
    }
    steps
        .iter()
        .filter(|s| s.state == KuaishouInitState::Failed)
        .filter_map(|s| {
            let due = chrono::DateTime::parse_from_rfc3339(s.next_retry_at.as_deref()?).ok()?;
            let delay = due.signed_duration_since(now).to_std().unwrap_or_default();
            // Runtime-produced backoff is at most 320 seconds. Unexpected persisted dates
            // need explicit recovery, not an indefinitely retained driver and profile gate.
            (delay <= Duration::from_secs(320)).then_some(delay.max(Duration::from_millis(1)))
        })
        .min()
}

fn initial_page_url(step: KuaishouInitStep) -> &'static str {
    match step {
        KuaishouInitStep::Subject => page::SUBJECT_URL,
        KuaishouInitStep::Slice => page::SLICE_URL,
    }
}
fn forget_proven_missing(
    pages: &mut HashMap<String, Option<String>>,
    session: &str,
    expected: &str,
) -> bool {
    if pages.get(session).and_then(|target| target.as_deref()) != Some(expected) {
        return false;
    }
    pages.remove(session);
    true
}

/// Bounded liveness probe for a target browser inventory says is present.
/// A crashed renderer keeps its target id but never replies; a short evaluate
/// timeout here is therefore destruction evidence, unlike a transient TaskPage
/// acquisition failure or a plain navigation timeout, which are not.
enum Liveness {
    Responsive,
    Unresponsive,
}
const LIVENESS_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
async fn probe_owned_liveness(
    session: &cdp_driver::session::BrowserSession,
    target: &str,
) -> Liveness {
    let probe = async {
        let page = session.bind_page(target).await.ok()?;
        page.evaluate("1").await.ok()
    };
    match tokio::time::timeout(LIVENESS_PROBE_TIMEOUT, probe).await {
        Ok(Some(_)) => Liveness::Responsive,
        Ok(None) | Err(_) => Liveness::Unresponsive,
    }
}

pub(in crate::driver) struct InitRuntime {
    pub(super) cache: AttachmentCache,
    pub(in crate::driver) stop: TaskCancel,
    started: AtomicBool,
    active: Mutex<HashSet<String>>,
    // None means a create request had unknown completion: never create another automatically.
    pages: Mutex<HashMap<String, Option<String>>>,
    runs: Mutex<HashMap<(String, String), u32>>,
}
impl InitRuntime {
    pub fn new(root: PathBuf) -> Self {
        Self {
            cache: AttachmentCache::new(root),
            stop: TaskCancel::new(),
            started: AtomicBool::new(false),
            active: Mutex::new(HashSet::new()),
            pages: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
        }
    }
    #[cfg(test)]
    pub(super) fn test_record_unknown_creation(&self, session: &str) {
        self.pages.lock().unwrap().insert(session.into(), None);
    }
    #[cfg(test)]
    pub(super) fn test_record_known_target(&self, session: &str, target: &str) {
        self.pages
            .lock()
            .unwrap()
            .insert(session.into(), Some(target.into()));
    }
    #[cfg(test)]
    pub(super) fn test_owned_record(&self, session: &str) -> Option<Option<String>> {
        self.pages.lock().unwrap().get(session).cloned()
    }
}
struct Active {
    runtime: Arc<InitRuntime>,
    id: String,
    _identity: Option<crate::driver::identity::InitializationReservation>,
}
impl Drop for Active {
    fn drop(&mut self) {
        self.runtime.active.lock().unwrap().remove(&self.id);
    }
}
impl TauriBrowserDriver {
    fn init_slot(&self, id: &str) -> Option<Active> {
        if self.account_init.stop.is_cancelled() {
            return None;
        }
        let mut active = self.account_init.active.lock().unwrap();
        // Pending handoffs consume the same bounded capacity as running work.
        if active.len() >= 4 || active.contains(id) {
            return None;
        }
        active.insert(id.into());
        Some(Active {
            runtime: self.account_init.clone(),
            id: id.into(),
            _identity: None,
        })
    }
    async fn init_admit(&self, mut active: Active, wait: Duration) -> Option<Active> {
        let reservation = tokio::select! {
            biased;
            _ = self.account_init.stop.cancelled() => None,
            _ = self.identity.stop.cancelled() => None,
            result = tokio::time::timeout(wait, self.identity.reserve_initialization_wait(&active.id)) => result.ok().flatten(),
        };
        active._identity = Some(reservation?);
        Some(active)
    }
    async fn init_enter_wait(&self, id: &str) -> Option<Active> {
        let active = self.init_slot(id)?;
        self.init_admit(active, Duration::from_secs(20)).await
    }
    async fn init_context(&self, profile_id: &str) -> Result<Guard> {
        let slot = self
            .registry
            .slot(profile_id)
            .await
            .ok_or_else(|| error("环境未运行或会话未就绪"))?;
        let id = profile_id.to_string();
        let session_id = slot.id.clone();
        let context = self
            .init_db(None, move |pm| {
                let o = pm
                    .kuaishou_identity_observation(&id)?
                    .ok_or_else(|| error("当前身份尚未可信检测"))?;
                let fresh = o
                    .snapshot
                    .checked_at
                    .as_deref()
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .is_some_and(|t| {
                        let age = chrono::Utc::now().signed_duration_since(t);
                        age.num_seconds() >= 0 && age.num_seconds() <= 30
                    });
                if !fresh
                    || o.session_id.as_deref() != Some(session_id.as_str())
                    || o.snapshot.status != KuaishouIdentityStatus::Detected
                {
                    return Err(error("当前身份观察不新鲜，请先重新检测"));
                }
                let context = KuaishouInitContext {
                    platform_user_id: o
                        .snapshot
                        .platform_user_id
                        .ok_or_else(|| error("当前账号不可信"))?,
                    profile_id: id.clone(),
                    session_id,
                    expected_business: pm.business_accounts_profile_state(&id)?,
                };
                super::validate(pm, &context)?;
                Ok(context)
            })
            .await?;
        let guard = Guard { context, slot };
        self.validate_init(&guard)
            .await
            .map_err(|_| error("当前环境已变化"))?;
        // Read all existing shop targets afresh. Conflicting pages must never select an account.
        let session = guard.slot.session().ok_or_else(|| error("会话已关闭"))?;
        let observed = tokio::time::timeout(
            Duration::from_secs(8),
            self.current_kuaishou_account(&session, guard.slot.cancel.clone()),
        )
        .await
        .map_err(|_| error("当前账号重验超时"))??;
        if observed != guard.context.platform_user_id {
            return Err(error("当前页面身份冲突或已换号"));
        }
        Ok(guard)
    }
    pub(super) async fn validate_init(&self, guard: &Guard) -> std::result::Result<(), Code> {
        if self.account_init.stop.is_cancelled() || guard.slot.cancel.is_cancelled() {
            return Err(Code::ContextChanged);
        }
        self.init_db(Some(guard.clone()), |_| Ok(()))
            .await
            .map_err(|_| Code::ContextChanged)
    }
    pub async fn kuaishou_init_retry(self: &Arc<Self>, profile_id: String) -> Result<()> {
        // Explicit write-capable entry; the preexisting detect/list remain read-only.
        let active = self
            .init_enter_wait(&profile_id)
            .await
            .ok_or_else(|| error("身份检测或初始化正在进行，请稍后重试"))?;
        let guard = self.init_context(&profile_id).await?;
        let id = guard.context.platform_user_id.clone();
        self.init_db(Some(guard.clone()), move |pm| {
            pm.kuaishou_init_retry_failed(&id)
        })
        .await?;
        self.account_init.runs.lock().unwrap().remove(&(
            guard.slot.id.clone(),
            guard.context.platform_user_id.clone(),
        ));
        let driver = self.clone();
        tauri::async_runtime::spawn(async move {
            // Keep the per-profile gate for the whole bounded campaign, including backoff.
            // This does not start the automatic monitor or reset a completed step.
            let _active = active;
            for attempt in 0..3 {
                driver.init_run(&guard).await;
                if attempt == 2 || driver.validate_init(&guard).await.is_err() {
                    break;
                }
                let Ok(steps) = driver
                    .kuaishou_init_steps(guard.context.platform_user_id.clone())
                    .await
                else {
                    break;
                };
                let Some(delay) = retry_delay(&steps, chrono::Utc::now()) else {
                    break;
                };
                tokio::select! {
                    biased;
                    _ = driver.account_init.stop.cancelled() => break,
                    _ = guard.slot.cancel.cancelled() => break,
                    _ = tokio::time::sleep(delay) => {},
                }
            }
        });
        Ok(())
    }
    /// App startup entry point. Both monitors keep their existing once guards,
    /// weak idle ownership, cancellation and independent eligibility checks.
    pub fn start_kuaishou_monitors(self: &Arc<Self>) {
        self.start_kuaishou_identity_monitor();
        self.start_kuaishou_init_monitor();
    }
    pub fn start_kuaishou_init_monitor(self: &Arc<Self>) {
        if self.account_init.started.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak = Arc::downgrade(self);
        let stop = self.account_init.stop.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { biased; _ = stop.cancelled() => break, _ = interval.tick() => {} }
                let Some(driver) = weak.upgrade() else {
                    break;
                };
                for id in driver.registry.ids().await {
                    let Some(active) = driver.init_slot(&id) else {
                        continue;
                    };
                    let driver = driver.clone();
                    tauri::async_runtime::spawn(async move {
                        // Register one bounded waiter, not another same-phase skip.
                        // Profile release hands its fair permit to this initializer.
                        let Some(_active) =
                            driver.init_admit(active, Duration::from_secs(20)).await
                        else {
                            return;
                        };
                        if let Ok(guard) = driver.init_context(&id).await {
                            driver.init_run(&guard).await;
                        }
                    });
                }
            }
        });
    }
    pub fn stop_kuaishou_init_monitor(&self) {
        self.account_init.stop.cancel();
    }
    async fn init_run(&self, guard: &Guard) {
        let budget_key = (
            guard.slot.id.clone(),
            guard.context.platform_user_id.clone(),
        );
        let exhausted = *self
            .account_init
            .runs
            .lock()
            .unwrap()
            .get(&budget_key)
            .unwrap_or(&0)
            >= 3;
        if exhausted {
            self.init_close_completed(guard).await;
            return;
        }
        let mut attempted = false;
        for step in [KuaishouInitStep::Subject, KuaishouInitStep::Slice] {
            if self.validate_init(guard).await.is_err() {
                break;
            }
            let ctx = guard.context.clone();
            let launcher = self.launcher_tx.clone();
            let mut lease = match self
                .init_db(Some(guard.clone()), move |pm| {
                    Ok(pm
                        .kuaishou_init_claim(&ctx, step)?
                        .map(|lease| super::lease::RunningLease::new(lease, launcher)))
                })
                .await
            {
                Ok(Some(lease)) => lease,
                _ => continue,
            };
            attempted = true;
            let deadline = Instant::now() + Duration::from_secs(100);
            let mut probe = super::diagnostics::Probe::new(step, "step");
            let result = tokio::select! {
                biased;
                _ = self.account_init.stop.cancelled() => {
                    probe.finish("application-stopping"); Err(Code::InterruptedNeedsVerification)
                },
                _ = guard.slot.cancel.cancelled() => {
                    probe.finish("session-cancelled"); Err(Code::InterruptedNeedsVerification)
                },
                result = tokio::time::timeout_at(deadline.into(), self.init_step(guard, &lease, deadline)) => result.unwrap_or(Err(Code::TimedOut)),
            };
            probe.finish(
                result
                    .as_ref()
                    .err()
                    .copied()
                    .map(super::diagnostics::step_error)
                    .unwrap_or("none"),
            );
            if let Err(code) = result {
                let seconds = 20u32.saturating_mul(1u32 << lease.attempts().min(4));
                // Releasing only the original token is allowed after session invalidation.
                // The operation future and TaskPage are dropped before releasing its DB lease.
                lease.fail(code, seconds).await;
                if matches!(
                    code,
                    Code::InterruptedNeedsVerification | Code::TimedOut | Code::ContextChanged
                ) {
                    break;
                }
            }
        }
        if attempted {
            *self
                .account_init
                .runs
                .lock()
                .unwrap()
                .entry(budget_key)
                .or_default() += 1;
        }
        self.init_close_completed(guard).await;
    }
    async fn init_close_completed(&self, guard: &Guard) {
        // Retry cleanup even if no step was claimed (e.g. a successful commit/close reply
        // was lost). Persistent done states, not the last IPC result, are authoritative.
        if self.validate_init(guard).await.is_ok() {
            let id = guard.context.platform_user_id.clone();
            let done = self
                .init_db(Some(guard.clone()), move |pm| {
                    Ok(pm
                        .kuaishou_init_steps(&id)?
                        .iter()
                        .filter(|s| s.state == KuaishouInitState::Done)
                        .count()
                        == 2)
                })
                .await
                .unwrap_or(false);
            if done {
                let target = self
                    .account_init
                    .pages
                    .lock()
                    .unwrap()
                    .get(&guard.slot.id)
                    .cloned()
                    .flatten();
                if let (Some(target), Some(session)) = (target, guard.slot.session()) {
                    // Only this runtime's self-created target can be closed. Never user pages.
                    if tokio::time::timeout(Duration::from_secs(5), session.close_page(&target))
                        .await
                        .is_ok_and(|r| r.is_ok())
                    {
                        self.account_init
                            .pages
                            .lock()
                            .unwrap()
                            .remove(&guard.slot.id);
                    }
                }
            }
        }
    }
    pub(super) async fn init_owned_target(
        &self,
        guard: &Guard,
        step: KuaishouInitStep,
        session: &cdp_driver::session::BrowserSession,
    ) -> std::result::Result<String, Code> {
        self.validate_init(guard).await?;
        let current = guard.slot.session().ok_or(Code::ContextChanged)?;
        if !std::ptr::eq(current.as_ref(), session) {
            return Err(Code::ContextChanged);
        }
        let owned = self
            .account_init
            .pages
            .lock()
            .unwrap()
            .get(&guard.slot.id)
            .cloned();
        // A proven-dead renderer is discarded and rebuilt in this same call; the
        // flag only changes the terminal error code if that rebuild cannot happen.
        let mut crashed = false;
        let owned = match owned {
            Some(Some(target)) => {
                let mut probe = super::diagnostics::Probe::new(step, "owned-inventory");
                // Browser-level inventory is authoritative here; an attached-page cache,
                // failed TaskPage acquisition or timeout is not proof of destruction.
                let inventory = tokio::time::timeout(
                    Duration::from_secs(2),
                    session.browser.execute(
                        chromiumoxide::cdp::browser_protocol::target::GetTargetsParams::default(),
                    ),
                )
                .await
                .map_err(|_| {
                    probe.finish("inventory-timed-out");
                    Code::TimedOut
                })?
                .map_err(|_| {
                    probe.finish("inventory-driver-error");
                    Code::PageUnsupported
                })?;
                self.validate_init(guard).await.inspect_err(|_code| {
                    probe.finish("context-rejected");
                })?;
                if inventory
                    .result
                    .target_infos
                    .iter()
                    .any(|info| info.target_id.as_ref() == target.as_str())
                {
                    // Inventory presence is not liveness: a crashed renderer keeps its
                    // target id forever. Prove the page actually answers before reuse.
                    match probe_owned_liveness(session, &target).await {
                        Liveness::Responsive => {
                            probe.finish("target-present");
                            Some(Some(target))
                        }
                        Liveness::Unresponsive => {
                            if !forget_proven_missing(
                                &mut self.account_init.pages.lock().unwrap(),
                                &guard.slot.id,
                                &target,
                            ) {
                                probe.finish("ownership-changed");
                                return Err(Code::ContextChanged);
                            }
                            probe.finish("target-crashed-rebuilt");
                            crashed = true;
                            None
                        }
                    }
                } else {
                    if !forget_proven_missing(
                        &mut self.account_init.pages.lock().unwrap(),
                        &guard.slot.id,
                        &target,
                    ) {
                        probe.finish("ownership-changed");
                        return Err(Code::ContextChanged);
                    }
                    probe.finish("target-proven-missing");
                    None
                }
            }
            other => other,
        };
        let target = match owned {
            Some(Some(target)) => target,
            Some(None) => {
                let mut probe = super::diagnostics::Probe::new(step, "owned-acquire");
                probe.finish("ownership-unknown");
                return Err(Code::InterruptedNeedsVerification);
            }
            None => {
                self.validate_init(guard).await?;
                self.account_init
                    .pages
                    .lock()
                    .unwrap()
                    .insert(guard.slot.id.clone(), None);
                let mut probe = super::diagnostics::Probe::new(step, "owned-create");
                let page = tokio::time::timeout(
                    Duration::from_secs(20),
                    session.new_bound_page(initial_page_url(step)),
                )
                .await;
                let page = match page {
                    Ok(Ok(page)) => page,
                    // When a proven crash could not be healed, report the crash rather
                    // than a generic structure/timeout code; the next attempt rebuilds.
                    Ok(Err(_)) => {
                        probe.finish("create-driver-error");
                        return Err(if crashed {
                            Code::PageCrashed
                        } else {
                            Code::PageUnsupported
                        });
                    }
                    Err(_) => {
                        probe.finish("create-timed-out");
                        return Err(if crashed {
                            Code::PageCrashed
                        } else {
                            Code::TimedOut
                        });
                    }
                };
                probe.finish("none");
                let target = page.target_id().to_string();
                self.account_init
                    .pages
                    .lock()
                    .unwrap()
                    .insert(guard.slot.id.clone(), Some(target.clone()));
                target
            }
        };
        Ok(target)
    }
    async fn init_step(
        &self,
        guard: &Guard,
        lease: &KuaishouInitLease,
        deadline: Instant,
    ) -> std::result::Result<(), Code> {
        let session = guard.slot.session().ok_or(Code::ContextChanged)?;
        let target = self
            .init_owned_target(guard, lease.step(), &session)
            .await?;
        let mut probe = super::diagnostics::Probe::new(lease.step(), "owned-acquire");
        let mut task = session
            .task_page(&target, guard.slot.cancel.clone(), Duration::from_secs(2))
            .await
            .map_err(|error| {
                probe.finish(super::diagnostics::task_error(&error));
                Code::PageUnsupported
            })?;
        probe.finish("none");
        page::wait_identity(self, guard, &mut task).await?;
        match lease.step() {
            KuaishouInitStep::Subject => self.init_subject(guard, lease, &mut task, deadline).await,
            KuaishouInitStep::Slice => {
                let all_four_disabled = page::slice(self, guard, &mut task).await?;
                page::identity(self, guard, &mut task).await?;
                let verification = SliceVerification {
                    platform_user_id: guard.context.platform_user_id.clone(),
                    all_four_disabled,
                    persisted_readback: true,
                };
                let lease = lease.clone();
                self.init_db(Some(guard.clone()), move |pm| {
                    pm.kuaishou_init_complete_slice(&lease, &verification)
                })
                .await
                .map_err(|_| Code::ContextChanged)
            }
        }
    }
    async fn init_subject(
        &self,
        guard: &Guard,
        lease: &KuaishouInitLease,
        task: &mut TaskPage<'_>,
        deadline: Instant,
    ) -> std::result::Result<(), Code> {
        // A corrected valid candidate after an earlier OCR failure can finish without recollection.
        let existing = self
            .kuaishou_subject_detail(guard.context.platform_user_id.clone())
            .await
            .map_err(|_| Code::ContextChanged)?;
        if let Some(detail) = &existing {
            if detail.archive.validation.can_confirm() {
                if let Ok(verified) = self
                    .account_init
                    .cache
                    .verify(&detail.archive.attachments, deadline)
                    .await
                {
                    page::identity(self, guard, task).await?;
                    let revision = detail.archive.revision;
                    let lease = lease.clone();
                    return self
                        .init_db(Some(guard.clone()), move |pm| {
                            pm.kuaishou_init_complete_subject(&lease, revision, &verified)
                        })
                        .await
                        .map_err(|_| Code::ContextChanged);
                }
            }
        }
        page::navigate(self, guard, task, page::SUBJECT_URL).await?;
        // Read the talent subject tab for certificate photos only — plaintext
        // on either tab is unreliable (operator tab often empty, talent tab
        // masked). OCR of the certificate photo is the sole identity source.
        let subject = page::subject(self, guard, task, "达人主体信息").await?;
        let evidence = SubjectVisibleEvidence {
            real_name: Some(subject.name.clone()),
            id_card: Some(subject.card.clone()),
        };
        let batch = super::staging::BatchState::begin(
            &self.account_init.cache,
            self.launcher_tx.clone(),
            deadline,
        )
        .await
        .map_err(|_| Code::AttachmentUnavailable)?;
        let mut attachments = Vec::new();
        for encoded in subject.pictures {
            if encoded.len() > local_ocr::MAX_IMAGE_BYTES.div_ceil(3) * 4 {
                return Err(Code::AttachmentUnavailable);
            }
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| Code::AttachmentUnavailable)?;
            let a = batch
                .store(bytes, deadline)
                .await
                .map_err(|_| Code::AttachmentUnavailable)?;
            if !attachments.contains(&a) {
                attachments.push(a);
            }
        }
        let mut candidate = SubjectCandidate {
            real_name: subject.name,
            id_card: subject.card,
            source: SubjectSource::TalentTabPlaintext,
            evidence,
            attachments,
        };
        // OCR is the sole identity source; plaintext is never trusted.
        match self
            .recognize_attachments(&candidate.attachments, deadline)
            .await
        {
            Ok((name, card)) => {
                candidate.real_name = name;
                candidate.id_card = card;
                candidate.source = SubjectSource::Ocr;
            }
            Err(_) => return Err(Code::OcrFailed),
        }
        page::identity(self, guard, task).await?;
        let revision = existing.map(|d| d.archive.revision).unwrap_or(0);
        let saved_lease = lease.clone();
        let archive = self
            .init_db(Some(guard.clone()), move |pm| {
                pm.kuaishou_subject_save_candidate(&saved_lease, revision, candidate)
            })
            .await
            .map_err(|_| Code::ContextChanged)?;
        if !archive.validation.can_confirm() {
            return Err(Code::ValidationFailed);
        }
        let verified = self
            .account_init
            .cache
            .verify(&archive.attachments, deadline)
            .await
            .map_err(|_| Code::AttachmentUnavailable)?;
        page::identity(self, guard, task).await?;
        let lease = lease.clone();
        self.init_db(Some(guard.clone()), move |pm| {
            pm.kuaishou_init_complete_subject(&lease, archive.revision, &verified)
        })
        .await
        .map_err(|_| Code::ContextChanged)
    }
}
