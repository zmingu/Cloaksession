# Guideline Bootstrap Execution Plan

## Planning gate

- [x] Inspect the existing task, package roots, workspace manifest, docs, representative implementations/tests, and current CI.
- [x] Persist evidence, requirements, design, and execution plan.
- [x] Curate implement/check context manifests with populated shared guidance and research.
- [x] User approves the final planning summary (confirmed by implementation dispatch).
- [x] Run `python ./.trellis/scripts/task.py start 00-bootstrap-guidelines` and confirm `task.py current --source` resolves to this task with status `in_progress` (activation preceded implementation; rechecked during verification).

Do not begin the steps below before the planning gate is complete.

## Implementation

- [x] Record a before-edit inventory and retain a task-local snapshot of the eight ignored spec roots; do not rely on Git for rollback.
- [x] Read relevant source bodies/tests for each package, following the evidence map. Additional findings/evidence are recorded in owner guides and check-results.md.
- [x] Fill `multizen-core` guidance and real examples: shared model/serde contracts, defaults, errors.
- [x] Fill `profile-manager` guidance and real examples: DB ownership, migrations/row mapping, CRUD, directories, tests.
- [x] Fill `settings-store` guidance and real examples: cached file persistence, normalization, update/load differences.
- [x] Fill `behavioral` guidance and real examples: pure seeded generators and deterministic tests.
- [x] Fill `browser-launcher` guidance and real examples: engine boundaries, lifecycle, proxy bridge, launch parameters, Chromix resources.
- [x] Fill `cdp-driver` guidance and real examples: sessions/tools/bootstrap, raw-CDP and safety limitations.
- [x] Fill `mcp-server` guidance and real examples: metadata/schema/dispatch/driver boundaries, actual HTTP/security/activity behavior.
- [x] Fill `tauri-app` guidance and real examples: launcher thread/state, IPC/events, embedded MCP, archive/extensions/update ownership.
- [x] Remove/merge unused topic scaffolding, update indexes and both checklists, and verify references between package owners/consumers.

## Verification

- [x] Add and run a small dependency-free task-local `check_specs.py`; cover all eight package roots, required indexes/checklist sections, template markers, and local file/link references. Keep it limited to this documentation task.
- [x] Manually cross-check important claims and quoted snippets against implementation/tests; record evidence and any limitations in `check-results.md`.
- [x] Verify index links cover the final page set and that no package was omitted.
- [x] Verify commands against Cargo/npm manifests and CI; distinguish unit checks from optional browser/system-dependent integration checks.
- [x] Compare source/control-file changes to the starting clean Git state and review the separate ignored-spec inventory.
- [x] Perform an independent full-scope check pass, fix findings, and rerun the document check (see check-results.md).

Commands from the repository root:

```bash
python ./.trellis/scripts/get_context.py --mode packages
python ./.trellis/scripts/task.py validate 00-bootstrap-guidelines
python ./.trellis/tasks/00-bootstrap-guidelines/check_specs.py
git status --short
git diff --check
```

The last two commands cover tracked/unignored collateral changes only. Explicitly inspect ignored task/spec paths; do not call an empty Git diff a complete validation.

This task changes documentation only. Cargo tests, UI builds, runtime npm tests, Playwright, and real-browser tests are not execution gates here and must not be reported as run. Their real commands/prerequisites are content to document for future development.

## Completion and rollback gates

- [x] Update acceptance checkboxes only after corresponding checks pass (R6 passed independent review).
- [x] Perform the Phase 3.3 spec-update review; avoid redundant additional docs when the bootstrap itself already captures the learning.
- [x] Report the completed local-only changes and verification results in the main-session handoff. No commit, force-add, ignore-rule change, or archival; task remains in_progress pending separate user authorization.

If a source/doc conflict cannot be resolved by inspection, record the limitation rather than inventing a guarantee. If product changes become necessary, stop and return to planning. For rollback, restore the task-local pre-edit snapshot selectively; never use blanket Git reset/clean on the working tree.
