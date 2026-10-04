# Runtime Ownership and Lifecycle

## Startup and thread boundaries

`crates/tauri-app/src/lib.rs::run` initializes one tracing subscriber (`RUST_LOG`, default info), installs the dialog plugin, builds/manages `AppState`, registers commands and runs the shell. `resolve_paths` uses app-local data, then app-config, then current directory; it creates paths for profiles.db, profiles/, extensions/ and settings.json. Actual app identity is com.cloaksession.browser in `crates/tauri-app/tauri.conf.json`, not the older comment's identifier.

`AppState` contains Arc driver/activity, async mutex settings/token, and a std mutex update state. Keep DB work behind `TauriBrowserDriver`'s command channel. `crates/tauri-app/src/driver.rs::launcher_thread_main` constructs ProfileManager/BrowserLauncher inside a dedicated OS thread with a current-thread Tokio runtime + LocalSet. `Arc<ProfileManager>` is not Send/Sync because the manager is not Sync; don't move it into a spawned async task. No unsafe Send impl or extra connection is needed for an IPC handler.

`LauncherCmd` has a 64-slot mpsc channel and oneshot replies. `launcher_task` handles commands serially; a long launch delays CRUD. New manager operations should follow existing ListProfiles/GetProfile/SetExtensions variants and paired driver wrappers. `start` only confirms thread spawn, not successful DB/runtime initialization. Channel failures become contextual Mcp errors; initialization failures are traced and end the thread. Explicit Shutdown calls close_all, but channel EOF simply exits the loop; shutdown is not a joined, guaranteed app-exit cleanup hook.

## Browser lifecycle

Driver launch sends a command, waits for launcher success, attaches through `ProfileRegistry`, reads the profile, bootstraps CDP (a no-op for Chromix), then sets its sync running cache and emits success. **Chromix is the only engine**: global settings shallow-merge per-profile options on the launcher thread before SDK launch, and every launch goes through `launch_with_chromix`. The driver captures engine/binary/Chromix/skip-download settings at startup; saving settings does not rebuild it.

On CDP-attach failure Chromix requests launcher close only if the captured Arc slot is still current (ClosePrepared is checked on the serialized launcher loop); an old failure cannot kill a reopened process. Later profile-read/bootstrap failure lacks complete rollback. A failed launch therefore is not proof no process/session remains. After bootstrap, running-cache and success-event publication require the same current, uncancelled slot under the registry lock. Close first removes the registry entry, then requests launcher close; its late reply clears the cache/emits stopped only while no replacement slot exists. It can leave cache/registry inconsistent on a channel error. Do not add cleanup promises based on success-event comments.

`is_running` remains a std-mutex HashSet UI cache and may be stale after external exit. `registry.rs` now stores per-process marker slots with UUID, launch-network snapshot, cancellation and a OnceCell<Arc<BrowserSession>>. Concurrent attachments share one cell; close/replacement cancels and rejects stale completion. Launcher prepares before replying; driver uses connect_prepared so a stale reply cannot recreate a removed slot. Identity read/list/save checks launcher liveness and projects stored observations against the current UUID. Chromix process liveness uses the SDK supervisor flag. See [identity lifecycle](./kuaishou-identity.md).

Page tools use `require_session`: they do not auto-launch. `cdp_send` forwards arbitrary method/params to the session and ignores `_safe` (the old module table claiming Runtime.evaluate-only dispatch is stale). [CDP](../../cdp-driver/backend/sessions.md) owns active-page routing; [MCP](../../mcp-server/backend/transport-security.md) owns its external policy checks.

## Business metadata and isolation gate

The same serialized LauncherCmd loop now handles business list/state/save/unbind and guarded Profile option updates. Save carries startup engine/global Chromix settings, requires launcher liveness=stopped for target and any existing binding, validates effective directories, then invokes one manager transaction. The Launch branch validates before any process/proxy/mark_opened side effect for both UI and embedded MCP; it then reuses exactly the same shallow-merged options for Chromix. BrowserHandle snapshots retain occupied live directories even after an unscoped Profile's options are changed. Scope-bearing running Profiles cannot change effective directories or be deleted through IPC. See [business isolation](../../browser-launcher/backend/business-isolation.md).

Windows driver tests link real Tauri event code even when AppHandle=None. These imports require Common Controls v6; build.rs emits an embedded linker manifest for Rust test harnesses on MSVC, then /MANIFEST:NO for binary targets which already carry tauri-build's resource.lib. `rustc-link-arg-tests` alone does not apply to library unit tests. Without the harness manifest, added driver tests failed before executing with STATUS_ENTRYPOINT_NOT_FOUND; generic embedding without the binary override caused duplicate resource MANIFEST/1. This is a test-linker prerequisite, not a runtime browser workaround.

## Embedded MCP and background work

`crates/tauri-app/src/mcp_embed.rs` implements McpDispatcher separately from the library's reusable handlers; review [both paths](../../mcp-server/backend/tools.md). It binds 127.0.0.1 with token auth, logs bind/serve failures, and offers no stop/rebind handle. A OnceLock retains the first exposed McpState but does not itself prevent subsequent start calls from attempting another listener. Startup's "requested" log is not bind success.

`crates/tauri-app/src/token.rs` trims/reuses any 64-character ASCII hex token (uppercase also accepted), otherwise writes two UUIDv4 simple strings. Do not repeat the comment's 256-bit-entropy claim: UUIDs contain fixed version/variant bits. Unix permissions are set to 0600 **after** writing; existing valid tokens are not chmod'ed, and Windows uses default ACLs. Token generation is attempted even if HTTP is disabled. `system_info` exposes it to the local renderer and reports a hardcoded 7777 URL, not necessarily the bound port.

Startup in `lib.rs` also:

- Writes embedded companion files if contents differ and ensures the extensions directory, ignoring these filesystem failures.
- Bridges ActivityLog broadcasts to activity:event; warns/continues after lag but does not replay missed events (recent history is separately queryable).
- Sequentially probes missing proxy countries with a 6000 ms request, best-effort persists lowercase country, then emits `{id,country}` even if persistence failed.
- Sweeps extension directories not referenced by stored dir basename, including temporary unpack directories. It is not a transactional reference-count collector.
- After eight seconds reads autoUpdate from the settings store and may run update_check; this is a single delayed check, not a scheduler. On Windows a new release can auto-download, but installation remains a separate command.

Library tracing is enabled here, not in each crate. Push events and maintenance work are best-effort; do not log tokens, proxy credentials, passphrases or full sensitive tool outputs when adding diagnostics.
