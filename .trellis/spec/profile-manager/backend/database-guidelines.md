# Persistence, Migrations, and CRUD Contracts

## Source map and ownership

- `crates/profile-manager/src/manager.rs`: `ProfileManager`, parameterized SQL, profile directories and CRUD.
- `crates/profile-manager/src/migrate.rs`: idempotent table/index creation and additive columns.
- `crates/profile-manager/src/row.rs`: `ProfileRow`, JSON decoding and legacy extension normalization.
- `crates/profile-manager/src/fingerprint.rs`: fixed default persona, seeded from the supplied string. The richer UI fingerprint catalog belongs to Tauri, not this function.

`ProfileManager::new` creates parent/root directories, opens one rusqlite `Connection`, enables WAL and foreign keys, then runs migrations. The connection is not `Sync`; the application accesses it on a dedicated launcher thread (`crates/tauri-app/src/driver.rs::launcher_thread_main`). Do not treat `Arc<ProfileManager>` as sufficient thread safety.

## Schema and serialization

`run_migrations` creates `profiles` and the non-unique name index `idx_profiles_name`. `add_column_if_missing` checks `PRAGMA table_info(profiles)` before each ALTER. There is no ORM or migration-version table. Keep schema identifiers internal constants; parameterize data via `params!` as `get`, `insert_row`, and `update` do.

JSON text columns include tags, proxy, fingerprint, extensions, and chromix_options. The last is `TEXT NOT NULL DEFAULT '{}'` with a JSON-valid/object CHECK when created by this migration. Existing columns are not retroactively rebuilt or validated by the column-presence check.

Real excerpt from `crates/profile-manager/src/migrate.rs::run_migrations`:

```rust
add_column_if_missing(
    conn,
    "chromix_options",
    "TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(chromix_options) AND json_type(chromix_options) = 'object')",
)?;
```

When adding metadata, update migration, insert/update SQL, both get/list projections, `ProfileRow`/`row_to_profile`, shared models, and consumers together. `crates/profile-manager/tests/migrate.rs` covers idempotency, original-schema upgrade, and invalid JSON column writes; `crates/profile-manager/tests/manager.rs` covers old rows, arbitrary nested JSON, reopen, replace, and import.

## CRUD behavior to preserve

- `create`: generate UUID + UTC RFC3339 timestamps; create `<profiles_root>/<id>`; use complete fingerprint if supplied, otherwise `default_fingerprint(&id)`, then apply legacy partial patch. Directory creation and DB insertion are not one atomic transaction.
- `list`: descending `updated_at`; initializes `is_running` to false. Tauri/MCP overlay live state. `mark_opened` updates only `last_opened_at`; it does not reorder by `updated_at`.
- `get`: absent ID is `Ok(None)`; update turns absence into `NotFound`. Delete of an absent ID succeeds. Do not impose unique profile names: only IDs are unique.
- `update`: whole-replace fingerprint and Chromix map; absent patch fields preserve data. Changing proxy credentials/host/etc. (JSON comparison) clears cached proxy country. Rust nested options allow clearing selected fields, but JSON does not currently preserve that distinction; see [shared contract](../../multizen-core/backend/contracts.md).
- `delete`: deletes the DB row then best-effort removes the stored `data_dir`; it does not close browsers. Browser cleanup belongs to callers: `crates/mcp-server/src/tools.rs::delete_profile` and the embedded MCP dispatcher check/close first, but `crates/tauri-app/src/commands/profiles.rs::profiles_delete` currently skips that step. Filesystem failure may leave an orphan.
- `insert_imported`: rejects existing ID, creates the supplied directory, then inserts. It trusts the caller's ID/path; archive validation/rewriting belongs to `crates/tauri-app/src/commands/archive.rs`. `insert_row` does not persist imported `last_opened_at` or `proxy_country`, despite accepting a `Profile` containing them.
- Engine directories are chosen in `crates/browser-launcher/src/driver.rs`: CFT uses the profile root, CloakBrowser uses `engines/cloakbrowser`, Chromix defaults to `engines/chromix` (SDK options can override it). The manager should not duplicate that policy.

## Errors, defaults, and limitations

Use `multizen_core::Result` and `?` for DB/IO/serde failures. No crate-local logging framework is installed; contextual adapter errors are added higher up.

`row_to_profile` currently **panics** on corrupt fingerprint, proxy, or Chromix JSON; tags default empty, and `normalize_extensions` tolerates malformed/missing extension data with defaults. In contrast, `list` propagates serde errors for fingerprint/proxy/Chromix. These are observed differences, not a recommendation to add more `expect` calls or a corruption-recovery guarantee.

`default_fingerprint` supplies a Windows/Chrome 148 US persona with seed copied verbatim and 2,000,000,000-byte quota; it does not randomize the device from the seed. See `crates/profile-manager/tests/fingerprint.rs` and the launcher version-sync/flag logic before changing defaults.
