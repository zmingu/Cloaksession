# multizen-core Backend Guidelines

Shared Rust models and error vocabulary; no database connection, browser process, or application state lives here. `crates/multizen-core/src/lib.rs` re-exports the public contract.

## Guides

- [Models, wire compatibility, and errors](./contracts.md)
- Consumers: [profile persistence](../../profile-manager/backend/index.md), [settings persistence](../../settings-store/backend/index.md), [Tauri integration](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read `crates/multizen-core/src/profile.rs`, `crates/multizen-core/src/settings.rs`, and [contracts](./contracts.md) before changing a shared field.
- [ ] Trace serde consumers in profile-manager, settings-store, MCP schemas, and `crates/tauri-app/ui/src/types.ts`; a Rust field change alone does not update them.
- [ ] Read the [shared thinking guides](../../guides/index.md); distinguish Rust patch semantics from JSON missing/null behavior.

## Quality Check

- [ ] From the repository root: `cargo test -p multizen-core --locked`. This crate currently has no dedicated test files; compilation alone is not wire-compatibility coverage.
- [ ] Run relevant consumer checks: `cargo test -p profile-manager -p settings-store --locked`. The full/partial fingerprint and arbitrary Chromix JSON round trips live there.
- [ ] Use Rust stable and the existing lockfile/dependencies; bundled rusqlite needs a working native C toolchain. The workspace CI command is `cargo test --workspace --locked` in `.github/workflows/build.yml` (Tauri adds platform prerequisites).
- [ ] Check serialized names, defaults, null handling, and numeric units, not just Rust type compatibility. Do not claim a browser check from model tests.
