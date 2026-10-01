# mcp-server Backend Guidelines

Owns tool schemas/catalog, transport-agnostic handlers, JSON-RPC response wrapping, HTTP gates, and in-memory activity. Tauri owns the production dispatcher, listener lifetime and token file.

## Guides

- [Tools, schemas, and dispatch](./tools.md)
- [Transport, security, and activity](./transport-security.md)
- Owners: [shared models](../../multizen-core/backend/index.md), [profile storage](../../profile-manager/backend/index.md), [CDP dispatch](../../cdp-driver/backend/index.md), [Tauri integration](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read both guides, then trace schema → metadata → `tools.rs` → `crates/tauri-app/src/mcp_embed.rs`. The embedded dispatcher does not call the reusable handlers.
- [ ] For new tools update visibility, input schema, validation, response shape, activity and both execution paths together; do not rely on a metadata label as authorization.
- [ ] Read [shared thinking guides](../../guides/index.md) and adjacent mock-driver/security tests. Check the actual browser/session method before promising behavior.

## Quality Check

- [ ] From repository root: `cargo test -p mcp-server --locked -- --test-threads=1` (Rust stable/native toolchain). Serial execution matters because raw-CDP tests mutate process-wide environment variables.
- [ ] `crates/mcp-server/tests/tools.rs` uses `tests/mock_driver.rs` plus temporary SQLite storage, not a real browser. `tests/security.rs`, `tests/activity.rs`, `tests/catalog.rs` and `tests/transport.rs` cover helper/catalog/RPC behavior.
- [ ] Transport tests call header helpers and `handle_json_rpc`, not a live HTTP server. Add route-level checks when changing route gates; do not infer HTTP/SSE integration coverage from the file name.
- [ ] `.github/workflows/build.yml` uses `cargo test --workspace --locked` without serialization. There is no current clippy/fmt CI gate to cite. Review the Tauri dispatcher separately for production-path changes.
