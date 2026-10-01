# Tools, Schemas, and Dispatch

## Architecture and source map

`crates/mcp-server/src/driver.rs::BrowserDriver` is the Send + Sync async interface for browser operations, except **synchronous** `is_running`. `crates/mcp-server/src/server.rs::McpDispatcher` accepts tool name + JSON arguments. These are distinct seams: browser operations versus an application's tool routing.

`crates/mcp-server/src/tools.rs` holds reusable handlers taking `&dyn BrowserDriver`, `&ProfileManager`, `&ActivityLog`, and typed args. SQLite calls are synchronous. `crates/tauri-app/src/mcp_embed.rs::TauriMcpDispatcher` implements production dispatch separately, because its manager lives on the launcher thread. Changes to only the reusable handlers do not change the embedded application.

`TOOL_NAMES` currently lists 23 names, including raw `cdp_send`; default visibility is 22. `tool_definitions` filters raw CDP, chooses description/schema and exposes it through `tools/list`. Runtime dispatch is not automatically generated from the catalog. In particular `list_tabs` currently falls back to an empty `ListProfilesArgs` schema although both implementations require `profileId`.

## Schema compatibility

`crates/mcp-server/src/schema.rs` derives Deserialize/Serialize/JsonSchema on camelCase args. Local proxy and partial-fingerprint schema mirrors avoid adding schemars to core; `From<ProxyConfigSchema>` and `From<PartialFingerprintSchema>` serde-roundtrip into core. Keep both sides wire compatible; the conversions currently `expect` this compatibility. Proxy `type` is an unconstrained String in both the mirror and core (`crates/multizen-core/src/profile.rs::ProxyConfig`), not a validated enum. Successful deserialization therefore does not validate supported protocols: legacy bridge/geo code treats every non-socks5 string as HTTP, while Chromix forwards it in the proxy server scheme.

Create/update exposed fingerprint fields are userAgent, locale, timezone and country; seed is accepted but ignored. Reusable create explicitly sets unexposed fields to None. Embedded create instead deserializes core `CreateProfileInput` directly and accepts more fields than the advertised schema (including a complete object under fingerprint, plus chromixOptions). Embedded and reusable update apply a partial fingerprint to the existing full one but do not reconcile related locale/language fields; they do not support clearing proxy or updating Chromix options. See [shared contracts](../../multizen-core/backend/contracts.md).

## Handler shape and errors

Preserve the start → validate/execute → finish sequence so rejected calls also get completed activity records. Real excerpt from `crates/mcp-server/src/tools.rs::navigate`:

```rust
let res = async {
    assert_profile_running(driver, &args.profile_id)?;
    security::assert_safe_url(&args.url)?;
    let url = driver.navigate(&args.profile_id, &args.url).await?;
    Ok(serde_json::json!({ "url": url }))
}
.await;
let (status, summary) = status_summary(&res);
activity
    .finish(&id, status, summary, Some(started.elapsed().as_millis() as u64))
    .await;
res
```

`click`, `type_text`, `get_cookies` and other handlers repeat this pattern. Browser/DB failures use `multizen_core::Result`; policy failures use `MultizenError::Mcp`, and not-running uses `NotFound`. `error_json` maps error codes for helper consumers, but **HTTP does not use it**: `server.rs::dispatch_tool` emits MCP text content and `isError`, with Display error text on failure. Successful values are JSON-serialized inside a text item.

## Behavior and path differences

- Profile lists/create/update redact proxy credentials in responses. Delete checks `is_running`, closes if true, then deletes; unlike the Tauri IPC delete command, both MCP paths perform this check.
- `extract`/`screenshot` wrap driver output in `{data: ...}`. Navigation returns `{url}`, type/click return boolean markers, not detailed CDP results.
- `wait_for_selector` polls Runtime.evaluate every 150 ms; missing JSON timeout defaults to 30000, but Rust-derived Default gives zero, then execution clamps to at least 1 ms. Navigation/load waits both poll `document.readyState === 'complete'`; they do not wait for a *new* navigation. Deadlines are checked between driver calls, not hard cancellation of a stuck call.
- Reusable tab tools send raw Target methods; embedded tab operations use `TauriBrowserDriver` helpers which also maintain the active page. Raw target dispatch alone does not update that cache.
- Internal CDP helpers in `tools.rs` use `cdp_send_safe` for the denylist. Embedded internal tools send fixed commands directly; raw-CDP checks are implemented explicitly in its raw branch. The `safe` bool is not universal downstream enforcement.

Evidence: `crates/mcp-server/tests/tools.rs` and `crates/mcp-server/tests/mock_driver.rs` exercise handler wiring, not production equivalence. Catalog/device lists also occur in `crates/tauri-app/src/mcp_embed.rs`, `crates/tauri-app/src/commands/fingerprint.rs`, and core serde enums; search all before changing them.
