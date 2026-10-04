# Business directory isolation

## Gate and ownership

`tauri-app/src/driver.rs::launcher_task` handles both UI and embedded MCP Launch commands. It calls `BrowserLauncher::validate_business_directory` **before** `mark_opened`, proxy operations, Node spawn, or CDP attach, using the startup engine and global Chromix snapshot. After validation it uses the same `global.with_profile_options` shallow merge to launch through `launch_with_chromix`. Direct low-level `launch_with_chromix` calls are process primitives; a new application caller must use the serialized gate and supply the actual global settings. Tests of SDK primitives alone are not business isolation tests.

`browser-launcher/src/business_guard.rs` also provides stopped-only save/unbind and guarded Profile updates. These run on the same serialized launcher thread as launch, so an async UI cached-state check is not authoritative. Chromix uses its SDK supervisor liveness flag to decide running/stopped. No unsafe Send/Sync, extra DB connection, reverse profile-manager dependency, or new Node worker.

## Actual directory

`data_dir.rs::default_data_dir` is shared by the launch request and the isolation checks. **Chromix is the only engine**: the default is `<profile.data_dir>/engines/chromix` (SDK options can override it via top-level `userDataDir`). `effective_data_dir` takes the **already merged** config and respects a top-level nonempty string userDataDir, never silently rewrites it. Known nested userDataDir and raw directory/profile-directory flags in args/ignoreDefaultArgs across all layers are refused, aligned with read-only bridge.prepareOptions. Unknown values/objects/arrays/nulls/false remain untouched.

Windows normalization supports drive-absolute and ordinary relative paths resolved against inherited process cwd; slash/backslash, local extended drive prefixes, normal `..`, existing short-name/symlink/junction resolution, nearest-existing-parent plus missing suffix. Component overlap compares Windows ordinal ignore-case (CompareStringOrdinal), never string starts_with. POSIX uses case-sensitive components. Refuse unreadable/non-directory/dangling paths, non-Unicode/control/overlength paths, UNC/network/device/drive-relative/root-relative paths, alternate streams, ambiguous trailing dots/spaces and DOS device names. Paths combining reparse/symlink traversal with `..` are refused because SDK/OS lexical semantics can differ; supply a canonical absolute path instead. Permission errors never become a permissive lexical fallback.

A candidate reserved/proposed jinniu compares against **all other Profiles**, including unscoped ones. A non-jinniu candidate compares against jinniu reservations. Equal, ancestor and descendant directories all conflict; siblings with similar prefixes do not. Unrelated non-jinniu-to-non-jinniu sharing is not globally prohibited by this policy. With no relevant Jinniu reservation/live handle, the isolation gate returns before parsing ordinary paths, including kuaishou-scoped Profiles; original bridge validation remains. Save validates before the atomic DB write; launch checks again.

## Running snapshots and updates

Every BrowserHandle retains the actual raw directory and best-effort verified directory identity at launch plus scope snapshot. Relevant checks include live handles even if absent from the latest DB, both original canonical identity and current raw-path resolution. An unscoped running Profile cannot hide its occupied directory by editing options. A missing/unverifiable original identity fails closed when relevant to a new jinniu check.

Profile options are validated on a candidate before pm.update, preserving stored options/timestamp on failure. With a reservation, a running Profile cannot change effective directory, but identical autosaves and edits that preserve the effective directory are allowed. Other Profile metadata edits are not globally disabled. IPC deletion of a running reserved Profile is rejected to avoid dropping its active reservation; MCP's existing close-before-delete behavior remains.

## Limits and tests

This is application data-directory policy, **not a domain firewall or proof of login/session independence**. It does not stop users browsing other sites. It is not an OS lock against external browser processes, hostile filesystem remapping between check/use, direct DB edits, SDK replacement or startup code that changes cwd. It tracks current configured and live paths, not a historical registry of all directories formerly used while stopped. Global settings remain the driver's startup snapshot; restart applies changes. macOS filesystem case behavior has not been validated by this Windows run.

Tests: launcher `tests/data_dir.rs` and `tests/business_guard.rs`; Tauri `driver/business_tests.rs` exercises the actual channel loop with a protocol-only fake bridge, cached-running=false, stopped-only writes, atomic options rejection, retained live path, both launch directions and no-spawn/no-mark-opened failure. SDK fake-bridge tests remain separate; no real browser/platform was used.
