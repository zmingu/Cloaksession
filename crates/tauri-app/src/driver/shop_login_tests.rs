use super::*;

#[tokio::test]
async fn shop_login_guard_preserves_custom_urls_and_rejects_jinniu_even_after_unbind() {
    let (_dir, driver) = crate::driver::business_tests::fixture(ChromixSettings::default());
    let profile = driver.create_profile(CreateProfileInput {
        name: "shop launch fixture".into(),
        start_url: Some("https://example.com/custom".into()),
        ..Default::default()
    }).await.unwrap();
    let before = serde_json::to_value(&profile).unwrap();
    driver.require_kuaishou_login_scope(&profile.id).await.unwrap();
    assert_eq!(serde_json::to_value(driver.get_profile(&profile.id).await.unwrap().unwrap()).unwrap(), before);
    assert!(driver.require_kuaishou_login_scope("absent").await.is_err());
    let account = driver.business_accounts_save(multizen_core::SaveBusinessAccountInput {
        id: None, profile_id: profile.id.clone(), kind: multizen_core::BusinessAccountKind::Jinniu,
        display_name: "Jinniu".into(), platform_user_id: None,
    }).await.unwrap();
    assert!(driver.require_kuaishou_login_scope(&profile.id).await.unwrap_err().to_string().contains("金牛"));
    driver.business_accounts_unbind(&account.id).await.unwrap();
    assert!(driver.require_kuaishou_login_scope(&profile.id).await.is_err());
    let after = driver.get_profile(&profile.id).await.unwrap().unwrap();
    assert_eq!(after.start_url, profile.start_url);
    assert!(after.last_opened_at.is_none());
    assert!(driver.registry().get(&profile.id).await.is_none());
    let shop = driver.create_profile(CreateProfileInput {
        name: "registered shop".into(), ..Default::default()
    }).await.unwrap();
    driver.business_accounts_save(multizen_core::SaveBusinessAccountInput {
        id: None, profile_id: shop.id.clone(), kind: multizen_core::BusinessAccountKind::KuaishouShop,
        display_name: "Shop".into(), platform_user_id: None,
    }).await.unwrap();
    driver.require_kuaishou_login_scope(&shop.id).await.unwrap();
    assert!(driver.get_profile(&shop.id).await.unwrap().unwrap().last_opened_at.is_none());
    driver.shutdown().await;
}

#[test]
fn launch_entry_wire_is_explicit_and_url_is_login_not_post_login_home() {
    use crate::commands::profiles::{LaunchEntry, KUAISHOU_LOGIN_URL};
    assert!(serde_json::from_value::<LaunchEntry>(serde_json::json!("kuaishou-shop")).is_ok());
    assert!(serde_json::from_value::<LaunchEntry>(serde_json::json!("jinniu")).is_err());
    assert!(serde_json::from_value::<Option<LaunchEntry>>(serde_json::Value::Null).unwrap().is_none());
    assert_eq!(KUAISHOU_LOGIN_URL, "https://login.kwaixiaodian.com/?biz=zone&redirect_url=https%3A%2F%2Fs.kwaixiaodian.com%2Fzone%2Fhome");
}
