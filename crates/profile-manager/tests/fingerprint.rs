use profile_manager::fingerprint::default_fingerprint;

#[test]
fn default_is_self_consistent_and_keeps_neutral_locale() {
    let fp = default_fingerprint("abc");
    // locale/timezone/country stay neutral (the caller realigns them to the
    // proxy's exit region), regardless of which device persona was drawn.
    assert_eq!(fp.locale, "en-US");
    assert_eq!(fp.timezone, "America/New_York");
    assert_eq!(fp.country, "US");
    assert_eq!(fp.seed, Some("abc".to_string()));
    // Whatever persona is chosen, the fields must be coherent.
    assert!(fp.user_agent.contains("Chrome/148"));
    assert!(fp.client_hints.sec_ch_ua.contains("Chrome"));
    assert!(fp.dpr >= 1.0);
    assert!(fp.hardware_concurrency >= 1);
    assert!(fp.device_memory >= 1);
    assert!(fp.screen.width > 0 && fp.screen.height > 0);
    assert!(!fp.webgl.vendor.is_empty());
    assert!(!fp.webgl.renderer.is_empty());
    assert!(fp.avail_screen.is_some());
}

#[test]
fn same_seed_is_deterministic_and_different_seeds_vary() {
    // A profile id is a stable seed: re-generating must not change the persona.
    assert_eq!(
        serde_json::to_string(&default_fingerprint("profile-1")).unwrap(),
        serde_json::to_string(&default_fingerprint("profile-1")).unwrap(),
    );

    // Across many seeds we must see more than one device family — otherwise the
    // "random" button would be a no-op.
    let families: std::collections::HashSet<String> = (0..64)
        .map(|i| {
            serde_json::to_value(default_fingerprint(&format!("seed-{i}")))
                .unwrap()
                .get("device")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert!(families.len() > 1, "expected varied device families, got {families:?}");
}

#[test]
fn empty_seed_draws_fresh_entropy_each_call() {
    // The UI's "random" button passes an empty seed; two calls must differ.
    let a = default_fingerprint("");
    let b = default_fingerprint("");
    assert_ne!(a.seed, b.seed);
    assert_ne!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
    );
}

#[test]
fn screen_and_webgl_populated() {
    let fp = default_fingerprint("x");
    assert!(fp.screen.width > 0 && fp.screen.height > 0);
    assert!(!fp.webgl.vendor.is_empty());
    assert!(!fp.webgl.renderer.is_empty());
}
