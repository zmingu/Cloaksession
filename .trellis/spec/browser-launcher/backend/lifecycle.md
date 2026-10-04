# Engine Lifecycle (Chromix-only)

## Architecture decision: Chromix is the only engine

**This project supports exactly one browser engine: Chromix.** The former `Cft` and `Cloakbrowser` engines were deleted from the codebase; `multizen_core::BrowserEngine` is now a **single-variant** enum (`Chromix`, `#[default]`). Do not reintroduce an engine selector, a second launch path, or engine-conditional branches.

Why (do not undo without re-deciding):
- CloakBrowser's free tier allowed **1 concurrent** instance, which blocked the multi-profile/business use case.
- The patched CloakBrowser build tripped a **CDP `DCHECK` crash** when `Runtime`/`Network` were enabled (this is what the old `CLOAK_RISKY_ENABLE_DOMAINS` policy guarded).
- Chromix is the **open edition of the same engine**, so it keeps the anti-detect behaviour without those two constraints.

The enum/field shape (`browser_engine` in `settings.json`, `AppSettings.browser_engine`, function signatures) is **kept for compatibility** even though it has one value. Legacy serialized strings `"cft"`/`"cloakbrowser"` and unknown strings are tolerated on load and normalized to `Chromix` (see [settings persistence](../../settings-store/backend/persistence.md)); this is tolerant-read compatibility, not live support for those engines.

## Ownership and data flow

`crates/browser-launcher/src/driver.rs::BrowserLauncher` owns a `RunningRegistry` and an `Arc<ProfileManager>`. Its `BrowserHandle` owns a Chromix supervisor (`chromix: Option<ChromixProcess>`); there is no longer a legacy Tokio-child/bridge variant. `crates/browser-launcher/src/registry.rs` protects handles with an async mutex. The application keeps this launcher on one dedicated thread; an Arc does not make rusqlite's connection `Sync`.

| Engine | Default browser data directory | Launch owner | Fingerprint policy |
| --- | --- | --- | --- |
| Chromix | `data_dir/engines/chromix`, overridable by top-level `userDataDir` | `launch_with_chromix` + SDK sidecar | SDK `chromixOptions` only; the app's `FingerprintConfig` is **not** translated or consumed |

The **only** public launch entry is `BrowserLauncher::launch_with_chromix(profile_id, binary_path, companion_dir, config, runtime_dir, skip_download, hidden)`; the old `launch(...)`/`build_spawn_args`-driven native path is **removed** (the `build_spawn_args` signature survives as a stub, see below). Source: `crates/browser-launcher/src/driver.rs`, `crates/browser-launcher/src/chromix.rs`, `crates/browser-launcher/src/data_dir.rs`. See [Chromix contract](./chromix.md).

## Launch and close

`launch_with_chromix` reuses a live registry entry, closes any stale entry for the profile, loads the profile, computes the effective data dir, starts the Node SDK sidecar, and **marks opened after ready** (if `mark_opened` fails it closes the process before returning the error). It records the `BrowserHandle`. `data_dir.rs::effective_data_dir` is called with `BrowserEngine::Chromix` (with a `debug_assert!` on that invariant) and respects a top-level nonempty `userDataDir` override.

`BrowserHandle::is_alive` checks the Chromix supervisor liveness flag. Registry reuse is not proof of OS liveness, nor atomic concurrent launch protection. `close` removes the handle and closes the Chromix process (the supervisor bounds child shutdown with a platform-specific termination fallback); despite an old comment it is **not** a verified SIGTERM-then-SIGKILL graceful sequence. `close_all` iterates stored IDs and ignores individual close errors.

## Native spawn args (Chromix stub) and leftover legacy helpers

`crates/browser-launcher/src/args.rs::build_spawn_args` is **kept only for signature/back-compat**. Since Chromix is the only engine, the native spawn args are the CDP endpoint contract alone — it now returns exactly:

```rust
vec![
    format!("--user-data-dir={browser_data_dir}"),
    "--remote-debugging-address=127.0.0.1".into(),
    format!("--remote-debugging-port={port}"),
]
```

All other parameters (`_profile`, `_engine`, `_proxy_bridge_url`, `_geo_coords`, `_companion_dir`, `_hidden`) are **ignored**. In particular there is **no** `--fingerprint-*` injection, no CFT `--user-agent`/`--test-type=gpu`, and no proxy/DNS-leak flags. `build_cloak_fingerprint_args` and the CFT-specific arg assembly were **deleted**. Regression lock: `crates/browser-launcher/tests/args.rs::chromix_args_are_only_the_cdp_endpoint_contract` and `chromix_args_ignore_hidden_and_fingerprint_inputs` (asserting no `--fingerprint*`, no `--user-agent`, no `--test-type=gpu`, no `--proxy-server`, and that hidden/fingerprint inputs do not change the vector).

Now-unused legacy helpers remain in the crate but are **not** on the Chromix launch path — do not document them as live behaviour:
- `crates/browser-launcher/src/session_restore.rs` (`ensure_session_restore` / `has_restorable_session` / `clean_stale_singleton_locks`) has no production caller after the legacy launch path was removed. It wrote a minimal `Default/Preferences` by temp-sibling + rename (**replaces**, not merges) and removed lock files without PID validation.
- `crates/browser-launcher/src/version.rs::detect_chromium_version` (reads Windows executable metadata, returns None elsewhere; **never** execute the browser with `--version`) and `synchronize_managed_fingerprint_version` (UA/client-hints heuristic) are likewise not invoked by the Chromix path — the SDK owns identity/versioning. `crates/browser-launcher/tests/version.rs` / `version_detect.rs` still exercise them.

## Hidden (off-screen) launch

`BrowserLauncher::launch_with_chromix(..., hidden: bool)` takes a trailing `hidden` flag; it is the supported way to capture a page (e.g. a login QR) without a browser window appearing on screen.

- Chromix does **not** go through `build_spawn_args`. `launch_with_chromix` injects the off-screen window by merging `--window-position=-32000,-32000` into the effective `launchOptions.args` (read the existing array, **append**, write back — see [Chromix contract](./chromix.md)). It never sets `headless`; the SDK keeps its `headless=false` default, so the session is identical to a normal launch.
- `build_spawn_args`'s own `hidden` parameter is now ignored (the flag is handled in the SDK options layer instead).
- Unverified ceiling: whether `--window-position=-32000,-32000` is truly invisible on a real desktop and still renders the page is a native/manual acceptance item, not proven by argument tests.

Tests: `crates/browser-launcher/tests/chromix.rs::hidden_launch_merges_window_position_into_existing_launch_args` (also asserts no `--headless`); `crates/browser-launcher/tests/args.rs::chromix_args_ignore_hidden_and_fingerprint_inputs`.

## Proxy and error boundaries

`crates/browser-launcher/src/socks5_bridge.rs::Socks5Bridge` is a **leftover legacy helper** (no production caller after the legacy engine removal; only its own tests exercise it). It exposed a loopback no-auth SOCKS listener; HTTP CONNECT supports Basic credentials when both supplied. Chromix handles its own proxy inside the SDK bridge (`chromixOptions`/`proxy` object) — see [Chromix contract](./chromix.md). Do not describe `Socks5Bridge` as part of the live proxy path.

`crates/browser-launcher/src/proxy_geo.rs::probe_proxy_geo` contacts ipapi.co through the proxy; `parse_ipapi_response` is separately testable and returns lowercase country codes. It **is** still live: `crates/tauri-app/src/lib.rs` probes missing proxy countries at startup (6000 ms) and `crates/tauri-app/src/commands/proxy.rs` uses it on demand. Geo/cache update failure is best effort. Proxy-related flags are implementation intent, not independently verified DNS/WebRTC leak prevention.

Use `MultizenError::Launch` for contextual launch failures, existing Config errors for geo parsing/request failures, and `?` for owned IO/serde operations. `tracing::debug!` records detected versions; subscriber setup belongs to Tauri. Never log proxy credentials or treat missing tests as guarantees.
