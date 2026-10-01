# Bootstrap Project Development Guidelines

## Goal

Replace the initial Trellis backend spec templates with concise, source-backed development guidance so future work follows Cloaksession's actual architecture, contracts, and tests instead of generic conventions.

## Background

The existing bootstrap task targets the eight Rust workspace crates. Their backend spec indexes and topic files are unfilled templates. Shared thinking guides already contain guidance. Existing READMEs provide useful context but some claims lag behind implementations and current CI.

## Requirements

- **R1 — Complete package coverage:** Populate backend guidance for `multizen-core`, `profile-manager`, `settings-store`, `behavioral`, `browser-launcher`, `cdp-driver`, `mcp-server`, and `tauri-app` under their existing `.trellis/spec/<package>/backend/` roots.
- **R2 — Evidence-based content:** Describe actual ownership, key contracts, error handling, persistence/logging where applicable, and representative tests. Important rules must cite existing repository files and relevant symbols. Include a short real code example for each package; support repeated conventions with multiple local examples where available. Clearly distinguish current limitations from guarantees.
- **R3 — Useful navigation:** Each package index must link its final guide set and include a Pre-Development Checklist and Quality Check section. Remove or merge irrelevant template topics rather than inventing a database/ORM or logging policy for every crate.
- **R4 — Cross-package consistency:** Explain the shared model, persistence, launch, CDP, MCP, and Tauri boundaries consistently. Read React IPC/types and Chromix runtime resources as needed to verify backend contracts, without expanding into a separate frontend spec project.
- **R5 — Existing documentation conventions:** Write final specs in English, matching the current spec indexes. Preserve already-populated shared thinking guides unless source inspection reveals a concrete contradiction.
- **R6 — Accurate verification:** Record applicable commands and their prerequisites from actual manifests/CI. Check the new docs for template text, broken local references, index/file-set mismatches, and unsupported claims. Do not report historical, skipped, or unrun build/browser checks as passed.
- **R7 — Local-only operational boundary:** Keep work in the existing task and spec directories. `.trellis/` is currently ignored by Git; do not change ignore rules, force-add files, install dependencies, launch browsers, or automatically commit/archive as part of this request.

## Acceptance Criteria

- [x] All eight package roots contain usable project-specific guidance (R1).
- [x] Each package has at least one real code example and source/test references; important claims match implementation rather than stale prose (R2).
- [x] All package indexes match their files and include both required checklists; no unfilled template headings or placeholder instructions remain in the package specs (R3).
- [x] Shared types/units, persistence ownership, engine-specific launch behavior, CDP limitations, MCP transport/security boundaries, and Tauri IPC/event integration are documented without contradictions (R4).
- [x] Specs are English; existing useful shared guides are preserved (R5).
- [x] A recorded full-scope document/source review verifies links, source paths, navigation, and placeholder removal. Development commands are real and prerequisites are explicit (R6).
- [x] No product source, dependency, lockfile, runtime configuration, or Git policy change is made (R7).

Implementation self-review and the separate independent full-scope check are recorded in `check-results.md`; R6 passed after source review, four documentation corrections and rerun validation. Task status remains in_progress; no completion/archive action was taken.

## Out of Scope

- Fixing product bugs, changing behavior, upgrading dependencies, or refactoring source.
- Rewriting root/crate READMEs or historical acceptance evidence.
- Creating a dedicated React/frontend guideline layer or documenting vendored SDK internals as project-owned code.
- Full build, network/browser integration testing, UI redesign, publishing, Git tracking changes, commits, and task archival without separate approval.
