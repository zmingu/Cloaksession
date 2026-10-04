# Sessions, Tools, and Safety Boundaries

## Source map and lifecycle

- `crates/cdp-driver/src/session.rs`: `BrowserSession`, HTTP endpoint discovery, handler pump, active page, target methods, raw commands, companion signal polling.
- `crates/cdp-driver/src/tools.rs`: legacy active-page tool wrappers and consuming `close`.
- `crates/cdp-driver/src/bound_page.rs`: borrowed fixed-target API; `page_ops.rs`: shared page-scoped implementation with explicit Legacy/Bound compatibility policy.
- `crates/cdp-driver/src/task_page.rs`: cooperative session/target leases, cancellation, typed operation deadlines, selector waits. See [TaskPage control contract](./task-control.md); ordinary BoundPage remains nonexclusive.
- `crates/cdp-driver/src/bootstrap.rs` / `crates/cdp-driver/src/scripts.rs`: bootstrap entry point and generated JS. **For Chromix (the only engine) `bootstrap_targets` is a no-op** — the SDK bridge owns identity/context emulation, so nothing is installed at CDP level. `scripts.rs` still contains the generated JS builders (fingerprint preload, WebRTC block/spoof) but they are no longer invoked by the launch path; treat them as retained code, not live behaviour.
- `crates/cdp-driver/src/safe_cdp.rs`: local refcounts and a now-unconditional domain policy, **not full enforcement** (see below).

`BrowserSession::connect` expects an HTTP CDP base endpoint, fetches `/json/version` (20 attempts separated by 500 ms), obtains `webSocketDebuggerUrl`, attaches with `ignore_invalid_messages`, and spawns the handler stream pump. This is not a hard ten-second deadline: HTTP requests have no explicit per-attempt timeout here. Connection errors become contextual `MultizenError::Cdp` values; handler stream items/errors are currently ignored.

`active_page` returns its cached page or the first existing page; it does not create a page. `navigate` creates one on an active-page error, caches it, then calls goto. `new_page`/`activate_page` also update the cache; `close_page` selects a remaining page only if the closed target was active. The mutex protects the cached handle, not a transaction spanning multiple tool calls. Concurrent tools can observe interleaved active-page changes.

Tauri's `ProfileRegistry` stores per-process UUID slots containing an `Arc<BrowserSession>` through OnceCell. Removing a slot cancels its read-only identity tasks and rejects stale attach/commit; process closure is still a separate launcher operation. `close(mut self)` consumes the session and calls browser close, ignoring errors; it cannot be invoked through a shared Arc. See [Tauri identity lifecycle](../../tauri-app/backend/kuaishou-identity.md) for the current generation guards; this is not a universal OS/CDP liveness lease.

## Legacy tool results and known ceilings

`navigate` returns actual URL/title from evaluation. Its timeout argument only wraps a post-goto sleep, **not** goto or the whole operation. `screenshot` returns base64 PNG without a data-URI prefix. `extract` returns `{url,title,text}` with innerText sliced to 8000 characters, not a structured accessibility tree.

`click` evaluates the target center, errors when absent, generates a short deterministic approach using coordinates as seed, then dispatches mouse events. `type_text` focuses the selector, uses text byte length as delay seed and emits events per Rust char with millisecond sleeps. Many event execution/focus errors are discarded, so success is not confirmation the requested input reached the page. Behavioral generation remains pure in its [owner crate](../../behavioral/backend/generators.md); sleeping/CDP dispatch remains here.

Preserve contextual error conversion rather than flattening low-level failures silently. Real excerpt from `crates/cdp-driver/src/session.rs::new_page`:

```rust
let page = self
    .browser
    .new_page(url)
    .await
    .map_err(|e| MultizenError::Cdp(format!("new page {url}: {e}")))?;
let target_id = page.target_id().as_ref().to_string();
self.set_active_page(page).await;
Ok(target_id)
```

The same contextual pattern appears in `activate_page`, `cdp_send` and `page_ops.rs::screenshot`; use it for new operations. This crate emits tracing warnings/debug context but installs no subscriber and has no DB/log persistence policy.

## Fixed-target Rust task operations

### 1. Scope / trigger

New Rust automation must bind its target rather than use the session's mutable active-page cache. Business automation is Rust + chromiumoxide; the existing Chromix Node launch bridge remains, with no new Node business worker. This batch adds no business scheduler, account workflow or Tauri/MCP command.

### 2. Signatures

```rust
// BrowserSession
pub async fn bind_page(&self, target_id: &str) -> Result<BoundPage<'_>>;
pub async fn new_bound_page(&self, url: &str) -> Result<BoundPage<'_>>;
// BoundPage<'session> (also exported as cdp_driver::BoundPage)
pub fn target_id(&self) -> &str;
pub async fn navigate(&self, url: &str, timeout_ms: u64) -> Result<NavResult>;
pub async fn evaluate(&self, expression: &str) -> Result<serde_json::Value>;
pub async fn screenshot(&self) -> Result<String>;
pub async fn click(&self, selector: &str) -> Result<()>;
pub async fn type_text(&self, selector: &str, text: &str) -> Result<()>;
pub async fn extract(&self) -> Result<serde_json::Value>;
```

### 3. Contracts

- `BoundPage` privately holds one chromiumoxide `Page` and borrows its `BrowserSession` (including engine/safe observation policy). Keep an `Arc<BrowserSession>` in a spawned task and bind inside it. It cannot outlive the session borrow.
- Binding looks up an already attached target. It does not activate, write/read `active_page`, create a replacement page, or fall back to another target. A lookup is not a liveness lease: closure can race a successful lookup, and the next operation may fail.
- `new_bound_page` requests `Target.createTarget` with `background=true`; it never changes the legacy cache. The browser may ignore the background request. An initially empty cache still has its ordinary first-page fallback on the next legacy call.
- Drop does not close the page/browser. The caller owns explicit cleanup through `close_page(target_id)`. Binding is not a mutex, task scheduler, ownership lease, or browser-profile isolation; multiple bindings/users can still act on the same page.
- `NavResult` remains `tools::NavResult { url: String, title: String }`; screenshot returns PNG base64 without a prefix; extract returns `{url,title,text}` (8000 innerText characters max, not AX).
- Shared implementation is `page_ops.rs::PageOperations` with explicit `OperationPolicy::Legacy/Bound`. Legacy selection, timeout and best-effort Input behavior remain compatible.
- Bound navigation deadline wraps goto and metadata extraction. Timeout stops waiting, **not** an already-sent browser action; callers must not blindly retry a business action.
- Chromiumoxide 0.9.1 high-level screenshot calls `activate()`. Bound screenshot dispatches `CaptureScreenshotParams` directly on the fixed page, avoiding that implicit activation. Arbitrary evaluated JS, navigation and page-authored behavior are not sandboxed by this API.

### 4. Validation & error matrix

| Condition | Bound behavior |
| --- | --- |
| Unknown / empty / destroyed target after handler processes destruction | `MultizenError::Cdp`, no fallback |
| Closed page used through an existing binding | Original page/session channel fails; never selects active page |
| Evaluation/JS/serialization/screenshot error | Contextual error propagated |
| Click selector missing | `element not found` |
| Typing selector missing / focus evaluation fails | Error before key dispatch (also for empty text) |
| Mouse move/press/release or key down/up CDP failure | Propagate at first failed command; no success masking |
| Navigation deadline expires | Error naming target and deadline; no alternate-target retry |
| New task page creation fails | Error, no cache update or retry |

Input success only confirms dispatch, not visibility/actionability, actual DOM focus, application acceptance, or business completion. Strict error propagation does not undo earlier successfully dispatched input. Existing connection retry ceilings and safe-enable gaps still apply.

### 5. Good / base / bad cases

Good: bind A and B, switch legacy active to B, run `a.extract()` and get A. Base: old `session.evaluate()` continues to use the active page. Bad: bind missing/closed A and silently use B; this is forbidden.

### 6. Tests required

`tests/page_binding.rs` runs six browser-free HTTP/WebSocket CDP tests against the real chromiumoxide handler: two-target concurrent evaluation and all operation routing, active-switch independence, unknown/closed target errors, each input stage's failure plus legacy compatibility, background creation/cache preservation/creation failure, whole-navigation deadline. It records actual outgoing method/sessionId/params and validates frame fixtures with the generated CDP type (malformed frame initialization otherwise gets silently ignored by the library).

The peer does not execute JavaScript, render PNGs or model platform DOM/actionability; its target-tagged payloads prove transport routing only. Real browser/engine behavior remains unverified by these tests. Never run the old ignored integration against a user's session: it closes the browser.

### 7. Wrong vs correct

```rust
// Wrong for tasks: another caller can switch active between these calls.
session.activate_page(target_id).await?;
session.extract().await?;
// Correct: selection remains bound even when another caller activates a tab.
let page = session.bind_page(target_id).await?;
page.extract().await?;
```

## Raw CDP is dispatch, not authorization

`BrowserSession::cdp_send` builds a `RawCdpCommand` whose serialization is just params (absent params become JSON null). It first requires an active page even for browser-level methods, rejects an explicit session ID that differs from that page's session, routes four Target operations through the browser, and routes everything else through the page. It does **not** enforce an allowlist despite its doc comment. MCP opt-in/denylist/URL checks belong upstream; direct internal callers bypass them. `crates/tauri-app/src/driver.rs` delegates arbitrary method strings here and ignores its `_safe` argument.

`safe_enable_check` combines `SafeEnableRefcount::should_enable` with `cloak_allows_domain`. **Since Chromix is the only engine, `cloak_allows_domain` is now unconditionally `true`** — the old CloakBrowser rule that rejected `Runtime`/`Network` is gone (the patched CloakBrowser build tripped a `DCHECK` there; Chromix has no such restriction). The function name is kept as a named API so call sites stay explicit. The tools only log when the gate would block. chromiumoxide's automatic enables are not intercepted, and paired disable bookkeeping does not undo a browser crash. Tests in `crates/cdp-driver/tests/safe_cdp.rs` prove only the local policy/refcount functions.

## Bootstrap: no CDP-level engine split (Chromix owns identity)

`bootstrap_targets` **immediately returns `Ok(())` for Chromix** (the only engine) — the SDK bridge owns identity and context emulation, so there is no CDP bootstrap to apply. The former CFT/CloakBrowser branches (WebRTC spoof, fingerprint preload, UA override, locale evaluate) were **removed with those engines**; the function signature (`session`, `fp`, `engine`, `webrtc_spoof_ip`) is kept so callers did not have to change, but every parameter is ignored. `crates/tauri-app/src/driver.rs` still calls it after attach (with `webrtc_spoof_ip = None`); it is a no-op for Chromix. `crates/cdp-driver/src/scripts.rs` retains the generated-JS builders but nothing in the launch path invokes them.

There is no target-created listener to bootstrap every future tab. `crates/cdp-driver/tests/scripts.rs` checks text-generation properties of the retained builders; real browser effects require separately authorized runtime testing. `poll_companion_signal` evaluates matching pages, substring-filters URLs, reads/clears the DOM attribute and skips evaluation failures; it is not an authenticated browser-origin channel.
