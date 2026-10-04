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
- Engine directories are chosen in `crates/browser-launcher/src/data_dir.rs::default_data_dir`. **Chromix is the only engine**: it defaults to `<profile.data_dir>/engines/chromix`, and a top-level nonempty `userDataDir` in `chromixOptions` can override it (the manager must not duplicate that policy).

## Errors, defaults, and limitations

Use `multizen_core::Result` and `?` for DB/IO/serde failures. No crate-local logging framework is installed; contextual adapter errors are added higher up.

`row_to_profile` currently **panics** on corrupt fingerprint, proxy, or Chromix JSON; tags default empty, and `normalize_extensions` tolerates malformed/missing extension data with defaults. In contrast, `list` propagates serde errors for fingerprint/proxy/Chromix. These are observed differences, not a recommendation to add more `expect` calls or a corruption-recovery guarantee.

`default_fingerprint` picks one **internally coherent** hardware persona from a device catalog (platform + UA + client hints + GPU + screen + cores + memory + DPR all belong to the same real device). It is **seed-deterministic**: a non-empty seed (a profile id) always regenerates the same persona, while an **empty seed draws fresh entropy** (pid + nanos + counter) so each call differs — this is what a "Re-roll" relies on. `locale`/`timezone`/`country` stay neutral (`en-US`/`America/New_York`/`US`); `fingerprint_reconcile` can realign them to a proxy exit region when the UI asks. Storage quota is a constant 2,000,000,000 bytes. Note `fingerprint_reconcile` does **not** re-derive UA/client-hints/screen/GPU when only `device` changes (device→hardware mapping is applied at generation, not reconcile). See `crates/profile-manager/tests/fingerprint.rs`.

**Caveat (Chromix-only):** these generated/persisted fingerprint values are **compatibility data** — the Chromix launch path does not consume `Profile.fingerprint` (no launch flags, no CDP bootstrap); the SDK owns identity. Do not assume editing `default_fingerprint` changes what a Chromix launch actually presents. The former launcher version-sync/flag logic (`synchronize_managed_fingerprint_version`, `build_cloak_fingerprint_args`) is retained but off the launch path.
