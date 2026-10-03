use multizen_core::{AppLanguage, AppSettings};
use serde_json::{Value, json};
use settings_store::{SettingsStore, default_settings_path};
use tempfile::TempDir;

fn load_raw(raw: &str) -> AppSettings {
    let dir = TempDir::new().unwrap();
    let path = default_settings_path(dir.path());
    std::fs::write(&path, raw).unwrap();
    SettingsStore::new(&path).load().unwrap()
}

fn load_value(value: Value) -> AppSettings {
    load_raw(&serde_json::to_string(&value).unwrap())
}

#[test]
fn default_language_is_zh_cn_on_model_and_wire() {
    assert_eq!(AppSettings::default().language, AppLanguage::ZhCn);
    let wire = serde_json::to_value(AppSettings::default()).unwrap();
    assert_eq!(wire["language"], json!("zh-CN"));
    let mut full = serde_json::to_value(AppSettings::default()).unwrap();
    full["language"] = json!("en");
    assert_eq!(
        serde_json::from_value::<AppSettings>(full).unwrap().language,
        AppLanguage::En
    );
}

#[test]
fn missing_file_loads_zh_cn() {
    let dir = TempDir::new().unwrap();
    let mut store = SettingsStore::new(&default_settings_path(dir.path()));
    assert_eq!(store.load().unwrap().language, AppLanguage::ZhCn);
}

#[test]
fn old_config_without_language_loads_zh_cn_and_keeps_other_fields() {
    let s = load_raw(
        r#"{"theme":"light","mcpHttpPort":8888,"browserEngine":"cft","browserBinaryPath":"/local/chrome"}"#,
    );
    assert_eq!(s.language, AppLanguage::ZhCn);
    assert_eq!(s.theme, "light");
    assert_eq!(s.mcp_http_port, 8888);
    assert_eq!(s.browser_binary_path.as_deref(), Some("/local/chrome"));
}

#[test]
fn unknown_language_value_falls_back_without_clearing_others() {
    let s = load_value(json!({
        "language": "fr",
        "mcpHttpPort": 8888,
        "browserEngine": "cft",
        "browserBinaryPath": "/local/chrome",
    }));
    assert_eq!(s.language, AppLanguage::ZhCn);
    assert_eq!(s.mcp_http_port, 8888);
    assert_eq!(s.browser_binary_path.as_deref(), Some("/local/chrome"));
}

#[test]
fn mistyped_or_null_language_falls_back_without_clearing_others() {
    for bad in [
        Value::Null,
        json!(42),
        json!(true),
        json!(["zh-CN"]),
        json!({"code": "zh-CN"}),
    ] {
        let s = load_value(json!({
            "language": bad,
            "mcpHttpPort": 8888,
            "browserEngine": "chromix",
        }));
        assert_eq!(s.language, AppLanguage::ZhCn, "bad value: {bad}");
        assert_eq!(s.mcp_http_port, 8888, "bad value: {bad}");
    }
}

#[test]
fn known_language_values_load() {
    assert_eq!(load_value(json!({"language": "en"})).language, AppLanguage::En);
    assert_eq!(
        load_value(json!({"language": "zh-CN"})).language,
        AppLanguage::ZhCn
    );
}

#[test]
fn update_then_reopen_preserves_language_and_cache() {
    let dir = TempDir::new().unwrap();
    let path = default_settings_path(dir.path());
    let mut store = SettingsStore::new(&path);
    assert_eq!(store.load().unwrap().language, AppLanguage::ZhCn);

    let mut next = store.load().unwrap();
    next.language = AppLanguage::En;
    next.mcp_http_port = 9999;
    store.update(next).unwrap();
    // Cache hit without re-reading the file.
    assert_eq!(store.load().unwrap().language, AppLanguage::En);

    let mut reopened = SettingsStore::new(&path);
    let s = reopened.load().unwrap();
    assert_eq!(s.language, AppLanguage::En);
    assert_eq!(s.mcp_http_port, 9999);
}

#[test]
fn write_failure_surfaces_error_without_poisoning_cache() {
    // Point the store at a directory so the write fails; the failed patch
    // must not land in the cache.
    let dir = TempDir::new().unwrap();
    let mut store = SettingsStore::new(dir.path());
    let before = store.load().unwrap();
    assert_eq!(before.language, AppLanguage::ZhCn);

    let attempted = AppSettings {
        language: AppLanguage::En,
        mcp_http_port: 9999,
        ..Default::default()
    };
    assert!(store.update(attempted).is_err());
    let after = store.load().unwrap();
    assert_eq!(after.language, AppLanguage::ZhCn);
    assert_eq!(after.mcp_http_port, before.mcp_http_port);
}

#[test]
fn valid_chromix_config_not_lost_with_bad_language() {
    let dir = TempDir::new().unwrap();
    let path = default_settings_path(dir.path());
    let mut settings = AppSettings {
        language: AppLanguage::En,
        ..Default::default()
    };
    settings.chromix.node_path = "/custom/node".into();
    settings.chromix.options = json!({
        "headless": false,
        "args": ["--fingerprint=18446744073709551615"],
        "futureSdkOption": {"enabled": true},
    })
    .as_object()
    .unwrap()
    .clone();
    settings
        .chromix
        .environment
        .insert("CHROMIX_CACHE_DIR".into(), "/custom/cache".into());
    SettingsStore::new(&path).update(settings.clone()).unwrap();

    // Corrupt only the language field on disk; valid chromix config survives.
    let mut disk: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    disk["language"] = json!("xx-unknown");
    std::fs::write(&path, serde_json::to_string_pretty(&disk).unwrap()).unwrap();

    let actual = SettingsStore::new(&path).load().unwrap();
    assert_eq!(actual.language, AppLanguage::ZhCn);
    assert_eq!(actual.chromix.node_path, "/custom/node");
    assert_eq!(actual.chromix.options, settings.chromix.options);
    assert_eq!(actual.chromix.environment, settings.chromix.environment);
}
