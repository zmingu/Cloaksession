# Archives, Extensions, and Updates

## Resource ownership

`crates/tauri-app/tauri.conf.json` bundles resources/chromix as chromix; debug builds use the source runtime directory while release uses Tauri's resource directory (`lib.rs::chromix_runtime_dir`). Rust browser-launcher owns the sidecar protocol, the owned `.mjs` bridge owns SDK adaptation, and vendored SDK internals are not a new project layer. Follow the [Chromix guide](../../browser-launcher/backend/chromix.md).

Companion manifest/script are embedded with include_str and written to the data directory at startup. `crates/tauri-app/src/commands/companion.rs` polls every 600 ms using the session DOM-attribute channel, parses JSON id, installs, emits a result, closes, waits 500 ms and relaunches. A successful relaunch starts another poller; failed/consumed signals end that poller. No duplicate-poller suppression or strong origin/authentication guarantee exists. It bypasses the command's Web Store ID parser, so do not treat every install_from_web_store caller as validated.

## Extensions

`crates/tauri-app/src/commands/extensions.rs` owns download/dialog/unpack/meta/icon work. Profile metadata goes through driver list/set commands; don't open SQLite here. Reuse `install_from_web_store` for command/companion installs, while preserving caller-boundary validation.

- Web-store/file installs use shared extensions/<id>; prepare variants stage configs without attaching a profile. Existing target directories are reused, not refreshed by version.
- ZIP extraction uses spawn_blocking, enclosed_name, a temporary sibling and rename. Failure is not a transactional rollback, and downloaded CRX authenticity/signature verification is not implemented by scanning for ZIP bytes. Don't call the fallback scan a complete CRX parser.
- Folder installs reference the chosen folder directly despite scope shared. IDs derive from manifest.key text or a path hash; this is not Chromium's key-to-ID algorithm. File IDs use a short SHA-256 hash.
- Remove/toggle updates metadata only; remove deliberately leaves shared files. Startup orphan sweep is separate. New extension launch args take effect on relaunch, not live metadata mutation.
- `parse_web_store_id` currently accepts 32 ASCII alphanumeric characters and http-prefixed inputs' last path segment, not only a-p or verified Web Store hosts. Icon paths come from ext.dir/manifest and are not confinement-checked here.

Use contextual String errors at these IPC edges. File-picker cancellation returns unchanged lists for add commands and None for prepare commands. Do not replace those non-error results with a thrown error without updating wrappers/UI.

## MZAR archives

`crates/tauri-app/src/commands/archive.rs` owns the format and dialogs. Version 2 is exported; header versions 1/2 are accepted. Layout is magic + BE u16 version + 16-byte salt + 12-byte nonce + AES-256-GCM ciphertext/tag. Scrypt parameters are log2(N)=14, r=8, p=1, output 32 bytes; no AAD. Plaintext is BE-u32-sized JSON manifest followed by sized file chunks in manifest order. Preserve this compatibility when changing it; the metadata hashes are SHA-256.

Export reads complete profile/extension trees into memory and encrypts them. It checks passphrase **byte length** >=8, not Unicode character count, and does not stop a running profile before collecting files. It is not a consistent live-database snapshot or streaming archive.

Import verifies header/decryption/chunk bounds/checksums, chooses a safe unused ID or UUID, rewrites profile data_dir and inserts into DB **before** restoring files. File write errors are warned and can still yield ok:true; filesystem+DB restore is not atomic. Imported extension directories in profile metadata are not rewritten; bundled files are restored under extensions/<id>/<version>, unlike regular installs. Do not promise portable extension references or complete rollback.

`write_guarded` attempts canonicalization/string-prefix checks but falls back for nonexistent paths and creates parents before checking. It is not a reliable traversal/symlink-confinement guarantee. Manifest size values are not fully enforced and there is no overall resource limit. Treat archive data as a trust boundary; do not reuse this helper as a proven security primitive. Format/security/runtime tests would be needed for a future archive change, not invented as existing coverage.

Cancellation/selected validation failures return ok:false payloads, while decryption/IO can return Err(String). The UI wrappers in `crates/tauri-app/ui/src/lib/ipc.ts` must handle both, not only the declared success/failure union.

## Updates

`crates/tauri-app/src/commands/update.rs` owns in-memory UpdateState, GitHub latest-release checking and status events. `is_newer` is a numeric-component heuristic, not full semver prerelease ordering. Client/parse errors can return early leaving Checking status; HTTP failure explicitly emits Error and updates last_checked.

Windows may auto-download a selected setup executable during update_check. Progress/state can become Ready while the command still returns Available. Download code does not verify checksum/signature or final byte count and ignores flush failure; Ready is not an authenticity guarantee. update_install separately spawns the cached installer without silent flags. Non-Windows does not auto-download; update_download uses a browser-open command on Windows/macOS only (no Linux opener in the body).

Update state is not persisted despite its comment. Event payload field names and wrapper shape are described in [IPC](./ipc.md). Never run download/install commands as a documentation validation step, and do not infer updater reliability from the Tauri build matrix.
