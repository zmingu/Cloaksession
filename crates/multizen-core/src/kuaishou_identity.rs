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

/// Kuaishou main-site (viewer) user id. The main site caches a principal id
/// (`localStorage.userId`) that is alphanumeric in practice — jieger normalizes
/// it with `[A-Za-z0-9_.-]{3,80}` — so a numeric-only rule would reject every
/// real viewer. Bounded to 32 here, and kept separate from the shop's
/// numeric-only [`valid_kuaishou_user_id`].
pub fn valid_kuaishou_viewer_id(id: &str) -> bool {
    (3..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// Identity persistence / id gate. A profile bound to the shop keeps the strict
/// numeric form; a viewer (sub) account — and an unbound profile, which is the
/// state the onboarding wizard detects in before it saves the account — also
/// accepts the main-site id form.
pub fn valid_kuaishou_identity_id(state: &crate::BusinessProfileState, id: &str) -> bool {
    valid_kuaishou_user_id(id)
        || (state
            .account
            .as_ref()
            .is_none_or(|a| a.kind == crate::BusinessAccountKind::KuaishouSub)
            && valid_kuaishou_viewer_id(id))
}

/// Identity detection is allowed for shop accounts and viewer sub-accounts.
/// Deliberately separate from [`kuaishou_identity_allowed`], which gates
/// shop-only flows (auto-popup / account initialization): widening viewer
/// support here cannot leak into those gates.
pub fn kuaishou_identity_detection_allowed(state: &crate::BusinessProfileState) -> bool {
    state.scope != Some(crate::BusinessProfileScope::Jinniu)
        && state.account.as_ref().is_none_or(|a| {
            matches!(
                a.kind,
                crate::BusinessAccountKind::KuaishouShop | crate::BusinessAccountKind::KuaishouSub
            )
        })
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

    fn bound(kind: crate::BusinessAccountKind) -> crate::BusinessProfileState {
        crate::BusinessProfileState {
            scope: Some(kind.scope()),
            account: Some(crate::BusinessAccount {
                id: "a".into(),
                kind,
                display_name: "A".into(),
                platform_user_id: None,
                profile_id: Some("p".into()),
                created_at: "2026-09-30T00:00:00Z".into(),
                updated_at: "2026-09-30T00:00:00Z".into(),
            }),
        }
    }
    fn unbound(scope: Option<crate::BusinessProfileScope>) -> crate::BusinessProfileState {
        crate::BusinessProfileState {
            account: None,
            scope,
        }
    }

    #[test]
    fn viewer_id_is_alphanumeric_and_bounded() {
        for id in [
            "",
            "ab",
            "a b",
            "快手",
            "a\nb",
            "abcdefghijklmnopqrstuvwxyz0123456789",
        ] {
            assert!(!valid_kuaishou_viewer_id(id), "{id}");
        }
        assert!(valid_kuaishou_viewer_id("3x7abcdef"));
        assert!(valid_kuaishou_viewer_id("12345"));
        assert!(valid_kuaishou_viewer_id("a.b-c_d"));
    }

    #[test]
    fn identity_id_gate_distinguishes_shop_and_viewer() {
        let shop = bound(crate::BusinessAccountKind::KuaishouShop);
        let sub = bound(crate::BusinessAccountKind::KuaishouSub);
        let none = unbound(None);
        assert!(valid_kuaishou_identity_id(&shop, "12345"));
        assert!(!valid_kuaishou_identity_id(&shop, "3xabcde"));
        assert!(valid_kuaishou_identity_id(&sub, "12345"));
        assert!(valid_kuaishou_identity_id(&sub, "3xabcde"));
        assert!(valid_kuaishou_identity_id(&none, "3xabcde"));
        assert!(!valid_kuaishou_identity_id(&none, "ab"));
    }

    #[test]
    fn detection_allows_shop_and_viewer_but_not_jinniu_or_other_kinds() {
        use crate::BusinessAccountKind::*;
        assert!(kuaishou_identity_detection_allowed(&unbound(None)));
        assert!(kuaishou_identity_detection_allowed(&bound(KuaishouShop)));
        assert!(kuaishou_identity_detection_allowed(&bound(KuaishouSub)));
        assert!(!kuaishou_identity_detection_allowed(&bound(KuaishouLive)));
        assert!(!kuaishou_identity_detection_allowed(&bound(KuaishouMate)));
        assert!(!kuaishou_identity_detection_allowed(&bound(Jinniu)));
        assert!(!kuaishou_identity_detection_allowed(&unbound(Some(
            crate::BusinessProfileScope::Jinniu
        ))));
        // The shop-only gate (auto-popup / initialization) must stay unchanged.
        assert!(kuaishou_identity_allowed(&bound(KuaishouShop)));
        assert!(!kuaishou_identity_allowed(&bound(KuaishouSub)));
    }
}
