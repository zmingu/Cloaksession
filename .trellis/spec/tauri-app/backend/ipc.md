# IPC, Events, and Verification

## Command contract

Keep `#[tauri::command]` functions under `crates/tauri-app/src/commands/`, import/register them in `crates/tauri-app/src/lib.rs::run`, and match wrappers in `crates/tauri-app/ui/src/lib/ipc.ts`. Invoke names are snake_case function names; JS argument object keys use camelCase (`profileId`, `urlOrId`), while struct payload serialization follows serde. Do not resurrect Electron colon-style command names; colon strings are still used for events.

Thin CRUD example from `crates/tauri-app/src/commands/profiles.rs`:

```rust
#[tauri::command]
pub async fn profiles_update(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateProfileInput,
) -> Result<Profile, String> {
    state
        .driver
        .update_profile(&id, patch)
        .await
        .map_err(|e| e.to_string())
}
```

`profiles_get/create/close` and extension list/set use the same adapter style: preserve typed Result internally, stringify at IPC. `profiles_list` overlays cached running state. **IPC delete does not close first**; it forwards directly to pm.delete. Both MCP delete paths do check/close first. `profiles_launch` spawns the companion poller after success; MCP launch bypasses that command and does not start a poller.

`crates/tauri-app/src/commands/settings.rs::settings_update` accepts arbitrary JSON, shallow-merges non-null top-level fields into serialized current settings, then deserializes/writes. A nested Chromix object is replaced, not deep-merged; null cannot clear a value and non-object patch is effectively a no-op. See [settings persistence](../../settings-store/backend/persistence.md) for normalization and startup-only effects. `activity_recent` defaults to 100, capped at 500.

Shared model names/units belong to [multizen-core](../../multizen-core/backend/contracts.md). TS types alone don't validate payloads: CreateProfileInput's custom deserializer accepts full or partial data under the same `fingerprint` key (there is no separate fullFingerprint wire field), and nullable update fields don't establish a wire-level clear contract.

`crates/tauri-app/src/commands/fingerprint.rs` actually returns rich device/locale catalogs and implements reconcile; stale comments in ipc.ts saying strings/stubs are not evidence. Reconcile maintains dependent locale/languages/Accept-Language/country fields; an MCP partial patch does not invoke it. Device changes do not regenerate UA/platform/client hints/WebGL, and screen changes leave availScreen unchanged. `fingerprint_generate` delegates to the fixed default persona even for an empty seed; it does not randomize as the old IPC comment claims. Keep catalog copies in MCP and core family serialization consistent.

## Explicit shop-login entry

`profiles_launch({id, entry?: "kuaishou-shop"})` remains a generic launch when entry is omitted/null (including existing UI and MCP callers). The `profiles.launchKuaishou` UI wrapper is exposed by the profile's “快手小店扫码” button. Only that explicit action checks persisted business scope before launch and again after it, rejects Jinniu even after unbinding, and creates/activates a new `https://login.kwaixiaodian.com/` tab. It never updates `startUrl`, closes/replaces restored tabs, clears cookies, scans a QR code, or submits platform actions. The existing launcher directory-isolation gate still applies. A page-open/activation failure leaves the browser open and returns actionable text. Tests: `driver/shop_login_tests.rs`, UI `tests/shop-login.spec.ts`; UI is mocked and is not native end-to-end/platform login evidence.

## Business account commands

`commands/business_accounts.rs` registers `business_accounts_list()` -> BusinessAccount[], `business_accounts_profile_state(profileId)` -> BusinessProfileState, `business_accounts_save({input})` -> BusinessAccount, and `business_accounts_unbind({id})` -> void. All forward to driver async helpers / LauncherCmd on the original launcher thread; no SQLite or platform I/O at the IPC adapter. Errors become strings for UI presentation via `business_error`, retaining the underlying cause with actionable Chinese refresh/input/profile-state/restart guidance. Records are manual metadata, never proof of login. See [models](../../multizen-core/backend/contracts.md), [persistence](../../profile-manager/backend/business-accounts.md) and [directory gate](../../browser-launcher/backend/business-isolation.md).

Business list includes orphaned records. Save/edit/unbind inspect launcher is_running_async, not the cached UI running set. Scope survives unbind. Profile options update uses a validated candidate; conflicts leave old config untouched. IPC delete rejects a running reserved Profile; ordinary unreserved deletion retains its old behavior. No business record delete is exposed.

## Current event compatibility (not guarantees)

Inspect emitters and listeners together. `listen` returns a Promise of UnlistenFn; callers must await registration/cleanup. Rust `rename_all` on an enum renames variants, not fields inside a struct variant.

| Event | Actual producer and payload | Consumer boundary |
| --- | --- | --- |
| profiles:running-changed | `driver.rs::RunningStateChange`: kind launched/closed with **profileId**; closed also reason user-close | Enum uses rename_all_fields=camelCase; exact JSON tests cover launched/closing/closed and reject profile_id. Closing/external-exit are not emitted by current driver |
| chromium:status | `driver.rs::ChromiumStatus`: profileId, status started/stopped/failed, optional error | onChromiumStatus is a no-op; chromium.status returns a ready stub, retry rejects |
| activity:event | `mcp-server/src/activity.rs::ActivityEvent`, camelCase | Direct listener; pending then completion updates, not a durable stream |
| profiles:proxy-country-updated | `lib.rs`: id, country | Direct listener matches |
| extensions:installed | `commands/companion.rs::ExtensionInstalledPayload`: ok, **profile_id**, extension or error | TS expects profileId: mismatch |
| update:status | `commands/update.rs::StatusEvent`: `{status: UpdateStatus}` | Listener unwraps status; Available's **release_notes** differs from TS releaseNotes; Rust **noUpdate/upToDate** variants differ from TS no-update/up-to-date |

Source locations above are within `crates/tauri-app/src/` except the explicitly named activity crate. Do not fix these mismatches by unchecked casts or claim compile success proves serialization compatibility. They are known documentation findings, not product fixes in this bootstrap.

## Verification commands and prerequisites

Run from repository root unless a cwd is stated. These are future development checks, not results of the docs bootstrap.

| Scope | Command | Prerequisites / what it proves |
| --- | --- | --- |
| Rust integration shell | `cargo test -p tauri-app --locked` | Rust stable, native Tauri toolchain/system libraries. Token helper tests plus `crates/tauri-app/tests/registry_smoke.rs`; no real browser. Dead-endpoint check intentionally retries loopback and can take seconds |
| Workspace | `cargo test --workspace --locked` | Same native prerequisites; current CI Rust gate, not desktop/browser E2E |
| UI type/build | `npm run build` in `crates/tauri-app/ui` | Installed UI deps, Node 22 as in CI; tsc -b + Vite. No lint script exists |
| UI fingerprint catalog | `node --experimental-strip-types src/lib/chromixFingerprint.test.mjs` in UI | Node 22; catalog contract regression, no browser |
| Chromix bridge | `npm test` in `crates/tauri-app/resources/chromix` | Installed bridge dependencies, Node >=20 (CI 22); see [bridge checks](../../browser-launcher/backend/chromix.md), not a live launch |
| UI browser | `npx playwright test --config playwright.config.ts` in UI (also npm test) | Installed Playwright/Chrome; starts Vite at 127.0.0.1:5174, tests desktop/mobile Chrome projects. UI tests mock Tauri IPC; not native shell E2E |
| Tauri compile | `npx tauri build --no-bundle --config ../tauri.conf.json` in UI | Installed Tauri CLI/native toolchain. beforeBuildCommand runs npm ci for Chromix + UI build; can install/download dependencies, so do not run casually in a no-install task |

`.github/workflows/build.yml` is authoritative: Linux test job installs WebKitGTK 4.1, GTK3, appindicator, secret/SSL/rsvg libraries, patchelf/libfuse2 and Node 22; tauri-build matrix compiles on Linux/macOS/Windows. It does not prove real browser/CDP, native dialogs, archives or updater installation work across all platforms. Browser tests and installers must be opt-in with disposable data, never implicit validation for a documentation change.
