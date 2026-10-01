# Cooperative TaskPage control

## Scope / source map

`crates/cdp-driver/src/task_page.rs` owns the new Rust-only coordination layer over `BoundPage`. `BrowserSession.task_locks` owns the weak lock registry; `BoundPage::into_task` delegates to `BrowserSession::task_page`. No MCP/Tauri/Chromix/business/account schema changes or reverse dependency on MCP. The original BoundPage and legacy tools remain available with unchanged result types.

## API (current contract)

```rust
// cdp_driver::{TaskPage, TaskCancel, TaskError, TaskResult, SelectorState}
// BrowserSession
pub async fn task_page(&self, target_id: &str, cancel: TaskCancel, timeout: Duration)
    -> TaskResult<TaskPage<'_>>;
// BoundPage<'session>, consumes the nonexclusive binding
pub async fn into_task(self, cancel: TaskCancel, timeout: Duration)
    -> TaskResult<TaskPage<'session>>;
// TaskCancel: Clone + Default, one-shot tokio watch (no reset)
pub fn new() -> Self;
pub fn cancel(&self);
pub fn is_cancelled(&self) -> bool;
pub async fn cancelled(&self); // also allows enclosing read-only network work to select cancellation
// TaskPage: no Clone, Deref, raw BoundPage/Page access
pub fn target_id(&self) -> &str;
pub fn release(self);
pub async fn navigate(&mut self, url: &str, timeout: Duration) -> TaskResult<NavResult>;
pub async fn evaluate(&mut self, expression: &str, timeout: Duration) -> TaskResult<Value>;
pub async fn screenshot(&mut self, timeout: Duration) -> TaskResult<String>;
pub async fn click(&mut self, selector: &str, timeout: Duration) -> TaskResult<()>;
pub async fn type_text(&mut self, selector: &str, text: &str, timeout: Duration) -> TaskResult<()>;
pub async fn extract(&mut self, timeout: Duration) -> TaskResult<Value>;
pub async fn wait_for_selector(&mut self, selector: &str, state: SelectorState,
    timeout: Duration, poll_interval: Duration) -> TaskResult<()>;
```

Timeout is per acquisition/operation, not an implicit total lifetime budget. Acquisition covers queue plus post-lock lookup. Each operation covers all nested CDP waits; selector wait covers every evaluate and sleep. Zero timeout returns TimedOut without dispatch. A timeout beyond the clock range returns InvalidOptions. All actions use &mut self; a compile-fail doctest prevents same-lease join! concurrency. BrowserSession may live in an Arc retained by a spawned task; bind/acquire inside that task.

## Lock / lifecycle contract

- Exactly one new-interface lease for a `(BrowserSession instance, target_id)`; separate bindings share the same lock. Different targets and different session instances are independent (not a global target/profile lock).
- Registry holds Weak<tokio::sync::Mutex<()>> only. Owners/queued futures retain strong references. Acquisitions prune dead entries, avoiding a strongly retained lock per ever-seen tab. Registry entries may remain stale until the next acquisition.
- Lock first, then bind the existing target again. Never read/change active page, activate, create a fallback, or retry another target. Target disappearance after lookup may still race execution.
- release/drop releases the guard, not the tab/browser. Failed/abandoned acquisition also releases it. Cancellation does NOT automatically unlock a retained TaskPage; explicit release/drop remains required.
- Cooperative scope only: ordinary BoundPage, legacy tools/raw CDP, user input, other BrowserSession connections, arbitrary evaluated page JS and page-authored behavior can still change the same target.

## Error / cancellation matrix

| Condition | Result / continuation |
| --- | --- |
| Pre-cancel, including ready lock/operation/deadline | Cancelled before dispatch; biased select prioritizes cancellation |
| Queue or whole-operation deadline | TimedOut; queued future removed; existing lease is terminal |
| Cancel during request or selector polling/sleep | Cancelled promptly at async polling; lease terminal |
| Caller drops/aborts already-polled action future | Interrupted on subsequent action through retained lease |
| Driver/JS/CDP/missing target/malformed observation | Driver(MultizenError), immediate; no automatic retry/fallback; ordinary driver error does not terminally poison lease |
| Invalid poll interval (zero) / overflowing timeout | InvalidOptions, no dispatch; configuration errors do not poison lease |
| Later operation after Cancelled/TimedOut/Interrupted | Same terminal reason; no new dispatch (invalid wait options are validated first) |

TaskCancel uses existing tokio watch; cancellation is one-way and clone-shared, dropping a handle is not cancellation. A watch sender stays alive during a waiter; no closed-channel busy loop. The control future pessimistically latches Interrupted before polling work, clears it only on completion, and selects cancel -> deadline -> operation. Tokio async cancellation is cooperative, not preemption of synchronous CPU work.

**Cancellation/timeout/drop cannot recall already queued/dispatched CDP, stop page-authored JS, prove browser completion, or roll back a browser transaction.** Dropping a click/type future prevents its later Rust dispatches, but already sent keyDown/mousePressed may remain without keyUp/release. Releasing the lease can permit the next task while browser effects are still in flight. Do not blindly retry side-effecting operations; recovery/compensation belongs to a separately approved higher layer.

## Selector state / validation

JS uses `serde_json::to_string(selector)`, not Rust Debug escaping. It calls document.querySelector, returns `{attached,visible}`, and Rust matches:

| State | Condition |
| --- | --- |
| Attached | Element exists |
| Detached | Element does not exist |
| Visible | Exists, computed display != none, visibility != hidden/collapse, bounding rect width/height > 0 |
| Hidden | Detached OR not visible |

Visible does not check opacity, viewport, occlusion, stability, enabled status, actual focus, application acceptance, or full actionability. First matching element only; no iframe/shadow traversal. Invalid CSS selectors throw normally; JS exceptions/CDP errors/deserialization failures propagate, never become an absent element. poll_interval must be positive. Selector query has no side-effect retry policy beyond its explicit read-only polling.

## Good / base / bad

Good: acquire A, run wait -> click -> extract with &mut lease, finally drop. Base: legacy tools still operate on their active page without acquiring this lock. Bad: maintain one lock per BoundPage, expose raw page/Deref, reuse a timed-out lease, treat dropping the future as browser rollback, or fallback from closed A to B.

## Offline verification

`tests/common/mod.rs` is the shared HTTP/WebSocket peer extracted byte-exact with filesystem-copy from the previous page_binding tests. `tests/task_control.rs` contains 15 wire tests on real chromiumoxide handler routing/response decoding, queue release/drop, cross-target/session independence, ready-branch cancellation, direct/selector evaluate cancellation, timeout, input press interruption, closure revalidation, and no fallback. Three unit tests cover weak registry reuse/pruning, state/JSON matching, sticky clone cancellation; one compile-fail doctest covers &mut exclusion. Existing six page_binding tests still run unchanged apart from fixture imports.

Use `cargo test -p cdp-driver --locked`, `cargo test --workspace --locked`, `cargo check --workspace --locked`. Never automatically run ignored browser integration (it closes the browser). The peer scripts observation values and exceptionDetails, does not parse/execute selectors, render pages, model actual focus/actionability, or verify engine safety. String escaping assertions supplement wire tests; they are not DOM proof.

### Public avatar GET amendment (2026-10-01)

TaskPage::read_public_avatar(url, timeout) is a narrow yximgs HTTPS443-only capability, not arbitrary raw CDP. It uses the bound browser's native GET and response-stage Fetch stream; no host HTTP, navigation, activation, universal isolated-world access or CORS changes. The operation requires &mut self and uses existing controlled cancellation/deadlines. A worker retains a separate weak session/target lock through bounded abort/IO.close/Fetch.failRequest/Fetch.disable cleanup after caller cancellation; the worker never closes a target. The browser request uses mode:cors, credentials:omit, redirect:error and no-referrer; the response remains unavailable to normal page JS. Each read is <=32KiB, accepted total <=2MiB. Exact URL patterns escape wildcard characters; main-frame/request URL/method/initiator and networkId correlation exclude unrelated requests. Non200, Location and redirect responses fail closed. Original cache-only canvas API remains unchanged; the new API explicitly may issue one public avatar GET. Do not substitute Network.loadNetworkResource: it follows redirects and accumulates body before its IO stream is returned. Existing arbitrary raw Fetch settings in the same CDP session cannot be safely composed with this narrow operation and are not a supported concurrency mode.

### Loaded avatar safety finding (2026-09-30; historical canvas-only phase)

`Page.getResourceContent` is **not approved for cache-only reads**: the dedicated local Chrome 153 identity fixture returned a cached-looking PNG but caused an extra Fetch request for the main document (Network events alone missed it). Do not add a generic resource/file API or treat a successful cache-miss test as cache-hit safety evidence. Identity now uses the existing controlled `TaskPage::evaluate` with a main-frame-only, already-loaded-image canvas probe, retaining &mut lease/cancel/deadline behavior and failing closed on taint. `tests/cached_resource.rs` reuses `tests/common` for fixed-target routing, target retention, ID/URL/document/loading rechecks, payload limits, cancellation and timeout; it does not itself execute DOM or prove zero network. The opt-in dedicated `identity_browser.rs` fixture owns that separate runtime evidence; never point it at a user's browser.
