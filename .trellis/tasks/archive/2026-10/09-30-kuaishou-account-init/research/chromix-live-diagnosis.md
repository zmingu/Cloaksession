# Chromix live diagnosis — 2026-10-02

## Acceptance authority and authorization

User reported local OCR apparently successful, slice permissions unsuccessful, missing automatic collection and intermittent page-opening errors. User explicitly requires actual Chromix operation, not Edge/Chrome substitutes. The automatic monitor remains disabled pending verified flow; that explains the absent automatic trigger, not a successful automatic implementation.

## Actual environment

Installed Chromix 151.0.7922.173. Application was restarted after only a hidden helper window remained. Launched the user's single test profile through the actual development UI. UI showed subject collection done, slice incomplete/interrupted. No document values/images were exported into this record.

A diagnostic-owned slice tab was opened in that application-launched Chromix process. The fixed production identity extractor and account-init page adapter were evaluated in the actual platform page. Script `chromix-live-slice-readonly.mjs` is session-bound diagnostic material; its endpoint is ephemeral, not a reusable application API. Do not run against a recycled port without revalidating process ownership/version.

## Observations

- Production `open` succeeds once the settings entry is ready.
- Immediate `slice` read after opening rejects; subsequent read with mounted drawer succeeds. All four target checkbox states are enabled at this observation. No permission was toggled by these probes.
- Actual titled drawer has zero matching close buttons for the product selector.
- Its visible close control is a unique `SPAN.anticon.anticon-system-close-medium-line[role=img]` inside `DIV.kwaishop-tianhe-shortVideoB-pc-drawer-extra`, itself inside `DIV.kwaishop-tianhe-shortVideoB-pc-drawer-header`. This structural observation contains no account/subject data.

## Chromix production-path RED fixture

`RUN_ACCOUNT_INIT_CHROMIX_FIXTURE=1 cargo test -p tauri-app --locked --lib driver::account_init::page::chromix_entry_fixture::chromix_delayed_slice_entry_after_ready_identity -- --ignored --exact --nocapture`

Parent ran in the implementation worktree with installed Chromix: immediate-entry control passed (64ms); delayed-entry attempt rejected in 2ms; entry became ready after 1502ms; late-entry control passed (7ms); explicit H1 RED failed, 0 passed/1 failed, 2.01s. This establishes missing settings readiness handling at the production seam, not merely hypothetical page delay.

## Separate non-authoritative-for-slice observation

Full pointer fixture also times out on hidden-page mouse input in installed Chromix. The application slice adapter uses DOM clicks through evaluate, not that pointer path. Do not attribute slice failure to pointer failure without additional evidence.

## Actual permission-write and persistence evidence

Under the user's request to test the failed functionality on their actual Chromix account, the diagnostic used the production adapter to close only still-enabled target permissions. No other controls were clicked and no target permission was re-enabled.

The first three off/read pairs succeeded. After the fourth off, all four boxes were unchecked but the adapter rejected the read. Exact header text became `全自动发布权限\n未开启`, which the numeric-only `开启 n/4` parser rejects. This is a confirmed zero-state false failure.

The repaired close adapter closed the observed header icon successfully. A full `Page.reload` followed. On the refreshed document the entry changed from `修改设置` to `去设置`. The diagnostic opened that unique scoped entry; fresh DOM then showed all four boxes unchecked, header `未开启`, no visible confirmation dialogs. This verifies persistent closure for this tested account. It does not yet prove the product's entire automatic campaign or database completion path.

The earlier Chromix H1/close regression was rerun after the readiness/icon fix and passed (exit 0). Zero-header and renamed-entry handling plus known-closed-owned-page recovery are now in progress. No final automatic acceptance claimed.

## Next

Support exact zero-state header and the two observed entry labels with strict uniqueness checks; rerun Chromix regressions and the product's already-off full slice/readback path. Complete proven-missing owned-page recovery without adopting user pages or treating unknown creation completion as safe. Then enable and verify the user-requested automatic flow through the actual app, retaining all identity/session guards.
