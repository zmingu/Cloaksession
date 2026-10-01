# Models, Wire Compatibility, and Errors

## Ownership and naming

`crates/multizen-core/src/profile.rs` owns `Profile`, `ProfileSummary`, fingerprint/proxy/extension types, create/update inputs, and `LaunchedProfile`. `crates/multizen-core/src/settings.rs` owns `AppSettings`, `BrowserEngine`, and `ChromixSettings`. Keep persistence and runtime policy in their consumer crates.

- Public serde structs use camelCase; `ProxyConfig.proxy_type` is explicitly `type`. `DeviceFamily` uses explicit kebab-case spellings; `BrowserEngine` serializes as `cft`, `cloakbrowser`, or `chromix`.
- `ProfileId` is a `String`, not a validated UUID wrapper. `ProfileManager::create` supplies a UUID; imported IDs require validation at the importing boundary.
- `FingerprintConfig.storage_quota` is bytes end-to-end (not GB); screen dimensions are integer sizes, `dpr` is `f64`, `device_memory` is the GB-like persona value later clamped for CloakBrowser's API. See `crates/browser-launcher/src/args.rs::build_cloak_fingerprint_args` and `crates/browser-launcher/tests/args.rs::cloak_storage_quota_preserves_custom_values_in_bytes`.
- Model timestamps are strings; `crates/profile-manager/src/manager.rs` and `crates/browser-launcher/src/driver.rs` produce UTC RFC3339. Do not substitute Unix numbers on the wire.
- UI types in `crates/tauri-app/ui/src/types.ts` are manually maintained, not generated. For example, Rust `ProfileSummary` includes `chromixOptions` while the current TS summary does not. Check actual command/event adapters rather than trusting that file's historical comments.

## Create versus update

`CreateProfileInput` has custom `Deserialize` (no derived `Serialize`). Its single JSON `fingerprint` field first attempts a complete `FingerprintConfig`, then falls back to `PartialFingerprintInput` (userAgent, locale, timezone, country only). Complete UI input lands in `full_fingerprint`; legacy MCP input lands in `fingerprint`. An incomplete full-looking object can fall back to the partial shape and lose unmodeled fields: this is not strict full-fingerprint validation.

`UpdateProfileInput.fingerprint` whole-replaces a complete config. MCP partial updates first load the current config, patch its supported fields, then pass the full object; see `crates/mcp-server/src/tools.rs::update_profile` and `crates/tauri-app/src/mcp_embed.rs::TauriMcpDispatcher::dispatch`.

The Rust `Option<Option<T>>` fields (`icon`, `start_url`, `search_provider`, `proxy`) support keep / clear / set when constructed directly. **Do not promise the same three states over JSON**: ordinary derived serde treats missing and null as outer `None`; no double-option deserializer is installed here. The manager's clear test constructs `Some(None)` in Rust. `notes: Option<String>` likewise cannot clear to null through the current patch.

## Opaque Chromix options

`Profile.chromix_options` and `ProfileSummary.chromix_options` are JSON maps with a missing-field default of `{}`. Profile JSON rejects explicit null/non-objects. Optional create/update maps accept missing or null as `None`; update `{}` clears the map, a nonempty object replaces it rather than deep-merging. Preserve unknown keys, arrays, false, null values inside the object, and string seeds; do not coerce large seed strings through floating point.

Real implementation from `crates/multizen-core/src/settings.rs::ChromixSettings::with_profile_options`:

```rust
let mut config = self.clone();
config.options.extend(options.clone());
config
```

This is a **shallow top-level override** of global options by profile options. Arrays and nested objects replace; environment and node path remain global. Evidence: `crates/settings-store/tests/chromix.rs::profile_options_override_global_values_without_merging_arrays_or_objects` and `crates/profile-manager/tests/manager.rs::chromix_options_update_replaces_preserves_omitted_and_clears_with_empty_object`.

## Business account metadata (independent wire contract)

`business.rs` owns BusinessAccountKind (kuaishou-shop, kuaishou-live, kuaishou-mate, kuaishou-sub, jinniu), BusinessProfileScope (jinniu/kuaishou), BusinessAccount, BusinessProfileState and SaveBusinessAccountInput. All struct fields are camelCase; account/state Options serialize explicit null (not missing). Input id defaults to None for omitted/null. Timestamps remain RFC3339 strings. Neither Profile nor ProfileSummary gains fields, and records have no credentials/login status. Persistence validates limits/uniqueness/scope; launcher validates runtime and directories. See [storage contract](../../profile-manager/backend/business-accounts.md).

## Kuaishou identity observations

`kuaishou_identity.rs` defines KuaishouIdentityStatus (kebab-case seven states) and KuaishouIdentitySnapshot (exact camelCase profileId/status/platformUserId/nickname/avatarKey/checkedAt/lastSeenAt/message, nullable fields explicit null). `detected` is read-only exact-origin DOM evidence, not authentication/init/OCR verification. `valid_kuaishou_user_id` requires the entire ID to be 5..32 ASCII digits. Internal KuaishouIdentityObservation carries session_id and avatar_url for persistence/reuse only; never expose that envelope to UI. Manual business accounts remain independent; see [identity persistence](../../profile-manager/backend/kuaishou-identity.md) and [runtime](../../tauri-app/backend/kuaishou-identity.md).

## Defaults and errors

- `AppSettings::default`: dark theme, MCP enabled on 7777, Cloakbrowser, no binary override, download allowed, auto-update false, usage reporting false. `ChromixSettings::default`: node executable `node`, empty options and environment. Only selected new fields have serde defaults; `AppSettings` itself is not a blanket missing-field merge.
- File-loading defaults differ: `SettingsStore::load` sets missing `autoUpdate` to **true**. See [settings guidance](../../settings-store/backend/persistence.md); do not infer persisted defaults solely from `AppSettings::default`.
- `crates/multizen-core/src/error.rs::MultizenError` wraps rusqlite, IO, and serde errors with `#[from]`, plus NotFound, AlreadyExists, Config, Launch, Cdp, and Mcp strings. Use its `Result<T>` alias internally; conversion to IPC strings or MCP envelopes belongs to adapters. `McpToolError` existing as a type does not mean HTTP uses it.
- This crate installs no logging subscriber or persistence layer. Do not add a parallel model or error hierarchy to solve a consumer-only concern.

Representative regression evidence: `crates/profile-manager/tests/manager.rs::create_full_fingerprint_json_round_trips_without_loss`, `create_legacy_partial_fingerprint_remains_compatible`, and `old_profile_json_import_defaults_chromix_options_to_empty`; `crates/settings-store/tests/chromix.rs::chromix_configuration_requires_objects_and_string_environment_values`.
