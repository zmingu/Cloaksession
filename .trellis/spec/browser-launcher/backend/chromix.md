# Chromix Sidecar Contract

## Chromix is the only engine

Chromix is the sole supported engine (CFT/CloakBrowser were removed — see [Engine lifecycle](./lifecycle.md#architecture-decision-chromix-is-the-only-engine)). `BrowserLauncher::launch_with_chromix` is the only launch entry; there is no engine selector and no second launch path.

## Fingerprint ownership (host does NOT inject flags)

Identity/anti-detection is owned **entirely by the SDK bridge**. The host no longer injects `--fingerprint-*`, `--user-agent` or `--test-type` args; `crates/browser-launcher/src/args.rs::build_spawn_args` produces only the CDP endpoint contract, and `build_cloak_fingerprint_args` was deleted.

- Chromix receives its identity config through `chromixOptions` in the launch request (see below), including `geoip` when a proxy exit region should drive the locale/timezone alignment.
- The application's `multizen_core::FingerprintConfig` (`Profile.fingerprint`) is **retained only for wire/serde compatibility** (field shape, MCP `fingerprint` input, UI catalogs). It is **not consumed by the Chromix launch path** — it is not translated to SDK options, not sent in the launch request, and not used for CDP bootstrap. Do not add code that reads `Profile.fingerprint` to derive launch behaviour for Chromix.

## Host versus SDK

`crates/browser-launcher/src/chromix.rs::start` starts the configured Node executable with only the canonical `bridge.mjs` path on argv. It sends one line-delimited JSON launch request on stdin; configured environment is passed via `Command::envs`. Proxy secrets/options stay off argv. `crates/tauri-app/resources/chromix/bridge.mjs` is project-owned glue around the pinned SDK; do not document vendored SDK internals as host policy.

The request carries options, binaryPath, skipDownload, host cdpPort, default userDataDir, proxy, existing enabled extension directories, and startUrl. Legacy `Profile.fingerprint` is **not** sent. Global/profile options are shallow-combined by `crates/tauri-app/src/driver.rs::launcher_task` using `ChromixSettings::with_profile_options` before the launcher call; direct launcher callers must provide the effective config themselves.

## Options and readiness

`bridge.mjs::prepareOptions` clones options and preserves unknown SDK fields/explicit false/null values unless a known host constraint rejects them. It:

- Allows only native mode and Playwright adapter if supplied; rejects `devicePool` through this sidecar.
- Reserves debug port/address flags and profile-directory/user-data-dir arguments across top-level, launchOptions and contextOptions, including ignoreDefaultArgs arrays. A top-level `userDataDir` override is permitted; nested overrides are rejected.
- Adds loopback remote-debugging flags to every effective args layer, without injecting any host fingerprint flags (the app's `FingerprintConfig` is never translated to args) or proxy-leak flags.
- Defaults headless to false only if no layer sets it; adds profile proxy/extensions only if options/args have not explicitly overridden them. **No default `geoip` is added by the bridge** — the app supplies `geoip` through `chromixOptions` when it wants the SDK to align locale/timezone with the proxy exit region (the shop-account wizard does this when a proxy is entered).

Hidden launches bypass the legacy argument builder. `launch_with_chromix` injects the off-screen window by merging `--window-position=-32000,-32000` into the effective `launchOptions.args`: read the existing array, **append** the switch, then write the merged `launchOptions` back. Never `insert`/replace the whole `launchOptions.args` array — that silently drops a profile's own args (the regression is covered by `crates/browser-launcher/tests/chromix.rs::hidden_launch_merges_window_position_into_existing_launch_args`, which also asserts no `--headless`). Headless is never set; the SDK keeps its `headless=false` default.

`configureBinary` checks nested executablePath (contextOptions before launchOptions), request binaryPath/environment override, or installed SDK cache when skipDownload is true. Missing local/cache binary under skipDownload is an error, not permission to download. With no override and downloads allowed, SDK resolution remains in charge. The host does not bundle Node itself (`crates/tauri-app/resources/chromix/package.json` requires >=20).

Rust briefly reserves a free loopback port, drops the listener for Chromium to bind it, then requires a matching ready endpoint; this is not race-free continuous port ownership. `waitForCdp` requires a successful `/json/version` response and parses `webSocketDebuggerUrl`, checking hostname 127.0.0.1 and the selected port before ready. It does not check the URL scheme or perform a WebSocket handshake; actual attachment happens in cdp-driver. Startup has a 15-minute Rust timeout including SDK downloads; the bridge's CDP readiness timeout is 60 seconds. The bridge opens the start URL only when no restored/nonblank page or explicit URL args already take precedence.

## Windows font compatibility

After `prepareOptions`, `windows-fonts.mjs::applyWindowsFonts` reads the pinned vendor font-name parser (vendor is unchanged) and adds an ASCII-family `--uxr-font-whitelist` to each effective args layer on Windows. It uses explicit `fontsDir` or `%SystemRoot%/Fonts` (`WINDIR` fallback), without writing settings/Profile data or translating legacy `Profile.fingerprint.fonts_dir`. Explicit font policy/whitelist flags, null/empty fontsDir, disabled fingerprinting, and non-Windows platforms opt out. Empty/over-limit results fail with configuration guidance rather than silently emitting an invalid whitelist.

Observed on Windows Chromix 151.0.7922.173: default CJK text and a full fontsDir-generated whitelist containing Chinese family aliases select Times New Roman (missing CJK glyphs); the same installed families with only ASCII names select Microsoft YaHei. Do not solve this by turning off fingerprinting or hardcoding `C:\\Windows\\Fonts` in the legacy fingerprint. `test/windows-fonts.test.mjs` exercises an isolated OpenType name-table fixture and option precedence; opt-in `CHROMIX_TEST_BINARY=<installed exe> node test/windows-fonts-smoke.mjs` compares default/explicit fontsDir before and after the host fix using local DOM/CDP font data. It does not visit a platform or use user profile data. This is measured font rendering evidence, not an anti-fingerprinting guarantee.

## Lifetime and control channel

`launch_with_chromix` serializes launches through a launcher-wide mutex, reuses live entries, removes stale state, starts the sidecar, and marks opened **after ready**. If marking opened fails, it closes the process before returning error. PID is the Node process PID, not necessarily the browser PID.

Stdout is reserved for ready/error/closed JSON; SDK output is redirected to stderr. `ChromixProcess` sends close on explicit close or drop; its supervisor tracks liveness, closes on startup cancellation/failure and bounds child shutdown (platform-specific termination fallback). Tauri additionally closes Chromix if CDP attachment fails. Do not confuse the launcher's live supervisor flag with Tauri's separate, potentially stale running cache.

## Focused evidence

- `crates/browser-launcher/tests/chromix.rs::fixture_import_handles_canonical_and_url_sensitive_paths`: real Node imports a temporary module through the same `bridge_import` helper as the fake-SDK fixture. Keep paths JSON-serialized and use Node `pathToFileURL` + dynamic import, never `format!("file://{}", canonical_path.display())`. Windows canonical paths carry the extended-length prefix (`\\?\`); Chinese/space/#/% filenames require URL encoding. This tests normal and canonical paths, not a browser or remote UNC share.
- `crates/browser-launcher/tests/chromix.rs::persistent_launch_preserves_options_and_keeps_secrets_off_argv`: fake SDK, opaque options and no credentials on argv.
- The same file covers delayed-ready registration, SDK failure, cancellation cleanup, external-exit liveness, override directories, and actionable missing-runtime errors.
- `crates/tauri-app/resources/chromix/test/bridge.test.mjs` tests host option validation and bridge control/lifecycle; `crates/tauri-app/resources/chromix/test/upstream.test.mjs` checks SDK/runtime compatibility without launching a real browser.
- Runtime test command is `npm test` in `crates/tauri-app/resources/chromix`; CI prepares dependencies with `npm ci --omit=dev` and uses Node 22 (`.github/workflows/build.yml`). Browser-launcher Rust tests need Node even when browser integration is skipped. The separate `test/native-smoke.mjs` is an explicit real-browser smoke path, not part of the `*.test.mjs` command.
