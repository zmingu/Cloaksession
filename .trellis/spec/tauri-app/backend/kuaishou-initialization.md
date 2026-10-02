# Account initialization concurrency and archive contract

## 1. Scope / Trigger

Applies to `driver/account_init`, its shared identity gate, subject archive UI and launcher-thread persistence. Initialization may navigate or change permissions; identity detection remains read-only. App setup starts identity monitoring and then the initialization monitor, each once. This automatic path was enabled after the user's request and actual Chromix permission transition/persistence verification; native end-to-end acceptance must still be recorded separately.

## 2. Signatures

Public command boundaries remain `kuaishou_init_retry(profile_id: String)`, `kuaishou_subject_list(query: KuaishouSubjectQuery)`, `kuaishou_subject_detail(platform_user_id: String)`, attachment reads by controlled key, and revision-bearing correction/confirmation inputs. See `commands/kuaishou_init.rs` for exact Tauri signatures. New ownership helpers live in `account_init/{lease,staging,cpu}.rs`; they are backend internals, not renderer-controlled capabilities.

## 3. Contracts

- Acquire the shared identity per-profile reservation before fresh initialization identity verification. Automatic/manual admission reserves one of four initialization slots including pending work, then waits at most 20 seconds on a fair per-profile semaphore. Detection remains nonblocking and cannot overtake queued initialization. Cancelled, expired or dropped waiters release their queue position and slot. Hold the reservation for the campaign; it must not consume identity's global detection worker permits. Prune dead weak semaphore references.
- Settings entry readiness is read-only and bounded to eight seconds; accept exactly one visible `修改设置` or `去设置`, then revalidate and click once. Do not retry an ambiguously dispatched click.
- Inside the exact titled drawer, support its explicit close button or the observed header/extra `span.anticon.anticon-system-close-medium-line[role=img]`, rejecting ambiguity/disabled controls. Parse counts from the permission title's immediate parent; `未开启` is valid only with four unchecked target boxes. A full close/navigation/reopen readback remains required.
- Recover a known owned target only after authoritative browser `Target.getTargets` proves it absent with surrounding context checks. Creation completion unknown remains fail-closed; never adopt existing user targets. Slice-only work creates its initial page on the slice route.
- While holding a TaskPage, verify that page directly, inspect other shop targets, then recheck the owned page. Never reacquire its cooperative lease or ignore conflicting user targets.
- Claimed database steps carry RAII ownership through queued responses and workers. Release only the original token through the original launcher connection; lost replies cannot reset a successor or a completed step.
- Staging tracks only new no-clobber publications. Existing content-addressed files are never adopted for cleanup. The writer gate covers publication, commit/reference checks and unlink completion.
- The actual blocking unlink worker owns an `Arc<OwnedMutexGuard<()>>`; cancelling its waiter must not release the gate while deletion continues.
- Local OCR/image admission retries only `OcrError::Busy` within the original deadline. Keep the two-worker resource bound; invalid images and admitted jobs are not blindly retried.
- Archives remain searchable with zero profiles. List results do not prefetch document images or provide a browser launch action for orphan archives.
- Photo requests are component-local promises shared across StrictMode effect replay. Explicit refresh/remount makes a new request; no global document-image cache.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Initialization owns profile gate | Detection skips/returns busy without persisting an identity error |
| Owned or peer page changes account; session/binding changes | Reject mutation/commit; release campaign gate on exit |
| Claim reply or worker is dropped | Original token cleanup survives; startup recovery is fallback if storage is unavailable |
| Cleanup deadline already expired | Do not admit unlink |
| Admitted unlink outlives caller deadline/drop | Worker keeps writer gate until unlink finishes |
| Existing/referenced/changed attachment or uncertain DB state | Preserve file |
| CPU admission Busy | Bounded retry; do not label image corrupt |
| Invalid image/worker failure/deadline | Surface failure, no unbounded retry |

## 5. Good / Base / Bad Cases

Good: an initialization attempt holds a page during OCR while normal identity polling skips internal contention. Base: no initialization reservation permits normal read-only detection. Bad: persisting Error because the application itself holds the page lock invalidates an otherwise valid attempt.

Good: a timed-out cleanup waiter leaves its admitted unlink worker holding the gate. Bad: releasing the gate lets a successor reference the file before the old unlink completes.

## 6. Tests Required

- Identity monitor loopback-peer regression: held TaskPage does not cause persisted Error; peer and owned-page account changes still reject writes; reservations release without consuming global permits.
- Unlink barrier regression: expired admission does not dispatch; timed-out/dropped waiters cannot admit successors until actual unlink completes.
- Lease tests: lost queued/delivered claim replies and release replies cannot revoke successors or completed steps.
- Staging tests: new-unreferenced cleanup only, existing/changed/referenced files survive, cancellation during publication retains ownership.
- CPU Busy/error/deadline/cancellation and concurrent real-image decode tests.
- Local Playwright: zero-profile search/copy/pagination, no photo prefetch, invalid/stale confirmation rejection, photo reads counted once per mount and freshly on reopen.

## 7. Wrong vs Correct

Wrong: wrap `tokio::fs::remove_file` in a timeout and drop the writer lock with the timed-out waiter.

Correct: check the deadline before admission, move a writer-lock owner into the blocking unlink worker, and retain ownership until actual filesystem completion even when the caller stops waiting.

Wrong: re-run attachment IPC on every StrictMode effect setup or globally cache sensitive photo data.

Correct: share the component-local request promise across replay, subscribe anew, and discard it on actual unmount.
