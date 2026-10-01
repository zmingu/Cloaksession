# Engine Lifecycle and Legacy Launch

## Ownership and data flow

`crates/browser-launcher/src/driver.rs::BrowserLauncher` owns a `RunningRegistry`, monotonically allocated legacy CDP ports starting at 9222, and an `Arc<ProfileManager>`. Its `BrowserHandle` owns either a Tokio child + optional bridge, or a Chromix supervisor. `crates/browser-launcher/src/registry.rs` protects handles with an async mutex. The application keeps this launcher on one dedicated thread; an Arc does not make rusqlite's connection `Sync`.

| Engine | Default browser data directory | Launch owner | Fingerprint policy |
| --- | --- | --- | --- |
| CFT | Profile `data_dir` | `launch` + `build_spawn_args` | UA argument, then CFT CDP bootstrap |
| CloakBrowser | `data_dir/engines/cloakbrowser` | `launch` + `build_spawn_args` | Native `--fingerprint-*` flags; limited CDP bootstrap |
| Chromix | `data_dir/engines/chromix`, overridable | `launch_with_chromix` + SDK sidecar | SDK options only; no legacy fingerprint translation |

Source: `crates/browser-launcher/src/driver.rs`, `crates/browser-launcher/src/args.rs`, `crates/browser-launcher/src/chromix.rs`, `crates/cdp-driver/src/bootstrap.rs`. Keep these paths distinct; see [Chromix contract](./chromix.md).

## Legacy launch and close

`launch` rejects Chromix, reuses registered endpoint info, loads the profile, **marks opened before spawn**, chooses a port/directory, synchronizes managed UA version if detected, prepares proxy/session state, builds args, spawns, and records the handle. It returns an HTTP CDP endpoint without waiting for CDP readiness; `BrowserSession::connect` later retries. Port allocation is not a socket reservation.

`BrowserHandle::is_alive` checks the supervisor flag only for Chromix; legacy handles return true even if the child exited. Registry reuse is not proof of OS liveness, nor atomic concurrent launch protection. `close` removes the handle, closes a Chromix process or stops a legacy bridge, and uses child kill/wait operations with two-second waits. Despite an old comment, it does not implement a verified SIGTERM-then-SIGKILL graceful sequence. `close_all` iterates stored IDs and ignores individual close errors.

`crates/browser-launcher/src/session_restore.rs::ensure_session_restore` writes a minimal `Default/Preferences` via a temporary sibling + rename; it **replaces**, rather than merges, existing preferences. `clean_stale_singleton_locks` removes existing lock files without PID validation. `has_restorable_session` exists but `build_spawn_args` still appends any permitted start URL unconditionally. Do not copy these limitations into stronger recovery promises.

## Argument and fingerprint contracts

Keep string arguments as a `Vec<String>` passed to `Command::args`; don't assemble a shell command. `build_spawn_args` includes persistent user data, session restore, locale/window/DPR; CFT adds `--user-agent`/`--test-type=gpu`, CloakBrowser adds native fingerprint flags. It does not add guest/incognito. Start URL accepts http/https prefixes or exact about:blank. Legacy extension args select enabled, nonempty paths without checking directory existence (Chromix does check).

`build_cloak_fingerprint_args` hashes profile seed (or ID) into a five-digit seed, clamps the device-memory API value, includes GPU/persona fields and `--fingerprint-noise=false`. Quota is **bytes** and omitted when absent or zero. Real excerpt from `crates/browser-launcher/src/args.rs`:

```rust
if let Some(q) = fp.storage_quota {
    if q > 0 {
        // The browser flag and persisted quota both use bytes.
        args.push(format!("--fingerprint-storage-quota={q}"));
    }
}
```

Tests: `crates/browser-launcher/tests/args.rs` covers engine separation, custom UA, seed shape, memory clamp and quota round trips. `crates/browser-launcher/src/version.rs::detect_chromium_version` reads Windows executable metadata and returns None elsewhere; **do not execute the browser with `--version`**. `synchronize_managed_fingerprint_version` uses a UA/client-hints matching heuristic, not an explicit user-customized flag. See `crates/browser-launcher/tests/version.rs` and `crates/browser-launcher/tests/version_detect.rs` when adjusting it.

## Proxy and error boundaries

`crates/browser-launcher/src/socks5_bridge.rs::Socks5Bridge::start` exposes a loopback no-auth SOCKS listener for legacy engines; HTTP CONNECT supports Basic credentials when both supplied. Upstream SOCKS currently negotiates no-auth only. It has simplified IPv6 formatting, fixed SOCKS reply reading, and incomplete HTTP CONNECT header draining. `stop` stops acceptance; the unused live-socket list means it does not prove all active tunnels are closed. Existing `crates/browser-launcher/tests/socks5_bridge.rs` tests greeting/error replies, not successful end-to-end tunneling.

`crates/browser-launcher/src/proxy_geo.rs::probe_proxy_geo` contacts ipapi.co through the proxy; `parse_ipapi_response` is separately testable and returns lowercase country codes. Legacy launch treats geo/cache update failure as best effort, unlike bridge-start failure. Proxy-related flags are implementation intent, not independently verified DNS/WebRTC leak prevention.

Use `MultizenError::Launch` for contextual launch failures, existing Config errors for geo parsing/request failures, and `?` for owned IO/serde operations. `tracing::debug!` records detected versions; subscriber setup belongs to Tauri. Never log proxy credentials or treat missing tests as guarantees.
