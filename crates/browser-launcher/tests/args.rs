use browser_launcher::args::build_spawn_args;
use multizen_core::{BrowserEngine, Profile};

fn base_profile() -> Profile {
    use multizen_core::*;
    Profile {
        id: "p1".into(),
        name: "t".into(),
        notes: None,
        tags: vec![],
        proxy: None,
        fingerprint: FingerprintConfig {
            device: DeviceFamily::WindowsDesktopIntel,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/148".into(),
            platform: "Win32".into(),
            client_hints: ClientHints {
                sec_ch_ua: r#""Chromium";v="148", "Google Chrome";v="148", "Not?A_Brand";v="99""#.into(),
                sec_ch_ua_platform: "Windows".into(),
                sec_ch_ua_platform_version: "10.0.0".into(),
                sec_ch_ua_arch: "x86".into(),
                sec_ch_ua_bitness: "64".into(),
                sec_ch_ua_mobile: "?0".into(),
                sec_ch_ua_model: "".into(),
                sec_ch_ua_full_version_list: r#""Chromium";v="148.0.0.0", "Google Chrome";v="148.0.0.0", "Not?A_Brand";v="99.0.0.0""#.into(),
            },
            locale: "en-US".into(),
            languages: vec!["en-US".into(), "en".into()],
            accept_language: "en-US,en;q=0.9".into(),
            timezone: "America/New_York".into(),
            country: "US".into(),
            screen: multizen_core::ScreenSize { width: 1920, height: 1080 },
            avail_screen: Some(multizen_core::ScreenSize { width: 1920, height: 1040 }),
            dpr: 1.0,
            webgl: multizen_core::WebGlConfig {
                vendor: "Google Inc. (Intel)".into(),
                renderer: "ANGLE (Intel UHD)".into(),
            },
            hardware_concurrency: 8,
            device_memory: 8,
            fonts_dir: None,
            storage_quota: Some(2_000_000_000),
            seed: Some("abc".into()),
        },
        chromix_options: Default::default(),
        extensions: None,
        icon: None,
        start_url: Some("https://example.com".into()),
        search_provider: None,
        data_dir: "/tmp/p1".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        last_opened_at: None,
        proxy_country: None,
        group: None,
    }
}

/// Chromix is the only engine: the native spawn args are exactly the CDP
/// endpoint contract. No CloakBrowser `--fingerprint-*`, no CFT `--user-agent`
/// / `--test-type`, no proxy/DNS flags leak through.
#[test]
fn chromix_args_are_only_the_cdp_endpoint_contract() {
    let profile = base_profile();
    let args = build_spawn_args(
        &profile,
        BrowserEngine::Chromix,
        12345,
        "/profile/engines/chromix",
        Some("socks5://127.0.0.1:1080"),
        Some((40.7, -74.0)),
        Some("/companion"),
        false,
    );
    assert_eq!(args, vec![
        "--user-data-dir=/profile/engines/chromix",
        "--remote-debugging-address=127.0.0.1",
        "--remote-debugging-port=12345",
    ]);
    assert!(args.iter().all(|a| !a.starts_with("--fingerprint")));
    assert!(args.iter().all(|a| !a.starts_with("--user-agent")));
    assert!(args.iter().all(|a| a != "--test-type=gpu"));
    assert!(args.iter().all(|a| !a.starts_with("--proxy-server")));
}

/// Hidden launch and every fingerprint/proxy input are irrelevant to the native
/// args for Chromix: they are owned by the SDK bridge (`launch_with_chromix`
/// appends `--window-position` into `launchOptions.args`). This locks in that no
/// engine-specific flag can reappear here.
#[test]
fn chromix_args_ignore_hidden_and_fingerprint_inputs() {
    let profile = base_profile();
    let visible = build_spawn_args(&profile, BrowserEngine::Chromix, 9222, "/d", None, None, None, false);
    let hidden = build_spawn_args(&profile, BrowserEngine::Chromix, 9222, "/d", None, None, None, true);
    assert_eq!(visible, hidden);
    assert!(visible.iter().all(|a| !a.starts_with("--window-position")));
    assert!(visible.iter().all(|a| !a.starts_with("--headless")));
    assert!(visible.iter().any(|a| a == "--user-data-dir=/d"));
    assert!(visible.iter().any(|a| a == "--remote-debugging-port=9222"));
}
