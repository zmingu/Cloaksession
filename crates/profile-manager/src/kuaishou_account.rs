//! Independent subject documents; no profile FK and no manual business-account writes.
use crate::ProfileManager;
use multizen_core::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS kuaishou_subject_archives (
            platform_user_id TEXT PRIMARY KEY NOT NULL REFERENCES kuaishou_identities(platform_user_id),
            document TEXT NOT NULL CHECK(json_valid(document)
                AND json_extract(document,'$.platformUserId')=platform_user_id
                AND json_type(document,'$.revision')='integer'
                AND json_extract(document,'$.revision')>0),
            real_name TEXT GENERATED ALWAYS AS (json_extract(document,'$.realName')) VIRTUAL,
            id_card TEXT GENERATED ALWAYS AS (json_extract(document,'$.idCard')) VIRTUAL,
            revision INTEGER GENERATED ALWAYS AS (json_extract(document,'$.revision')) VIRTUAL
        );
        CREATE INDEX IF NOT EXISTS idx_kuaishou_subject_name ON kuaishou_subject_archives(real_name);
        CREATE TABLE IF NOT EXISTS kuaishou_init_steps (
            platform_user_id TEXT NOT NULL REFERENCES kuaishou_identities(platform_user_id),
            step TEXT NOT NULL CHECK(step IN ('subject','slice')),
            state TEXT NOT NULL CHECK(state IN ('pending','running','done','failed')),
            attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0),
            next_retry_at TEXT,
            last_error_code TEXT CHECK(last_error_code IS NULL OR json_valid(last_error_code)),
            completed_at TEXT,
            updated_at TEXT NOT NULL,
            lease_token TEXT,
            PRIMARY KEY(platform_user_id,step),
            CHECK((state='running')=(lease_token IS NOT NULL)),
            CHECK((state='done')=(completed_at IS NOT NULL))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_kuaishou_init_account_running
            ON kuaishou_init_steps(platform_user_id) WHERE state='running';",
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn invalid(message: &str) -> MultizenError {
    MultizenError::Config(message.into())
}

pub(crate) fn decode<T: DeserializeOwned>(raw: &str) -> Result<T> {
    // Serde errors may contain an unexpected stored string: never echo the document.
    serde_json::from_str(raw).map_err(|_| invalid("账号资料格式异常，请重新检查"))
}

fn enum_value<T: DeserializeOwned>(raw: String) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(raw))
        .map_err(|_| invalid("账号资料状态异常，请重新检查"))
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn validate_archive(archive: &KuaishouSubjectArchive) -> SubjectValidation {
    validate_subject_fields(
        &archive.real_name,
        &archive.id_card,
        &archive.evidence,
        &chrono::Utc::now().format("%Y-%m-%d").to_string(),
    )
}

pub(crate) fn read_archive(conn: &Connection, id: &str) -> Result<Option<KuaishouSubjectArchive>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT document FROM kuaishou_subject_archives WHERE platform_user_id=?",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| {
        let mut archive: KuaishouSubjectArchive = decode(&s)?;
        archive.validation = validate_archive(&archive);
        Ok(archive)
    })
    .transpose()
}

fn require_archive(conn: &Connection, id: &str, revision: u64) -> Result<KuaishouSubjectArchive> {
    let archive = read_archive(conn, id)?.ok_or_else(|| invalid("账号主体档案不存在"))?;
    if revision == 0 || archive.revision != revision {
        return Err(invalid("资料版本已变更，请刷新后重试"));
    }
    Ok(archive)
}

fn bounded_field(value: &str, max: usize) -> bool {
    value.chars().count() <= max && !value.chars().any(char::is_control)
}

fn validate_candidate(candidate: &SubjectCandidate) -> Result<()> {
    if !bounded_field(&candidate.real_name, 100)
        || !bounded_field(&candidate.id_card, 128)
        || [&candidate.evidence.real_name, &candidate.evidence.id_card]
            .into_iter()
            .flatten()
            .any(|s| !bounded_field(s, 100))
    {
        return Err(invalid("资料字段长度或格式不符合要求"));
    }
    if !candidate.attachments.is_empty() && !valid_subject_attachments(&candidate.attachments) {
        return Err(invalid("证件附件元信息无效，请重新采集"));
    }
    Ok(())
}

fn update_archive(
    conn: &Connection,
    archive: &mut KuaishouSubjectArchive,
    expected: u64,
) -> Result<()> {
    archive.revision = expected
        .checked_add(1)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| invalid("资料版本超出范围"))?;
    archive.updated_at = now();
    archive.validation = validate_archive(archive);
    let changed = conn.execute(
        "UPDATE kuaishou_subject_archives SET document=? WHERE platform_user_id=? AND revision=?",
        params![
            serde_json::to_string(archive)?,
            archive.platform_user_id,
            expected
        ],
    )?;
    if changed != 1 {
        return Err(invalid("资料版本已变更，请刷新后重试"));
    }
    Ok(())
}

fn profiles(conn: &Connection, id: &str) -> Result<Vec<KuaishouArchiveProfile>> {
    let mut stmt = conn.prepare(
        "SELECT p.id,p.name FROM profiles p JOIN kuaishou_identity_observations o ON o.profile_id=p.id
         WHERE json_extract(o.observation,'$.snapshot.platformUserId')=? ORDER BY p.id",
    )?;
    let rows = stmt
        .query_map([id], |r| {
            Ok(KuaishouArchiveProfile {
                profile_id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn identity(conn: &Connection, id: &str) -> Result<(Option<String>, Option<String>)> {
    Ok(conn
        .query_row(
            "SELECT nickname,avatar_key FROM kuaishou_identities WHERE platform_user_id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or_default())
}

impl ProfileManager {
    /// Internal collection write. Lease guards both account and runtime context. Revision 0 creates.
    /// A stored candidate is never automatically confirmed or used to set a step to done.
    pub fn kuaishou_subject_save_candidate(
        &self,
        lease: &crate::KuaishouInitLease,
        expected_revision: u64,
        candidate: SubjectCandidate,
    ) -> Result<KuaishouSubjectArchive> {
        validate_candidate(&candidate)?;
        let tx = self.conn.unchecked_transaction()?;
        self.require_init_lease(lease, KuaishouInitStep::Subject)?;
        let old = read_archive(&tx, &lease.context().platform_user_id)?;
        if old.as_ref().map_or(0, |a| a.revision) != expected_revision {
            return Err(invalid("资料版本已变更，请刷新后重试"));
        }
        let timestamp = now();
        let validation = validate_subject_fields(
            &candidate.real_name,
            &candidate.id_card,
            &candidate.evidence,
            &chrono::Utc::now().format("%Y-%m-%d").to_string(),
        );
        let mut archive = KuaishouSubjectArchive {
            platform_user_id: lease.context().platform_user_id.clone(),
            real_name: candidate.real_name.trim().into(),
            id_card: normalize_subject_id_card(&candidate.id_card),
            source: candidate.source,
            evidence: candidate.evidence,
            attachments: candidate.attachments,
            validation,
            review_status: SubjectReviewStatus::PendingReview,
            revision: 1,
            source_profile_id: Some(lease.context().profile_id.clone()),
            created_at: old
                .as_ref()
                .map_or(timestamp.clone(), |a| a.created_at.clone()),
            updated_at: timestamp,
            confirmed_at: None,
        };
        if old.is_some() {
            update_archive(&tx, &mut archive, expected_revision)?;
        } else {
            tx.execute(
                "INSERT INTO kuaishou_subject_archives(platform_user_id,document) VALUES (?,?)",
                params![archive.platform_user_id, serde_json::to_string(&archive)?],
            )?;
        }
        tx.commit()?;
        Ok(archive)
    }

    pub fn kuaishou_subject_detail(
        &self,
        platform_user_id: &str,
    ) -> Result<Option<KuaishouSubjectDetail>> {
        let tx = self.conn.unchecked_transaction()?;
        let Some(archive) = read_archive(&tx, platform_user_id)? else {
            return Ok(None);
        };
        let (nickname, avatar_key) = identity(&tx, platform_user_id)?;
        let detail = KuaishouSubjectDetail {
            archive,
            nickname,
            avatar_key,
            profiles: profiles(&tx, platform_user_id)?,
            steps: self.kuaishou_init_steps(platform_user_id)?,
        };
        tx.commit()?;
        Ok(Some(detail))
    }

    /// Literal substring search: %, _ and backslash are not SQL wildcard operators.
    /// No full document, full ID card or photo metadata is selected for list items.
    pub fn kuaishou_subject_list(
        &self,
        query: &KuaishouSubjectQuery,
    ) -> Result<KuaishouSubjectPage> {
        if !(1..=100).contains(&query.limit) || !bounded_field(&query.search, 128) {
            return Err(invalid("查询条件或分页范围无效"));
        }
        let search = query.search.trim();
        let tx = self.conn.unchecked_transaction()?;
        let total: u64 = tx.query_row(
            "SELECT count(*) FROM kuaishou_subject_archives WHERE instr(real_name,?1)>0
             OR instr(platform_user_id,?1)>0 OR instr(id_card,?1)>0",
            [search],
            |r| r.get(0),
        )?;
        let raw = {
            let mut stmt = tx.prepare(
                "SELECT platform_user_id,real_name,
                    CASE WHEN length(id_card)=18 THEN substr(id_card,1,3)||'************'||substr(id_card,16,3)
                         ELSE '未完整识别' END,
                    json_extract(document,'$.source'),json_extract(document,'$.reviewStatus'),revision,
                    json_extract(document,'$.updatedAt')
                 FROM kuaishou_subject_archives WHERE instr(real_name,?1)>0 OR instr(platform_user_id,?1)>0
                    OR instr(id_card,?1)>0
                 ORDER BY json_extract(document,'$.updatedAt') DESC,platform_user_id ASC LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt
                .query_map(params![search, query.limit, query.offset], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, u64>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        let mut items = Vec::with_capacity(raw.len());
        for (id, real_name, masked_id_card, source, review_status, revision, updated_at) in raw {
            let (nickname, avatar_key) = identity(&tx, &id)?;
            items.push(KuaishouSubjectSummary {
                profiles: profiles(&tx, &id)?,
                platform_user_id: id,
                real_name,
                masked_id_card,
                nickname,
                avatar_key,
                source: enum_value(source)?,
                review_status: enum_value(review_status)?,
                revision,
                updated_at,
            });
        }
        tx.commit()?;
        Ok(KuaishouSubjectPage {
            items,
            total,
            offset: query.offset,
            limit: query.limit,
        })
    }

    /// Corrections retain immutable source evidence and photos, always return to pending review.
    /// They do not create identity archives or alter collection/slice completion history.
    pub fn kuaishou_subject_correct(
        &self,
        input: CorrectSubjectInput,
    ) -> Result<KuaishouSubjectArchive> {
        let tx = self.conn.unchecked_transaction()?;
        let mut archive = require_archive(&tx, &input.platform_user_id, input.expected_revision)?;
        if !bounded_field(&input.real_name, 100) || !bounded_field(&input.id_card, 128) {
            return Err(invalid("资料字段长度或格式不符合要求"));
        }
        archive.real_name = input.real_name.trim().into();
        archive.id_card = normalize_subject_id_card(&input.id_card);
        archive.review_status = SubjectReviewStatus::PendingReview;
        archive.confirmed_at = None;
        update_archive(&tx, &mut archive, input.expected_revision)?;
        tx.commit()?;
        Ok(archive)
    }

    /// Attachment owner must reread/hash/decode all files outside the DB transaction and pass
    /// their freshly verified metadata, NOT metadata deserialized from the renderer.
    pub fn kuaishou_subject_confirm(
        &self,
        input: ConfirmSubjectInput,
        verified_attachments: &[SubjectAttachment],
    ) -> Result<KuaishouSubjectArchive> {
        let tx = self.conn.unchecked_transaction()?;
        let mut archive = require_archive(&tx, &input.platform_user_id, input.expected_revision)?;
        if !validate_archive(&archive).can_confirm() {
            return Err(invalid("资料自动校验未通过，请修正后再确认"));
        }
        if !valid_subject_attachments(verified_attachments)
            || archive.attachments != verified_attachments
        {
            return Err(invalid("证件照片缺失或版本已变更，请重新读取后确认"));
        }
        archive.review_status = SubjectReviewStatus::Confirmed;
        archive.confirmed_at = Some(now());
        update_archive(&tx, &mut archive, input.expected_revision)?;
        tx.commit()?;
        Ok(archive)
    }

    /// Explicit user-requested re-OCR from already referenced photos, not an automatic step reset.
    /// Keeps original page evidence. Failed/partial text may be stored but cannot be confirmed.
    pub fn kuaishou_subject_update_ocr(
        &self,
        input: CorrectSubjectInput,
        verified_attachments: &[SubjectAttachment],
    ) -> Result<KuaishouSubjectArchive> {
        let tx = self.conn.unchecked_transaction()?;
        let mut archive = require_archive(&tx, &input.platform_user_id, input.expected_revision)?;
        if !valid_subject_attachments(verified_attachments)
            || archive.attachments != verified_attachments
        {
            return Err(invalid("证件照片缺失或版本已变更，请重新读取后识别"));
        }
        if !bounded_field(&input.real_name, 100) || !bounded_field(&input.id_card, 128) {
            return Err(invalid("识别字段长度或格式不符合要求"));
        }
        archive.real_name = input.real_name.trim().into();
        archive.id_card = normalize_subject_id_card(&input.id_card);
        archive.source = SubjectSource::Ocr;
        archive.review_status = SubjectReviewStatus::PendingReview;
        archive.confirmed_at = None;
        update_archive(&tx, &mut archive, input.expected_revision)?;
        tx.commit()?;
        Ok(archive)
    }

    pub fn kuaishou_subject_attachment_referenced(&self, key: &str) -> Result<bool> {
        if !valid_subject_attachment_key(key) {
            return Ok(false);
        }
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM kuaishou_subject_archives a, json_each(a.document,'$.attachments') x
                WHERE json_extract(x.value,'$.key')=?)", [key], |r| r.get(0))?)
    }
}
