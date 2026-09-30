//! Read-only page observations, distinct from manually registered business accounts.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KuaishouIdentityStatus {
    Unknown,
    Detected,
    NotDetected,
    Conflict,
    Closed,
    Skipped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KuaishouIdentitySnapshot {
    pub profile_id: String,
    pub status: KuaishouIdentityStatus,
    pub platform_user_id: Option<String>,
    pub nickname: Option<String>,
    pub avatar_key: Option<String>,
    pub checked_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub message: Option<String>,
}

impl KuaishouIdentitySnapshot {
    pub fn empty(profile_id: impl Into<String>, status: KuaishouIdentityStatus) -> Self {
        Self {
            profile_id: profile_id.into(),
            status,
            platform_user_id: None,
            nickname: None,
            avatar_key: None,
            checked_at: None,
            last_seen_at: None,
            message: None,
        }
    }
}

/// Internal persistence envelope; never sent to the renderer (in particular avatar_url).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KuaishouIdentityObservation {
    pub snapshot: KuaishouIdentitySnapshot,
    pub session_id: Option<String>,
    pub avatar_url: Option<String>,
}

pub fn valid_kuaishou_user_id(id: &str) -> bool {
    (5..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

pub fn kuaishou_identity_allowed(state: &crate::BusinessProfileState) -> bool {
    state.scope != Some(crate::BusinessProfileScope::Jinniu)
        && state
            .account
            .as_ref()
            .is_none_or(|a| a.kind == crate::BusinessAccountKind::KuaishouShop)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_wire_has_exact_fields_and_explicit_nulls() {
        let value = serde_json::to_value(KuaishouIdentitySnapshot::empty(
            "p",
            KuaishouIdentityStatus::NotDetected,
        ))
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({"profileId":"p","status":"not-detected","platformUserId":null,"nickname":null,"avatarKey":null,"checkedAt":null,"lastSeenAt":null,"message":null})
        );
        for (state, text) in [
            (KuaishouIdentityStatus::Unknown, "unknown"),
            (KuaishouIdentityStatus::Detected, "detected"),
            (KuaishouIdentityStatus::Conflict, "conflict"),
            (KuaishouIdentityStatus::Closed, "closed"),
            (KuaishouIdentityStatus::Skipped, "skipped"),
            (KuaishouIdentityStatus::Error, "error"),
        ] {
            assert_eq!(serde_json::to_value(state).unwrap(), text);
        }
    }
    #[test]
    fn numeric_id_is_full_ascii_and_bounded() {
        for id in [
            "1234",
            "12345x",
            "x12345",
            "１２３４５",
            "12345\n",
            "123456789012345678901234567890123",
        ] {
            assert!(!valid_kuaishou_user_id(id));
        }
        assert!(valid_kuaishou_user_id("0012345"));
        assert!(valid_kuaishou_user_id("12345678901234567890123456789012"));
    }
}
