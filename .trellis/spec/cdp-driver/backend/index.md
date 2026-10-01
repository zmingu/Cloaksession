# cdp-driver Backend Guidelines

Owns chromiumoxide session attachment, legacy active-page tools, fixed-target `BoundPage` operations, cooperative `TaskPage` leases/cancellation/waits, engine-aware bootstrap scripts, and the partial safe-enable observation API. Does not own browser processes, persistence, HTTP auth, business task scheduling/account relationships, or MCP authorization.

## Guides

- [Sessions, tools, and safety boundaries](./sessions.md)
- [Cooperative TaskPage leases, cancellation, deadlines and waits](./task-control.md) — read with `task_page.rs` before changing task execution.
- Boundaries: [launcher](../../browser-launcher/backend/index.md), [behavior generators](../../behavioral/backend/index.md), [MCP policy](../../mcp-server/backend/index.md), [Tauri registry](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read the session guide and actual methods in `crates/cdp-driver/src/session.rs`, `tools.rs`, `bound_page.rs` and `page_ops.rs`; preserve explicit Legacy/Bound behavior differences.
- [ ] For fingerprint changes read `bootstrap.rs`, `scripts.rs`, engine launch args and [shared model units](../../multizen-core/backend/contracts.md).
- [ ] Read [shared thinking guides](../../guides/index.md), the affected test, and Tauri/MCP caller before changing page state or raw-CDP routing.

## Quality Check

- [ ] `cargo test -p cdp-driver --locked` from repository root runs browser-free checks: six fixed-binding wire tests, fifteen task-control wire tests, three task-control unit tests and one &mut exclusion compile-fail doctest. The real chromiumoxide handler connects only to the shared local HTTP/WebSocket peer. These prove routing/error/coordination behavior, not actual JS/DOM/rendering or engine safety. `tests/raw_cdp.rs` still only checks entrypoint existence.
- [ ] `crates/cdp-driver/tests/safe_cdp.rs` checks refcounts and engine policy; `tests/scripts.rs` checks generated script text, not browser execution. Neither proves complete domain interception or fingerprint fidelity.
- [ ] Optional browser/network check: run `cargo test -p cdp-driver --locked --test integration -- --ignored` with `RUN_CDP_INTEGRATION=1` and an already-running CloakBrowser endpoint in `MULTIZEN_TEST_CDP` (default http://127.0.0.1:9222). It navigates to example.com and calls browser close; use a disposable test session. Without the variable it returns early.
- [ ] `.github/workflows/build.yml` runs `cargo test --workspace --locked`, not this ignored integration test. Do not report browser safety or end-to-end behavior from that command alone.
