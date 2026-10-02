# Resume results — 2026-10-01

## Status

The user explicitly requested continuing both unfinished tasks. The missing account-init task.json was restored and this existing task activated. Historical planning headings are not current implementation status. Changes below are integrated into the primary working tree; no commit/archive occurred. Automatic initialization remains disabled.

## Delivered

- Persisted-backoff bounded manual retries with account/session budgets and owned-page cleanup recovery.
- Shared per-profile identity reservation before fresh verification; direct held-page reads plus peer conflict checks prevent polling from invalidating initialization through internal lease contention.
- RAII claimed-step ownership survives abandoned replies/workers; token-specific release cannot revoke a successor.
- Bounded image/OCR Busy admission retry, without relaxing resource bounds or retrying invalid images.
- New-file-only attachment staging cleanup with fresh reference checks. Actual admitted unlink workers retain the writer gate despite caller timeout/drop.
- Exact route checks for subject/slice operations.
- Searchable paginated orphan archives even with zero browser profiles; no list photo prefetch or orphan launch actions.
- Local photo failure/refresh/reopen behavior with StrictMode request deduplication.
- Regression tests across Rust lifecycle, identity protocol peer, local DOM adapters and desktop/mobile Playwright.

## Fresh validation

Coordinator ran in the primary working tree after integration:

- `cargo check --workspace --locked`: exit 0.
- `cargo test --workspace --locked`: exit 0.
- `npm.cmd --prefix crates/tauri-app/ui run build`: exit 0.
- `PLAYWRIGHT_CHANNEL=msedge` with `npm.cmd --prefix crates/tauri-app/ui test`: exit 0, full suite (94 tests scheduled). Browser tests used local mocks/fixtures, not real accounts.

An independent static full-scope review identified two P1 races: self-contention between identity polling and initialization, and unlink outliving the writer gate. Both were fixed with regressions. Independent re-review of the integrated primary files confirmed both fixed and established no new blocking defect in those changed seams. Reviewer did not execute tests; runtime evidence above belongs to coordinator runs.

Earlier failed rounds are not hidden: three staging tests exposed Busy misclassified as image decode failure; the first UI round exposed duplicate StrictMode photo requests; additional concurrency-test compilation initially lacked a dev dependency. These were corrected and the subsequent gates above passed.

## Remaining acceptance gates — task is not complete

- Real-account document-image reading/quality and native UI/OCR end-to-end acceptance remain unverified in this continuation.
- Durable slice permission-save semantics remain unverified on a real account; no platform permission changes were performed.
- Automatic monitoring is deliberately off pending those gates, so the automatic-trigger PRD acceptance is not satisfied.
- The separate CDP task documents hidden-page mouse dispatch failure on installed Edge, including independent raw-WebSocket reproduction. No implicit activation workaround was added.

Do not archive or claim AC1–AC9 fully satisfied from local tests alone. Next action requiring user input is bounded real-account/native acceptance authorization; do not silently enable the monitor.

## Durable contract

See `.trellis/spec/tauri-app/backend/kuaishou-initialization.md` for ownership, cancellation, error and regression contracts.
