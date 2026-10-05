//! Account-level exclusive CAS lease. No time-based takeover: an expired/cancelled
//! browser action cannot be assumed stopped. Runtime releases failures after cleanup;
//! startup recovery is explicit and must run once before any workers are scheduled.
use crate::{
    kuaishou_account::{decode, invalid, now, read_archive, validate_archive},
    ProfileManager,
};
use multizen_core::*;
use rusqlite::params;
use uuid::Uuid;

#[derive(Clone)]
pub struct KuaishouInitLease {
    context: KuaishouInitContext,
    step: KuaishouInitStep,
    token: String,
    attempts: u32,
}

impl KuaishouInitLease {
    pub fn context(&self) -> &KuaishouInitContext {
        &self.context
    }
    pub fn step(&self) -> KuaishouInitStep {
        self.step
    }
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

impl std::fmt::Debug for KuaishouInitLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KuaishouInitLease")
            .field("step", &self.step)
            .field("attempts", &self.attempts)
            .finish_non_exhaustive()
    }
}

fn step_name(step: KuaishouInitStep) -> &'static str {
    match step {
        KuaishouInitStep::Subject => "subject",
        KuaishouInitStep::Slice => "slice",
    }
}

impl ProfileManager {
    fn require_init_context(&self, context: &KuaishouInitContext) -> Result<()> {
        if !valid_kuaishou_user_id(&context.platform_user_id) || context.session_id.is_empty() {
            return Err(invalid("初始化账号上下文无效"));
        }
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles WHERE id=?)",
            [&context.profile_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(invalid("环境已删除，初始化结果已丢弃"));
        }
        let current = self.business_accounts_profile_state(&context.profile_id)?;
        if current != context.expected_business
            || !kuaishou_identity_allowed(&current)
            || current
                .account
                .as_ref()
                .and_then(|a| a.platform_user_id.as_deref())
                .is_some_and(|id| id != context.platform_user_id)
        {
            return Err(invalid("业务绑定已变化或不适用，初始化结果已丢弃"));
        }
        let observation = self
            .kuaishou_identity_observation(&context.profile_id)?
            .ok_or_else(|| invalid("缺少有效账号身份观察"))?;
        if observation.snapshot.status != KuaishouIdentityStatus::Detected
            || observation.snapshot.platform_user_id.as_deref() != Some(&context.platform_user_id)
            || observation.session_id.as_deref() != Some(&context.session_id)
        {
            return Err(invalid("账号或会话已变化，初始化结果已丢弃"));
        }
        Ok(())
    }

    pub(crate) fn require_init_lease(
        &self,
        lease: &KuaishouInitLease,
        step: KuaishouInitStep,
    ) -> Result<()> {
        if lease.step != step {
            return Err(invalid("初始化步骤不匹配"));
        }
        let active: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM kuaishou_init_steps
                WHERE platform_user_id=? AND step=? AND state='running' AND lease_token=?)",
            params![lease.context.platform_user_id, step_name(step), lease.token],
            |r| r.get(0),
        )?;
        if !active {
            return Err(invalid("初始化执行凭据已失效，请重新检测"));
        }
        self.require_init_context(&lease.context)
    }

    /// Returns None for account busy, step done, or — for monitor scans — a failed
    /// step: the monitor claims only pending steps, while the manual retry path may
    /// re-claim failed ones after clearing persisted backoff. Same-account subject
    /// and slice cannot run concurrently, even through different Profiles/connections.
    pub fn kuaishou_init_claim(
        &self,
        context: &KuaishouInitContext,
        step: KuaishouInitStep,
        allow_failed: bool,
    ) -> Result<Option<KuaishouInitLease>> {
        let tx = self.conn.unchecked_transaction()?;
        self.require_init_context(context)?;
        let timestamp = now();
        for name in ["subject", "slice"] {
            tx.execute(
                "INSERT INTO kuaishou_init_steps(platform_user_id,step,state,updated_at)
                VALUES (?,?,'pending',?) ON CONFLICT(platform_user_id,step) DO NOTHING",
                params![context.platform_user_id, name, timestamp],
            )?;
        }
        let token = Uuid::new_v4().to_string();
        let state_filter =
            if allow_failed { "state IN ('pending','failed')" } else { "state='pending'" };
        let changed = tx.execute(
            &format!(
                "UPDATE kuaishou_init_steps SET state='running',attempts=attempts+1,next_retry_at=NULL,
                    completed_at=NULL,lease_token=?3,updated_at=?4
                 WHERE platform_user_id=?1 AND step=?2 AND {state_filter}
                    AND attempts<4294967295 AND (next_retry_at IS NULL OR next_retry_at<=?4)
                    AND NOT EXISTS(SELECT 1 FROM kuaishou_init_steps WHERE platform_user_id=?1 AND state='running')"
            ),
            params![context.platform_user_id, step_name(step), token, timestamp],
        )?;
        if changed == 0 {
            tx.commit()?;
            return Ok(None);
        }
        let attempts = tx.query_row(
            "SELECT attempts FROM kuaishou_init_steps WHERE platform_user_id=? AND step=?",
            params![context.platform_user_id, step_name(step)],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(Some(KuaishouInitLease {
            context: context.clone(),
            step,
            token,
            attempts,
        }))
    }

    pub fn kuaishou_init_steps(
        &self,
        platform_user_id: &str,
    ) -> Result<Vec<KuaishouInitStepRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT json_object('platformUserId',platform_user_id,'step',step,'state',state,
                'attempts',attempts,'nextRetryAt',next_retry_at,
                'lastErrorCode',CASE WHEN last_error_code IS NULL THEN NULL ELSE json_extract(last_error_code,'$') END,
                'completedAt',completed_at,'updatedAt',updated_at)
             FROM kuaishou_init_steps WHERE platform_user_id=? ORDER BY step DESC",
        )?;
        let rows = stmt
            .query_map([platform_user_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter().map(|s| decode(&s)).collect()
    }

    fn complete_init_step(&self, lease: &KuaishouInitLease) -> Result<()> {
        let timestamp = now();
        let changed = self.conn.execute(
            "UPDATE kuaishou_init_steps SET state='done',lease_token=NULL,next_retry_at=NULL,
                last_error_code=NULL,completed_at=?4,updated_at=?4
             WHERE platform_user_id=?1 AND step=?2 AND state='running' AND lease_token=?3",
            params![
                lease.context.platform_user_id,
                step_name(lease.step),
                lease.token,
                timestamp
            ],
        )?;
        if changed != 1 {
            return Err(invalid("初始化执行凭据已失效，请重新检测"));
        }
        Ok(())
    }

    /// Collection completion is separate from human review; caller rereads/verifies
    /// images outside this transaction. Invalid fields or stale photos cannot set done.
    pub fn kuaishou_init_complete_subject(
        &self,
        lease: &KuaishouInitLease,
        expected_revision: u64,
        verified_attachments: &[SubjectAttachment],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        self.require_init_lease(lease, KuaishouInitStep::Subject)?;
        let archive = read_archive(&tx, &lease.context.platform_user_id)?
            .ok_or_else(|| invalid("主体资料尚未保存"))?;
        if archive.revision != expected_revision {
            return Err(invalid("资料版本已变更，请刷新后重试"));
        }
        if !validate_archive(&archive).can_confirm() {
            return Err(invalid("资料自动校验未通过，采集尚未完成"));
        }
        if !valid_subject_attachments(verified_attachments)
            || archive.attachments != verified_attachments
        {
            return Err(invalid("证件照片未完成本地读取验证"));
        }
        self.complete_init_step(lease)?;
        tx.commit()?;
        Ok(())
    }

    /// Not exposed as a generic status setter or IPC. Runtime owns durable-readback proof.
    pub fn kuaishou_init_complete_slice(
        &self,
        lease: &KuaishouInitLease,
        verification: &SliceVerification,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        self.require_init_lease(lease, KuaishouInitStep::Slice)?;
        if verification.platform_user_id != lease.context.platform_user_id
            || !verification
                .all_four_disabled
                .iter()
                .all(|disabled| *disabled)
            || !verification.persisted_readback
        {
            return Err(invalid("切片权限尚未通过持久状态回读验证"));
        }
        self.complete_init_step(lease)?;
        tx.commit()?;
        Ok(())
    }

    /// Failure release checks only token ownership: must work even after session/profile
    /// deletion. Runtime must first cancel/finish its in-flight task before releasing.
    /// A failure is terminal for the automatic monitor; only the explicit manual retry
    /// path re-claims the step, so no retry time is scheduled here.
    pub fn kuaishou_init_fail(
        &self,
        lease: &KuaishouInitLease,
        code: KuaishouInitErrorCode,
    ) -> Result<bool> {
        let timestamp = chrono::Utc::now();
        Ok(self.conn.execute(
            "UPDATE kuaishou_init_steps SET state='failed',lease_token=NULL,next_retry_at=NULL,
                last_error_code=?4,completed_at=NULL,updated_at=?5
             WHERE platform_user_id=?1 AND step=?2 AND state='running' AND lease_token=?3",
            params![
                lease.context.platform_user_id,
                step_name(lease.step),
                lease.token,
                serde_json::to_string(&code)?,
                timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            ],
        )? == 1)
    }

    /// Startup-only recovery, before ANY workers start. Does not run during migrations,
    /// reads or manager construction (another DB connection must not revoke a live lease).
    /// Old tokens become invalid; done steps and attempt counts are preserved.
    pub fn kuaishou_init_recover_interrupted(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE kuaishou_init_steps SET state='failed',lease_token=NULL,next_retry_at=NULL,
                last_error_code=?1,completed_at=NULL,updated_at=?2 WHERE state='running'",
            params![
                serde_json::to_string(&KuaishouInitErrorCode::InterruptedNeedsVerification)?,
                now()
            ],
        )?)
    }

    /// Manual re-arm: clears any persisted backoff so the manual claim can take the
    /// failed step; never resets done/running or schedules automatic retries.
    pub fn kuaishou_init_retry_failed(&self, platform_user_id: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE kuaishou_init_steps SET next_retry_at=NULL,updated_at=?2
             WHERE platform_user_id=?1 AND state='failed'",
            params![platform_user_id, now()],
        )?)
    }
}
