# File Persistence and Normalization

## One file, one in-process cache

`crates/settings-store/src/defaults.rs` contains the store, private `RawSettings` deserializer, and `default_settings_path(dir)` (`settings.json`). `SettingsStore::new` best-effort creates the parent directory and starts with an empty cache. There is no database, watcher, file lock, multi-process merge, or logging subscriber.

`load(&mut self)` returns a cloned cached `AppSettings` once loaded. On first load, read or parse failure becomes an entirely default `RawSettings` (not per-field recovery). Present optional fields overlay defaults. External file edits are invisible until a new store is constructed.

`update(&mut self, patch: AppSettings)` is a **whole replacement**, not a JSON merge patch. Real implementation:

```rust
let json = serde_json::to_string_pretty(&patch)?;
std::fs::write(&self.json_path, json)?;
self.cache = Some(patch.clone());
Ok(patch)
```

Source: `crates/settings-store/src/defaults.rs::SettingsStore::update`. Serialization/write errors propagate through `multizen_core::Result`; the cache changes only after a successful write. This is a direct write, not temp-file rename/atomic durability; a failed write can still affect the disk file. Do not silently adopt this as an atomic-save guarantee.

## Load-time compatibility

- **Chromix is the only engine.** The raw loader reads `browserEngine` as `Option<String>` and **always** yields `BrowserEngine::Chromix`: any present string (historical `"cloakbrowser"`/`"cft"`, an unknown value) normalizes to Chromix, and a missing field falls back to `BrowserEngine::default()` (also Chromix). This is tolerant migration so old `settings.json` files keep loading; there is no live support for those engines. Evidence: `crates/settings-store/tests/store.rs::load_normalizes_invalid_browser_engine` and `load_normalizes_legacy_engine_values_to_chromix`.
- A blank/whitespace-only binary path becomes `None`. Nonblank paths are kept as supplied, not trimmed or validated as executables.
- Missing Chromix configuration becomes `ChromixSettings::default()`; its own serde defaults supply `node`, empty options, and empty string-valued environment map. Opaque option values survive intact.
- **Default mismatch:** `AppSettings::default().auto_update` is false, but `load` uses `raw.auto_update.unwrap_or(true)`. Missing/corrupt files therefore load auto-update true. Do not describe these two defaults as identical.
- `update` does not run these normalizers. For example, a whitespace binary path can remain in the cache until reopen; typed enum deserialization is stricter than the raw string engine loader.

Evidence: `crates/settings-store/tests/store.rs::load_recovers_from_corrupt_json`, `load_normalizes_invalid_browser_engine`, `load_clears_empty_browser_binary_path`, and `update_persists_and_caches`; `crates/settings-store/tests/chromix.rs::old_settings_get_empty_chromix_configuration` and `chromix_options_survive_reload_without_losing_sdk_fields`.

## Consumer boundary

The canonical model is `crates/multizen-core/src/settings.rs`. Profile overrides use `ChromixSettings::with_profile_options` (shallow map replacement, not recursive merge); see [model guidance](../../multizen-core/backend/contracts.md).

`crates/tauri-app/src/commands/settings.rs::settings_update` merges all supplied non-null top-level JSON fields into serialized loaded settings, then deserializes to AppSettings (unknown fields are ignored), then calls this store's whole-value update. `crates/tauri-app/src/lib.rs::build_app_state` constructs the browser driver from settings; MCP starts in setup, while auto-update is read by a delayed startup task. Saving settings does not rebuild the driver or restart MCP. UI types/IPC are consumers (`crates/tauri-app/ui/src/types.ts`, `crates/tauri-app/ui/src/lib/ipc.ts`), not another persistence owner.

For new fields, update raw loader, default/type, command merge, UI mirror, and affected consumer intentionally. Keep malformed-file fallback and surfaced write failures distinguishable in tests; avoid tests that only read back the existing cache.
