//! Safe-CDP gate: refcount of which CDP domains are currently enabled.
//!
//! The former CloakBrowser-specific policy (`CLOAK_RISKY_ENABLE_DOMAINS`)
//! existed because the patched CloakBrowser build tripped a DCHECK when
//! `Runtime`/`Network` were enabled. Chromix does not have that restriction, so
//! the policy is now unconditional.
//!
//! Full enforcement (wrapping every chromiumoxide CDP call that could enable a
//! domain, or a custom `Client::connect` that suppresses `Runtime.enable`)
//! remains deferred — chromiumoxide auto-enables `Runtime` and `Page` on
//! `Browser::connect`/`new_page`. `BrowserSession::safe_enable_check` exposes the
//! refcount state as observability.

use std::collections::HashMap;
use std::sync::Mutex;

use multizen_core::BrowserEngine;

pub const SAFE_PAIRED_DISABLE_DOMAINS: &[&str] =
    &["Runtime", "Network", "DOM", "Accessibility", "Log", "Performance"];

pub struct SafeEnableRefcount {
    inner: Mutex<HashMap<String, u32>>,
}

impl SafeEnableRefcount {
    pub fn new() -> Self {
        Self { inner: Mutex::new(HashMap::new()) }
    }
    pub fn count(&self, domain: &str) -> u32 {
        *self.inner.lock().unwrap().get(domain).unwrap_or(&0)
    }
    /// True if this domain is not yet enabled (refcount == 0).
    pub fn should_enable(&self, domain: &str) -> bool {
        self.count(domain) == 0
    }
    /// True if a disable would bring refcount to 0 (i.e., current count == 1).
    pub fn should_disable(&self, domain: &str) -> bool {
        self.count(domain) == 1
    }
    pub fn enable(&self, domain: &str) {
        let mut m = self.inner.lock().unwrap();
        *m.entry(domain.to_string()).or_insert(0) += 1;
    }
    pub fn disable(&self, domain: &str) {
        let mut m = self.inner.lock().unwrap();
        if let Some(c) = m.get_mut(domain) {
            if *c > 0 {
                *c -= 1;
            }
        }
    }
}

impl Default for SafeEnableRefcount {
    fn default() -> Self { Self::new() }
}

/// Chromix allows every CDP domain. Kept as a named API so call sites and the
/// safe-enable gate stay explicit.
pub fn cloak_allows_domain(_domain: &str, _engine: BrowserEngine) -> bool {
    true
}
