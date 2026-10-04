use multizen_core::{BrowserEngine, FingerprintConfig, Result};

use crate::session::BrowserSession;

/// Chromix owns identity and context emulation inside its SDK bridge, so there
/// is no CDP-level bootstrap to apply. The former CFT/CloakBrowser emulation
/// paths (WebRTC spoof, fingerprint preload, UA override, locale evaluate) were
/// removed together with those engines. The signature is kept so callers do not
/// need to change.
pub async fn bootstrap_targets(
    _session: &BrowserSession,
    _fp: &FingerprintConfig,
    _engine: BrowserEngine,
    _webrtc_spoof_ip: Option<&str>,
) -> Result<()> {
    Ok(())
}
