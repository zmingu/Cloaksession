//! Account initialization contracts. Validation is not proof of legal identity.
//! Candidate/lease types are backend-only; do not deserialize them from IPC.
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubjectSource {
    MainTab,
    TalentTabPlaintext,
    Ocr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubjectReviewStatus {
    PendingReview,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationCheck {
    Passed,
    Failed,
    Unavailable,
}

/// Only field-level page evidence, never a document, URL, or OCR transcript.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubjectVisibleEvidence {
    pub real_name: Option<String>,
    pub id_card: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectValidation {
    pub name: ValidationCheck,
    pub structure: ValidationCheck,
    pub checksum: ValidationCheck,
    pub birth_date: ValidationCheck,
    pub visible_name: ValidationCheck,
    pub visible_id_card: ValidationCheck,
}

impl SubjectValidation {
    /// Absent visible evidence stays unavailable; it is NOT silently reported as passed.
    /// Manual photo review may confirm otherwise valid fields with unavailable evidence.
    pub fn can_confirm(&self) -> bool {
        self.name == ValidationCheck::Passed
            && self.structure == ValidationCheck::Passed
            && self.checksum == ValidationCheck::Passed
            && self.birth_date == ValidationCheck::Passed
            && self.visible_name != ValidationCheck::Failed
            && self.visible_id_card != ValidationCheck::Failed
    }
}

pub const SUBJECT_MAX_ATTACHMENTS: usize = 8;
pub const SUBJECT_MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const SUBJECT_MAX_IMAGE_DIMENSION: u32 = 10_000;
pub const SUBJECT_MAX_IMAGE_PIXELS: u64 = 40_000_000;

/// Content-addressed application-level image reference, never a filesystem path.
/// The attachment owner MUST decode actual bytes, verify hash, then atomically install
/// before passing this metadata to persistence. Metadata alone cannot prove file IO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubjectAttachment {
    pub key: String,
    pub sha256: String,
    pub mime_type: String,
    pub byte_len: u64,
    pub width: u32,
    pub height: u32,
}

impl SubjectAttachment {
    pub fn is_valid(&self) -> bool {
        let extension = match self.mime_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => return false,
        };
        valid_subject_attachment_key(&self.key)
            && self.key == format!("{}.{}", self.sha256, extension)
            && self.byte_len > 0
            && self.byte_len <= SUBJECT_MAX_IMAGE_BYTES
            && self.width > 0
            && self.height > 0
            && self.width <= SUBJECT_MAX_IMAGE_DIMENSION
            && self.height <= SUBJECT_MAX_IMAGE_DIMENSION
            && u64::from(self.width) * u64::from(self.height) <= SUBJECT_MAX_IMAGE_PIXELS
    }
}

pub fn valid_subject_attachment_key(key: &str) -> bool {
    key.split_once('.').is_some_and(|(hash, ext)| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            && matches!(ext, "png" | "jpg" | "webp")
    })
}

pub fn valid_subject_attachments(attachments: &[SubjectAttachment]) -> bool {
    !attachments.is_empty()
        && attachments.len() <= SUBJECT_MAX_ATTACHMENTS
        && attachments.iter().all(SubjectAttachment::is_valid)
        && attachments.iter().enumerate().all(|(i, a)| {
            !attachments[..i]
                .iter()
                .any(|previous| previous.key == a.key)
        })
}

#[derive(Clone, PartialEq, Eq)]
pub struct SubjectCandidate {
    pub real_name: String,
    pub id_card: String,
    pub source: SubjectSource,
    pub evidence: SubjectVisibleEvidence,
    pub attachments: Vec<SubjectAttachment>,
}

/// The only editable IPC fields. Photos/evidence/source/review/status are server-owned.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorrectSubjectInput {
    pub platform_user_id: String,
    pub expected_revision: u64,
    pub real_name: String,
    pub id_card: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmSubjectInput {
    pub platform_user_id: String,
    pub expected_revision: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouSubjectArchive {
    pub platform_user_id: String,
    pub real_name: String,
    pub id_card: String,
    pub source: SubjectSource,
    pub evidence: SubjectVisibleEvidence,
    pub attachments: Vec<SubjectAttachment>,
    pub validation: SubjectValidation,
    pub review_status: SubjectReviewStatus,
    pub revision: u64,
    pub source_profile_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouArchiveProfile {
    pub profile_id: String,
    pub name: String,
}

/// Lists intentionally omit full ID cards, photos, and page evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouSubjectSummary {
    pub platform_user_id: String,
    pub real_name: String,
    pub masked_id_card: String,
    pub nickname: Option<String>,
    pub avatar_key: Option<String>,
    pub source: SubjectSource,
    pub review_status: SubjectReviewStatus,
    pub revision: u64,
    pub updated_at: String,
    pub profiles: Vec<KuaishouArchiveProfile>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KuaishouSubjectQuery {
    pub search: String,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouSubjectPage {
    pub items: Vec<KuaishouSubjectSummary>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouSubjectDetail {
    pub archive: KuaishouSubjectArchive,
    pub nickname: Option<String>,
    pub avatar_key: Option<String>,
    pub profiles: Vec<KuaishouArchiveProfile>,
    pub steps: Vec<KuaishouInitStepRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KuaishouInitStep {
    Subject,
    Slice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KuaishouInitState {
    Pending,
    Running,
    Done,
    Failed,
}

/// Closed vocabulary: never persist platform errors, document text, or sensitive URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KuaishouInitErrorCode {
    InterruptedNeedsVerification,
    ContextChanged,
    TimedOut,
    PageUnsupported,
    PageCrashed,
    AttachmentUnavailable,
    OcrUnavailable,
    OcrFailed,
    ValidationFailed,
    PersistenceUnverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouInitStepRecord {
    pub platform_user_id: String,
    pub step: KuaishouInitStep,
    pub state: KuaishouInitState,
    pub attempts: u32,
    pub next_retry_at: Option<String>,
    pub last_error_code: Option<KuaishouInitErrorCode>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

/// Runtime must additionally hold/recheck its registry session through each DB call.
#[derive(Clone)]
pub struct KuaishouInitContext {
    pub platform_user_id: String,
    pub profile_id: String,
    pub session_id: String,
    pub expected_business: crate::BusinessProfileState,
}

/// Backend evidence only, never an IPC argument. Must be obtained after close/reload
/// and an exact account/session recheck, not from the transient toggled DOM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceVerification {
    pub platform_user_id: String,
    pub all_four_disabled: [bool; 4],
    pub persisted_readback: bool,
}

fn is_mask(c: char) -> bool {
    matches!(c, '*' | '＊' | '•' | '●')
}

pub fn normalize_subject_id_card(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

pub fn masked_subject_id_card(value: &str) -> String {
    // Never echo malformed text or large OCR transcripts in the summary.
    let value = normalize_subject_id_card(value);
    if value.len() == 18
        && value.bytes().take(17).all(|b| b.is_ascii_digit())
        && value.as_bytes()[17].is_ascii_alphanumeric()
    {
        format!("{}************{}", &value[..3], &value[15..])
    } else {
        "未完整识别".into()
    }
}

/// Mask runs stand for one or more hidden characters (platforms compress masks).
/// Match visible prefix/suffix and ordered inner fragments; empty/all-mask = unavailable.
pub fn compare_subject_visible(value: &str, evidence: Option<&str>) -> ValidationCheck {
    let Some(pattern) = evidence.map(str::trim).filter(|s| !s.is_empty()) else {
        return ValidationCheck::Unavailable;
    };
    if pattern.chars().all(is_mask) {
        return ValidationCheck::Unavailable;
    }
    if pattern.chars().count() > 100 || pattern.chars().any(char::is_control) {
        return ValidationCheck::Failed;
    }
    let chars: Vec<char> = value.chars().collect();
    let mut dp = vec![false; chars.len() + 1];
    dp[0] = true;
    let mut previous_mask = false;
    for c in pattern.chars() {
        if is_mask(c) && previous_mask {
            continue;
        }
        let mut next = vec![false; chars.len() + 1];
        if is_mask(c) {
            let mut matched = false;
            for i in 1..=chars.len() {
                matched |= dp[i - 1];
                next[i] = matched;
            }
        } else {
            for i in 1..=chars.len() {
                next[i] = dp[i - 1] && chars[i - 1] == c;
            }
        }
        previous_mask = is_mask(c);
        dp = next;
    }
    if dp[chars.len()] {
        ValidationCheck::Passed
    } else {
        ValidationCheck::Failed
    }
}

fn calendar_date(y: u32, m: u32, d: u32) -> bool {
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    y > 0 && d > 0 && d <= days
}

/// `today` is a backend supplied UTC YYYY-MM-DD, never user input.
/// No address-registry lookup and no legal-authenticity claim is made.
pub fn validate_subject_fields(
    real_name: &str,
    id_card: &str,
    evidence: &SubjectVisibleEvidence,
    today: &str,
) -> SubjectValidation {
    use ValidationCheck::{Failed, Passed, Unavailable};
    let name = real_name.trim();
    let card = normalize_subject_id_card(id_card);
    let name_ok = !name.is_empty()
        && name.chars().count() <= 100
        && name.chars().any(char::is_alphabetic)
        && name
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '·' | '・' | ' ' | '-'));
    let structure = card.len() == 18
        && card.bytes().take(17).all(|c| c.is_ascii_digit())
        && (card.as_bytes()[17].is_ascii_digit() || card.as_bytes()[17] == b'X')
        && card.as_bytes()[0] != b'0'
        && &card[14..17] != "000";
    let mut out = SubjectValidation {
        name: if name_ok { Passed } else { Failed },
        structure: if structure { Passed } else { Failed },
        checksum: Unavailable,
        birth_date: Unavailable,
        visible_name: compare_subject_visible(name, evidence.real_name.as_deref()),
        visible_id_card: compare_subject_visible(
            &card,
            evidence
                .id_card
                .as_deref()
                .map(str::trim)
                .map(str::to_ascii_uppercase)
                .as_deref(),
        ),
    };
    if structure {
        let weights = [7u32, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
        let sum: u32 = card
            .bytes()
            .take(17)
            .zip(weights)
            .map(|(c, w)| u32::from(c - b'0') * w)
            .sum();
        out.checksum = if b"10X98765432"[(sum % 11) as usize] == card.as_bytes()[17] {
            Passed
        } else {
            Failed
        };
        let y = card[6..10].parse::<u32>().unwrap_or(0);
        let m = card[10..12].parse::<u32>().unwrap_or(0);
        let d = card[12..14].parse::<u32>().unwrap_or(0);
        let today_parts: Vec<_> = today
            .split('-')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        out.birth_date = if today.len() == 10
            && today.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
            && today_parts.len() == 3
            && calendar_date(today_parts[0], today_parts[1], today_parts[2])
            && calendar_date(y, m, d)
            && (y, m, d) <= (today_parts[0], today_parts[1], today_parts[2])
        {
            Passed
        } else {
            Failed
        };
    }
    out
}

macro_rules! redacted_debug {
    ($($t:ty),+ $(,)?) => {$(
        impl fmt::Debug for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($t), " { [资料已脱敏] }"))
            }
        }
    )+};
}
redacted_debug!(
    SubjectVisibleEvidence,
    SubjectCandidate,
    CorrectSubjectInput,
    KuaishouSubjectArchive,
    KuaishouInitContext,
    KuaishouSubjectQuery
);
