# settings-store Backend Guidelines

Owns cached JSON application settings through `crates/settings-store/src/defaults.rs::SettingsStore`. It does not own SQLite profiles or live application reconfiguration.

## Guides

- [File persistence and normalization](./persistence.md)
- [Shared settings models](../../multizen-core/backend/contracts.md) and [Tauri settings integration](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read [persistence guidance](./persistence.md), `RawSettings`, `AppSettings::default`, and both load/update paths.
- [ ] Trace every new setting to its consumer; persisted state and running state are different contracts.
- [ ] Check `crates/settings-store/tests/store.rs`, `crates/settings-store/tests/chromix.rs`, and the [shared thinking guides](../../guides/index.md).

## Quality Check

- [ ] Run `cargo test -p settings-store --locked` from the repository root (Rust stable, existing dependencies/native toolchain; no browser or Node process needed).
- [ ] Verify missing/corrupt file, partial/invalid raw JSON, update/cache/reopen, and opaque Chromix options. Current tests use TempDir, not a shared user settings file.
- [ ] Check defaults on load separately from defaults on construction, and blank-path normalization separately from update behavior.
- [ ] A successful save is not proof of MCP restart or changed live browser settings. Verify `crates/tauri-app/src/commands/settings.rs` and `crates/tauri-app/src/lib.rs` for integration changes.
