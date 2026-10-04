use browser_launcher::data_dir::{default_data_dir, effective_data_dir, verify_data_dir};
use multizen_core::{BrowserEngine as Engine, ChromixSettings, CreateProfileInput};
use profile_manager::ProfileManager;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn default_and_real_shallow_merged_overrides() {
    let d = TempDir::new().unwrap();
    let pm = ProfileManager::new(&d.path().join("p.db"), &d.path().join("profiles")).unwrap();
    let mut p = pm
        .create(CreateProfileInput {
            name: "P".into(),
            ..Default::default()
        })
        .unwrap();
    let mut global = ChromixSettings::default();
    assert_eq!(
        default_data_dir(&p, Engine::Chromix),
        std::path::Path::new(&p.data_dir).join("engines/chromix")
    );
    assert_eq!(
        effective_data_dir(&p, Engine::Chromix, &global).unwrap(),
        default_data_dir(&p, Engine::Chromix)
    );
    global.options=json!({"userDataDir":d.path().join("global"),"launchOptions":{"userDataDir":"invalid"},"unknown":[false,null]}).as_object().unwrap().clone();
    assert!(effective_data_dir(&p, Engine::Chromix, &global).is_err());
    p.chromix_options = json!({"launchOptions":{"slowMo":3}})
        .as_object()
        .unwrap()
        .clone();
    let config = global.with_profile_options(&p.chromix_options);
    assert_eq!(
        effective_data_dir(&p, Engine::Chromix, &config).unwrap(),
        d.path().join("global")
    );
    assert_eq!(config.options["unknown"], json!([false, null]));
    p.chromix_options
        .insert("userDataDir".into(), json!(d.path().join("profile")));
    assert_eq!(
        effective_data_dir(
            &p,
            Engine::Chromix,
            &global.with_profile_options(&p.chromix_options)
        )
        .unwrap(),
        d.path().join("profile")
    );
}

#[test]
fn nested_and_raw_directory_overrides_are_not_silently_ignored() {
    let d = TempDir::new().unwrap();
    let pm = ProfileManager::new(&d.path().join("p.db"), &d.path().join("profiles")).unwrap();
    let p = pm.create(CreateProfileInput::default()).unwrap();
    for options in [
        json!({"userDataDir":null}),
        json!({"userDataDir":" "}),
        json!({"contextOptions":{"userDataDir":"x"}}),
        json!({"launchOptions":null}),
        json!({"args":[" --USER-DATA-DIR=x"]}),
        json!({"contextOptions":{"ignoreDefaultArgs":["-profile-directory", "Default"]}}),
        json!({"launchOptions":{"args":["--user-data-dir x"]}}),
        json!({"args":[42]}),
    ] {
        let mut config = ChromixSettings::default();
        config.options = options.as_object().unwrap().clone();
        assert!(
            effective_data_dir(&p, Engine::Chromix, &config).is_err(),
            "{options}"
        );
    }
}

#[test]
fn nearest_existing_parent_dotdot_and_component_boundaries() {
    let d = TempDir::new().unwrap();
    let a = verify_data_dir(&d.path().join("fresh/sub/../cookies")).unwrap();
    let b = verify_data_dir(&d.path().join("fresh/cookies")).unwrap();
    assert!(a.same_directory(&b));
    assert!(a.overlaps(&verify_data_dir(&d.path().join("fresh")).unwrap()));
    assert!(!a.overlaps(&verify_data_dir(&d.path().join("fresh/cookies-other")).unwrap()));
    std::fs::write(d.path().join("file"), "x").unwrap();
    assert!(verify_data_dir(&d.path().join("file/child")).is_err());
    assert!(verify_data_dir(std::path::Path::new("")).is_err());
}

#[cfg(windows)]
#[test]
fn windows_case_slashes_extended_and_dotdot_are_same_identity() {
    let d = TempDir::new().unwrap();
    let raw = d.path().join("Absent/../Cookies");
    let a = verify_data_dir(&raw).unwrap();
    let lower = raw.to_str().unwrap().to_lowercase().replace('\\', "/");
    assert!(a.same_directory(&verify_data_dir(std::path::Path::new(&lower)).unwrap()));
    let extended = format!("\\\\?\\{}", raw.display());
    assert!(a.same_directory(&verify_data_dir(std::path::Path::new(&extended)).unwrap()));
    for invalid in [
        r"C:relative",
        r"\\server\share\dir",
        r"\\?\UNC\server\share",
        r"\\.\PIPE\x",
        r"C:\a:stream",
        r"C:\dir.\x",
        r"C:\NUL\child",
        r"\root-relative",
    ] {
        assert!(
            verify_data_dir(std::path::Path::new(invalid)).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn symlink_or_junction_alias_and_missing_suffix_are_resolved() {
    let d = TempDir::new().unwrap();
    let real = d.path().join("real");
    let alias = d.path().join("alias");
    std::fs::create_dir(&real).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    #[cfg(windows)]
    {
        // Junctions need no developer-mode/symlink privilege. Temp directories only.
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&alias)
            .arg(&real)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let a = verify_data_dir(&alias.join("missing/cookies")).unwrap();
    let b = verify_data_dir(&real.join("missing/cookies")).unwrap();
    assert!(a.same_directory(&b));
    assert!(verify_data_dir(&alias.join("../cookies")).is_err());
}
