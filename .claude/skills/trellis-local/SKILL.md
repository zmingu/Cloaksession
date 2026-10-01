---
name: trellis-local
description: Project-local routing record for Matt/Waza skills bound into this repo's Trellis workflow, plus their verified upstream sources. Read when you need to know which third-party skills are wired in here, at what revision, and with what ownership boundary. Does not execute skills or duplicate their text.
---

# Trellis Local Routing Record

Bound Matt/Waza integrations for this repo. Skill sources live under `.agents/skills/<name>/SKILL.md` (project-local copies). This file records routing, phase, artifact, gate, and upstream revision only.

## Bound routes

| Skill | Owner | Mode | Phase | Trigger | Artifact | Gate |
|---|---|---|---|---|---|---|
| `grilling` | matt | automatic | planning | stress-test a plan/decision | `prd.md`/`design.md` | before `task.py start`; deep Phase 1.1 |
| `grill-with-docs` | matt | explicit | planning | deep interview + GLOSSARY/ADR | `prd.md`/`design.md`/`GLOSSARY.md` | before `task.py start` |
| `tdd` | matt | automatic | implementation | test-first feature/bug work | `implement.md` | `trellis-check` before commit |
| `diagnosing-bugs` | matt | automatic | implementation | hard bug/regression | `research/` | reproduce and confirm root cause before repair |
| `ui` | waza | automatic | implementation | build/restyle UI | `design.md` | visual decision returns to `design.md` |
| `code-review` | matt | automatic | review | high-risk/pre-merge review | `review-record.md` | after `trellis-check`, before Phase 3.4 commit |

Review order: `trellis-check` -> `code-review` -> `trellis-update-spec` -> commit. `/code-review` never replaces `trellis-check`.

## Verified upstream sources (read-only, checked 2026-10-01)

- **Matt Pocock** — `github.com/mattpocock/skills`, branch `main`, 493 commits, no version tag. Routed: `grilling`, `grill-with-docs`, `tdd`, `diagnosing-bugs`, `code-review`. Installed as referenced dependencies (not routed): `domain-modeling` (called by `grill-with-docs`), `codebase-design` (consulted by `tdd`). `grill-with-docs` is the only explicit/user-invoked route (`disable-model-invocation: true`); the rest are model-invoked (automatic).
- **Waza** — `github.com/tw93/Waza`, branch `main`. `VERSION` file exists but its content was not web-rendered, so the **version number is UNVERIFIED**. Installed: `skills/ui/SKILL.md`. Frontmatter has `when_to_use` / `dispatch_intent` (intent-dispatched); no `disable-model-invocation`.

Revalidation: re-check upstream on every refresh; names, paths, and invocation metadata drift.

## Ownership boundaries (must not cross)

- **Trellis owns**: task lifecycle, package/spec scope, planning artifacts, acceptance evidence, check, spec updates, finish.
- **Matt skills own**: decision shaping, TDD loop, diagnosis discipline, independent review. Must not replace Trellis task state, package selection, or `trellis-check`.
- **Waza `ui` owns**: UI/visual iteration. Visual decisions return to `design.md`; they do not bypass it.
- **This record owns**: routing/phase/hook/artifact evidence. It does not execute any user-invoked or model-invoked skill.

## Runtime proof

- Manifest: `.trellis/skill-integration.json`
- Hook: `UserPromptSubmit -> python .claude/hooks/inject-workflow-state.py` (configured in `.claude/settings.json`)
- Workflow route table: `### Skill Routing` in `.trellis/workflow.md`; phase binding in `[workflow-state:in_progress]`
- Smoke command:
  ```powershell
  python C:/Users/Administrator/.claude/skills/trellis-workflow-enhancer/scripts/verify_integration.py F:/Cloaksession --smoke
  ```
