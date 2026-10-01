# Bootstrap Evidence Baseline

## Inspection scope

Planning inspection on 2026-09-29, starting from clean Git HEAD `aff746b`.
This is an initial evidence map, not a completed review of every source file.
Read the relevant implementations and tests before promoting a finding to a spec.

## Existing task and local constraints

- `00-bootstrap-guidelines` was created by `trellis init` with status `in_progress`, but contained only a template PRD and task metadata. No design, execution plan, context manifests, or completed guidelines were present. No task was active in this session.
- Planning is being restored before implementation; the user must approve the final planning summary before `task.py start`.
- Root `Cargo.toml` and `.trellis/config.yaml` agree on eight Rust packages. Preserve their existing backend spec roots.
- Every package index inspected is a template; each root currently has index, directory, database, error, quality, and logging pages. These are scaffolding, not evidence of actual database or logging conventions.
- Existing indexes require English documentation. Conversation with the user remains Chinese.
- `.trellis/spec/guides/index.md` contains populated shared thinking guidance; do not rewrite it simply to bootstrap package docs.
- `.gitignore` ignores all of `.trellis/`. `git status` and `git diff --check` do not validate or enumerate these new local docs. Do not force-add files or change ignore/auto-commit settings without approval.

## Source precedence and documentation drift

Use executable source, tests, manifests, and current CI before prose. README files and `docs/ACCEPTANCE.md` are discovery aids; historical test results are not results of this task.

Confirmed examples of drift:

- `crates/tauri-app/README.md` describes extensions and archives as out of scope and an earlier command registry. `crates/tauri-app/src/lib.rs::run` actually registers archive, extension, update, and other commands and wires an embedded MCP dispatcher.
- `crates/mcp-server/README.md` presents SSE as an available transport. `crates/mcp-server/src/transport.rs::handle_sse` returns HTTP 501 after auth/Host checks. The file's own opening skeleton comment is also stale: `handle_mcp` actually calls `handle_json_rpc`.
- Root `README.md` says there is no frontend test script. `crates/tauri-app/ui/package.json` has `test: playwright test`; `.github/workflows/build.yml` additionally runs the fingerprint catalog test directly with Node.
- `crates/cdp-driver/src/safe_cdp.rs` explicitly says the enable gate is not complete interception of chromiumoxide's automatic domain enables. Do not promote its existence to a browser-safety guarantee.

Do not repair these upstream docs or code in this task; write accurate new specs and identify limitations.

## Package evidence map

| Package | Ownership to document | Source/test entry points to inspect |
| --- | --- | --- |
| multizen-core | Shared models, serde compatibility, errors, engine/settings defaults | `src/{lib,error,profile,settings}.rs`; serde consumers in profile-manager/settings-store and UI `src/types.ts` |
| profile-manager | SQLite ownership, migrations, JSON columns, CRUD, profile directories, fingerprint defaults | `src/{manager,migrate,row,fingerprint}.rs`; `tests/{manager,migrate,fingerprint}.rs` |
| settings-store | JSON settings, defaults, normalization, in-process cache | `src/defaults.rs`; `tests/{store,chromix}.rs` |
| behavioral | Seeded, IO-free mouse/keyboard/scroll generation | `src/{mouse,keyboard,scroll}.rs`; matching files under `tests/` |
| browser-launcher | Process lifecycle, engine-specific arguments, proxy bridge, session restore, Chromix sidecar boundary | `src/{driver,args,chromix,socks5_bridge,session_restore}.rs`; `tests/{args,chromix,driver,socks5_bridge}.rs`; Tauri `resources/chromix/` |
| cdp-driver | Browser sessions, page operations, engine-specific bootstrap, raw CDP and gate limits | `src/{session,tools,bootstrap,safe_cdp}.rs`; `tests/{integration,raw_cdp,safe_cdp}.rs` |
| mcp-server | Tool metadata/schema/dispatch, BrowserDriver abstraction, HTTP auth/Host gates, activity records | `src/{driver,tools,schema,server,transport,security,activity,token}.rs`; matching tests |
| tauri-app | Application wiring, launcher thread, IPC commands/events, embedded MCP, archive/extensions/update integration | `src/{lib,driver,mcp_embed,registry,token}.rs`, `src/commands/`; `tests/registry_smoke.rs`; UI `src/lib/ipc.ts` and `src/types.ts` as contract consumers |

Paths in the last column are relative to each crate unless otherwise qualified. This table is an inspection map, not a claim that every listed body was read during planning.

## Directly checked examples

- `multizen-core/src/error.rs` defines the `thiserror`-based `MultizenError` and `Result<T>` alias, including automatic rusqlite, IO, and serde conversions.
- `profile-manager/src/migrate.rs::run_migrations` creates the profiles table/index and checks `PRAGMA table_info` before additive columns; `chromix_options` must be JSON object text.
- `settings-store/src/defaults.rs` caches cloned settings, defaults on read/parse errors, normalizes engine/blank binary path on load, and writes a complete `AppSettings` on update. Do not describe this as atomic persistence or hot reload.
- `behavioral/tests/mouse.rs` checks seeded determinism, distinct seeds, endpoints, and bounded backward movement.
- `tauri-app/src/lib.rs::run` installs the tracing subscriber, defaults `RUST_LOG` to info, wires state/events, and reads MCP/update settings for startup behavior.
- `mcp-server/src/transport.rs` limits request bodies to 1 MiB, dispatches parsed JSON-RPC, and exposes a separate unauthenticated health endpoint. Check the actual token/security/dispatcher functions before documenting broader security guarantees.

## Validation sources

Current `.github/workflows/build.yml` runs:

- `cargo test --workspace --locked`;
- `npm test` in `crates/tauri-app/resources/chromix`;
- `node --experimental-strip-types src/lib/chromixFingerprint.test.mjs` in the UI;
- `npm run build` in the UI (`tsc -b && vite build`);
- Playwright tests after installing Chrome;
- platform-specific Tauri compilation in its build matrix.

Node 22 and Rust stable are configured in that workflow. The UI manifest has no lint script. Do not invent an existing CI clippy/fmt gate or claim a passing cross-platform runtime test from the build matrix.

This documentation-only bootstrap requires direct document validation and source consistency review, not browser startup, dependency installation, product refactoring, or a full build. Relevant future development commands belong in the package specs, with prerequisites and optional integration tests distinguished from ordinary checks.
