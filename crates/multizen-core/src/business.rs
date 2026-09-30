//! Manually registered business metadata. This is never evidence of login.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BusinessAccountKind {
    KuaishouShop,
    KuaishouLive,
    KuaishouMate,
    KuaishouSub,
    Jinniu,
}

impl BusinessAccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::KuaishouShop => "kuaishou-shop",
            Self::KuaishouLive => "kuaishou-live",
            Self::KuaishouMate => "kuaishou-mate",
            Self::KuaishouSub => "kuaishou-sub",
            Self::Jinniu => "jinniu",
        }
    }

    pub fn scope(self) -> BusinessProfileScope {
        match self {
            Self::Jinniu => BusinessProfileScope::Jinniu,
            _ => BusinessProfileScope::Kuaishou,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BusinessProfileScope {
    Jinniu,
    Kuaishou,
}

impl BusinessProfileScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jinniu => "jinniu",
            Self::Kuaishou => "kuaishou",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessAccount {
    pub id: String,
    pub kind: BusinessAccountKind,
    pub display_name: String,
    pub platform_user_id: Option<String>,
    pub profile_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessProfileState {
    pub account: Option<BusinessAccount>,
    pub scope: Option<BusinessProfileScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBusinessAccountInput {
    #[serde(default)]
    pub id: Option<String>,
    pub profile_id: String,
    pub kind: BusinessAccountKind,
    pub display_name: String,
    pub platform_user_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wire_contract_defaults_and_explicit_nulls() {
        let input: SaveBusinessAccountInput = serde_json::from_value(json!({
            "profileId":"p", "kind":"kuaishou-shop", "displayName":"Alias", "platformUserId":null
        }))
        .unwrap();
        assert!(input.id.is_none());
        let value = serde_json::to_value(input).unwrap();
        assert_eq!(value["id"], json!(null));
        assert_eq!(value["profileId"], "p");
        assert_eq!(
            serde_json::to_value(BusinessProfileState {
                account: None,
                scope: None
            })
            .unwrap(),
            json!({"account":null,"scope":null})
        );
        for kind in [
            BusinessAccountKind::KuaishouShop,
            BusinessAccountKind::KuaishouLive,
            BusinessAccountKind::KuaishouMate,
            BusinessAccountKind::KuaishouSub,
            BusinessAccountKind::Jinniu,
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
        assert!(serde_json::from_value::<BusinessAccountKind>(json!("shop")).is_err());
        let a = BusinessAccount {
            id: "a".into(),
            kind: BusinessAccountKind::Jinniu,
            display_name: "J".into(),
            platform_user_id: None,
            profile_id: None,
            created_at: "2026-09-30T00:00:00Z".into(),
            updated_at: "2026-09-30T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 7);
        assert!(v.get("profileId").unwrap().is_null());
        assert!(v.get("platformUserId").unwrap().is_null());
        assert_eq!(serde_json::from_value::<BusinessAccount>(v).unwrap(), a);
    }
}
