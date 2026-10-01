# profile-manager Backend Guidelines

Owns SQLite profile metadata and profile-root directory lifecycle, not browser liveness or application settings. Entry point: `crates/profile-manager/src/manager.rs::ProfileManager`.

## Guides

- [Persistence, migrations, and CRUD contracts](./database-guidelines.md)
- [Business account metadata and persistent scopes](./business-accounts.md)
- [Independent Kuaishou identity archives and observations](./kuaishou-identity.md)
- Shared [model contract](../../multizen-core/backend/contracts.md); runtime owners: [launcher](../../browser-launcher/backend/index.md) and [Tauri](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read [persistence guidance](./database-guidelines.md), the affected manager method, row mapper, and migrations together.
- [ ] Check create/get/list/update/import paths for every field change, including old database and old JSON compatibility.
- [ ] Read `crates/profile-manager/tests/manager.rs` and `crates/profile-manager/tests/migrate.rs` before changing error or null behavior.
- [ ] Consult [shared thinking guides](../../guides/index.md); keep browser engine subdirectories and running state outside this crate.

## Quality Check

- [ ] Run `cargo test -p profile-manager --locked` from the repository root with Rust stable, existing dependencies, and a native C toolchain for bundled SQLite (`crates/profile-manager/Cargo.toml`). No browser is required.
- [ ] For schema changes, prove new DB + old DB migration + repeated migration + reopen/round trip, using TempDir or in-memory SQLite as existing tests do.
- [ ] Check SQL column order against `ProfileRow`, and distinguish `get` corruption panics from `list` errors; do not claim all malformed rows recover gracefully.
- [ ] For shared fields also run relevant settings/MCP consumers. `.github/workflows/build.yml` runs the full locked workspace suite; it is not a substitute for explicit migration assertions.
