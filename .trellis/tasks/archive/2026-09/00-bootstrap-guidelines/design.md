# Guideline Bootstrap Design

## Change boundary

The gap is documentation: eight backend spec roots contain templates rather than local conventions. Runtime behavior remains unchanged. Edit only this task's artifacts and `.trellis/spec/` package guidance; preserve Trellis workflow/configuration, product source, dependencies, and Git policy.

## Organization

Keep the eight existing package/backend roots configured in `.trellis/config.yaml`. Each index is a short entry point with package ownership, linked guides, a Pre-Development Checklist, and a Quality Check section.

Choose topic pages from real code rather than retaining the fixed six-file scaffold. Keep database guidance where SQLite is actually owned; describe settings JSON as file persistence, not an ORM. Merge small overlapping topics and remove empty/inapplicable template pages. Avoid duplicating shared model contracts across eight packages: cite the owner and link consumers to it.

This remains one integrated bootstrap task, not eight new child tasks. The package boundaries organize source inspection and docs; a single ownership pass plus full-scope review provides consistent guidance without eight independent planning/commit workflows.

## Evidence and contracts

Use `research/repository-baseline.md` as the initial source map. Before writing each guide, read its relevant implementations and tests. Source/test behavior wins over outdated README text and comments. Use repository-relative paths and symbol names rather than fragile line-only references; use relative Markdown links for spec navigation.

Document contracts at their owner and verify the consumers:

1. Shared Rust model/serde definitions → profile/settings serialization and UI payloads.
2. Profile persistence → engine-specific data directories, launch lifecycle and settings.
3. Browser launcher → legacy processes/Chromix sidecar → CDP connection and page operations.
4. MCP metadata/schema/dispatch → BrowserDriver implementation shared with Tauri IPC.
5. Tauri startup/state → commands, push events, embedded MCP and resource integration.

Do not assert guarantees merely because a type/helper exists. Examples already requiring care include incomplete Safe-CDP interception, unimplemented SSE, startup-only settings behavior, and differences between engine fingerprint paths.

## Execution ownership and review

After the user's planning approval and task activation, one implement sub-agent owns the source inspection and spec-writing pass. A check sub-agent then verifies all eight roots and the cross-package contracts. No recursive agent dispatch. Use real spec/research entries in both JSONL manifests; do not inject the empty package templates as authoritative rules.

## Validation and compatibility

Because only ignored local Markdown/task artifacts change, `git diff` is not sufficient evidence. Record a before/after file inventory, run a small task-local Python document check, and manually review source claims/code examples. The checker should cover all eight roots, nonempty files, residual template markers, local links/source paths, and required index sections. No new dependency or product test framework is needed.

Product check/test commands are documented from the repository but are not bootstrap acceptance checks. No full Cargo/UI build or real-browser/network test is required for a docs-only change. If scope unexpectedly requires source changes, stop and return to planning rather than silently broadening work.

## Rollback and operational controls

All edits are local; `.trellis/` is ignored by `.gitignore`. Preserve pre-edit package templates in a task-local snapshot before replacing/deleting them so rollback does not depend on Git. Restore only this task's edits if needed, preserving any user changes made afterward. Do not modify `.gitignore`, force-stage `.trellis`, or change session auto-commit configuration.

Planning history: the generated `in_progress` status was returned to `planning` because required artifacts and the review gate were absent. The user subsequently approved the summary and task activation succeeded before implementation. The independent verification subsequently passed (see check-results.md). The existing task remains `in_progress` for the main-session handoff, without completion or archive action; no new task is needed.
