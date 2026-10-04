use multizen_core::{BrowserEngine, Profile};

/// Chromix is the only engine. The bundled Node SDK bridge owns every
/// launch-affecting option (proxy, extensions, window position, fingerprint),
/// so the native spawn args are only the CDP endpoint contract. The signature
/// is kept stable for callers/tests; the now-unused parameters are ignored.
pub fn build_spawn_args(
    _profile: &Profile,
    _engine: BrowserEngine,
    port: u16,
    browser_data_dir: &str,
    _proxy_bridge_url: Option<&str>,
    _geo_coords: Option<(f64, f64)>,
    _companion_dir: Option<&str>,
    _hidden: bool,
) -> Vec<String> {
    vec![
        format!("--user-data-dir={browser_data_dir}"),
        "--remote-debugging-address=127.0.0.1".into(),
        format!("--remote-debugging-port={port}"),
    ]
}
