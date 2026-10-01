# Transport, Security, and Activity

## Actual HTTP / JSON-RPC surface

`crates/mcp-server/src/transport.rs::build_router` defines:

| Route | Gates / result |
| --- | --- |
| GET /healthz | Ungated health JSON (`multizen-mcp`), no Host/auth check |
| POST /mcp | Optional bearer auth, exact Host check, 1 MiB body limit, JSON parse then `handle_json_rpc` |
| GET /sse | Same auth/Host checks, then **501 not implemented** |

Auth is enforced only for `Some(token)`; Tauri passes Some. `parse_bearer` trims whitespace but requires case-sensitive `Bearer `. `allowed_hosts` contains exactly 127.0.0.1:port and localhost:port, not IPv6 or arbitrary origins. Header enforcement is not network binding: the application binds IPv4 loopback in `crates/tauri-app/src/mcp_embed.rs::start_embedded_mcp`. No TLS, CORS/origin policy, or production stdio/SSE transport is implemented here. An rmcp dependency in `crates/mcp-server/Cargo.toml` is not proof of rmcp transport wiring.

Body-read/size failure returns HTTP 400; invalid JSON returns a JSON-RPC -32700 body. `crates/mcp-server/src/server.rs::handle_json_rpc` supports initialize (protocol 2024-11-05), initialized notification, ping, tools/list and tools/call; unknown method is -32601. It defaults absent ID to null and returns replies even to notifications; it does not implement batch handling or full envelope validation. Do not claim complete protocol compliance from the happy path tests.

`crates/mcp-server/src/token.rs::token_matches` uses subtle constant-time byte comparison for equal lengths, with a separate unequal-length branch. Token generation/file permissions belong to Tauri; do not duplicate token persistence here.

## Policy boundaries and limitations

`crates/mcp-server/src/security.rs` is a **denylist**, not a strict CDP allowlist or browser sandbox:

- URL scan removes tabs/CR/LF and leading ASCII controls, then case-sensitively blocks file:, chrome:, devtools:, view-source:. It is not a parsed http/https-only validator and does not normalize scheme case.
- Raw-CDP param scan recursively checks string values in objects/arrays. It does not inspect object keys, scripts' semantics or redirects.
- Exact blocked methods include IO.read, Page.getResourceContent, Storage.getCookies, Network.getAllCookies, Browser.close/crash; blocked prefixes are DOMStorage., IndexedDB., CacheStorage., Fetch. Everything else is allowed by this helper.
- `MULTIZEN_MCP_ALLOW_RAW_CDP` accepts only exact `1`, `true`, `yes`, `on`. It controls catalog visibility and the raw tool branch, not controlled tools such as evaluate_js/get_cookies. Arbitrary evaluation remains available without raw CDP opt-in.

Keep the explicit running/URL/method/param checks in both execution paths. Do not describe `_safe` on Tauri's driver or [safe-enable observation](../../cdp-driver/backend/sessions.md) as enforcement of this policy. Tests in `crates/mcp-server/tests/security.rs`, `tests/catalog.rs` and `tests/tools.rs` cover helpers and representative denials, not complete malicious-input resistance.

## Activity versus tracing

`crates/mcp-server/src/activity.rs::ActivityLog` holds a 500-event in-memory deque under a std mutex plus a 256-slot broadcast channel. `start_call` creates UUID/RFC3339 timestamp, sanitized args and pending status; `finish` updates the matching event and broadcasts it. `recent` returns newest first. If the event was evicted before finish it is not reinserted. This is UI activity, not durable audit storage or lossless telemetry.

`sanitize_args` recursively removes username/password under proxy objects, replaces cookie-array values, and truncates long text. Length threshold is bytes, truncation is characters (77 + ellipsis); don't promise an 80-byte Unicode bound. It does **not** redact arbitrary secrets in all strings. `finish` summaries are unsanitized strings derived from output/errors; embedded cookie/evaluation results can therefore reach activity summaries. Avoid logging new credentials/content and review both args and summaries when adding sensitive tools.

Tauri bridges broadcast events to `activity:event`, continuing after lag. `crates/mcp-server/tests/activity.rs` covers basic redaction, update and capacity behavior. Diagnostic tracing/subscriber configuration remains at app startup, not in each library; the activity log needs no database/ORM/log-file scaffold.
