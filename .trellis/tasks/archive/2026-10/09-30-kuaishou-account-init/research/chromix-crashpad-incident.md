# Chromix Crashpad live incident — 2026-10-02

## User report

`Crashpad_NotConnectedToHandler` during real Chromix operation. Treat this as a browser/page crash incident, not the earlier selector mismatch or a proven reason to disable crash reporting.

## Reproduction and evidence (parent executed)

- Actual installed application-launched Chromix `Chrome/151.0.7922.173`.
- Native accessibility tree shows the selected slice tab as crashed and the exact user-reported error code.
- Raw page CDP evaluation on that slice target times out (2.5–3s), while the same browser's shop home and ordinary start page still respond.
- One native Reload of the same crashed tab reproduces the exact code again; no product-created tab or new initialization invocation was required for that manual control. A renderer PID changed after reload. Browser main process survived.
- App phase log shows successful owned-create/acquire, then a 20s slice navigation timeout; another creation timeout; later successful first navigation/open/read/close followed by a 20s persistence navigation timeout. These are correlated phase observations, not causal proof.
- Browser launch includes `--disable-breakpad`, `--no-sandbox`, `--disable-dev-shm-usage`; no separate crashpad-handler child was observed in the process snapshot. These are observations only; do not remove/add flags without a controlled test.
- No matching Application Error 1000 event was available in the queried two-hour window. No `chrome_debug.log`/crash-named files were located under this profile directory by the bounded search. No cookies, credentials, subject values or dumps were read.

## Competing hypotheses

1. Crash-handler initialization/lifetime under the actual launcher/default flags. Prediction: the same controlled navigation differs with one relevant flag or launcher-lifetime change in a disposable profile.
2. Renderer creation or navigation sequencing. Prediction: controlled reload/new-target patterns differ with otherwise identical actual Chromix launch settings.
3. Page-specific renderer failure unrelated to handler setup. Prediction: local synthetic pages remain healthy while the affected route reproducibly fails; crash reporting may only describe failure handling.

A manual reload of the same existing target reproducing the failure means this cannot yet be blamed solely on automatic task-page creation.

## Launch-stack baseline result

The parent syntax-checked and ran the disposable `crashpad-lifecycle-smoke.mjs` using the real production bridge and installed Chromix. `bridge-defaults` reported `disableBreakpad=true`, `noSandbox=true`, headed mode and Playwright pipe transport. Eight same-target navigation/reload cycles plus eight new-target cycles completed: exit 0, `NOT_REPRODUCED_IN_SYNTHETIC_FIXTURE`.

This negative synthetic result is not a fix and does not invalidate the live crash. It prevents attributing the failure to those flags alone. The tested documents stayed on one site; fresh renderer births were not established by that baseline. A bounded cross-site process-lifecycle comparison is the next experiment, without changing production switches.

A finite live crash-event watcher later refused to attach because the originally identified target was absent/changed. No new crash event or numeric exit code was captured; disappearance is not proof of recovery.

## Follow-up results: still no proven fix

- Four-cycle `bridge-defaults --scenario=cross-site`: exit 0, `NOT_REPRODUCED_IN_SYNTHETIC_FIXTURE`. Actual renderer additions/removals were observed at each synthetic slice/site-B transition, target absence was proved before replacement, and the home control remained responsive. No production flags changed.
- The original live target later pointed to qualification rather than slice and no longer exposed the observed error in the current accessibility snapshot. This is changed state, not proof of a repair.
- One new diagnostic-owned tab in the same actual application-launched Chromix loaded the real slice route successfully, with the settings entry visible and no observed crash. The script closed only its own successful diagnostic target. It did not click permissions, read subject data, reset the profile or reload existing user pages.
- `crashpad-lifecycle-smoke.mjs` is preserved under `crates/tauri-app/resources/chromix/test/`, outside the normal `*.test.mjs` test glob. Live diagnostic scripts remain session-bound under this task's research directory; their recorded ports/targets must be revalidated before reuse.

The earlier exact live crash/reload reproduction remains valid. These later negative results do not establish the root cause, fix the incident, or certify automatic initialization. Further diagnosis needs another failing live occurrence with crash-event capture attached before failure; stop speculative parameter changes meanwhile.

## Boundaries

Preserve the actual profile and other user tabs. No data deletion, browser reinstall, new engine substitution, crash-reporting workaround, or broad process termination. Chromix is the acceptance authority. Implementation agent is preparing a narrow disposable reproduction using the actual launch bridge; no production fix accepted yet.
