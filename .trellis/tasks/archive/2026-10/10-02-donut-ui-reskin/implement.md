# Implement — Donut-style UI reskin and sidebar layout

## Execution principles

- **Do the risky layer last.** Tokens + material + font first (reversible CSS), sidebar schema change last. This way a context-drop or interruption leaves a working app at every checkpoint.
- **No visual checkpoint without all three token files done.** Half-migrated purple→token looks worse than baseline; finish R1–R3 in one pass before running the dev server to look.

## Ordered checklist

### Phase 0 — Vendor fonts (R5, prerequisite for everything visual)

- [ ] **0.1** Install `geist` npm package as devDep in `crates/tauri-app/ui`.
- [ ] **0.2** Locate the WOFF2 files inside the package; copy Geist Sans (400/500/600/700) and Geist Mono (400/500/600) to `crates/tauri-app/ui/public/fonts/`.
- [ ] **0.3** Add `@font-face` blocks to `styles.css`.
- [ ] **0.4** Remove the Google Fonts `<link>` and preconnect from `index.html:7-12`.
- [ ] **0.5** Verify in `vite dev` that body text renders in Geist Sans (DevTools computed font-family).

### Phase 1 — Token system (R1, R2)

- [ ] **1.1** Add `@theme inline` block to `styles.css` mapping all Donut Mono tokens (table in `design.md`).
- [ ] **1.2** Add `:root` and `.dark` blocks; apply `.dark` class to `<html>` in `index.html` (or `main.tsx`).
- [ ] **1.3** Migrate `styles.css` purple usages: focus ring (line 24), selection (line 29), `.mz-button:hover[accent]` (line 118), `.btn-brand` (lines 136-148).
- [ ] **1.4** Migrate `LeftRail.tsx` purple inline styles (lines 47, 50-51).
- [ ] **1.5** Migrate `Constellation.tsx` purple tag chips (lines 202, 205-208) and tile-state dots (lines 30-34) to semantic tokens.
- [ ] **1.6** Grep-sweep: `Grep "rgba\(168"`, `Grep "text-purple-`, `Grep "#a855f7|#6366f1|#ec4899"` across `crates/tauri-app/ui/src` — confirm zero hits.

### Phase 2 — Material + scroll-fade (R3)

- [ ] **2.1** Port `surface-material*` classes and `prefers-reduced-transparency` fallback to `styles.css`.
- [ ] **2.2** Port `scroll-fade` / `scroll-fade-x` utilities.
- [ ] **2.3** Create `lib/useScrollFade.ts` (writes `data-fade-top`/`data-fade-bottom` on scroll).
- [ ] **2.4** Apply `surface-material` to Modal, TopBar, command palette, toast.
- [ ] **2.5** Apply `scroll-fade` to Constellation scroll body, MCP activity log, settings scroll.
- [ ] **2.6** Verify under `prefers-reduced-transparency: reduce` (DevTools rendering emulation) — no broken layout.

### Phase 3 — Sidebar IA (R4) — risky, do last

- [ ] **3.1** Rust: add `group: Option<String>` to `Profile` in `crates/multizen-core/src/profile.rs`; update `ProfileSummary`.
- [ ] **3.2** SQLite migration in `crates/profile-manager` adding `group TEXT NULL`; default existing rows NULL.
- [ ] **3.3** IPC commands: `list_groups`, `set_profile_group`, `delete_group` in `crates/tauri-app/src/commands/`; register in `Driver`.
- [ ] **3.4** TS types: add `group` to `Profile`/`ProfileSummary` in `types.ts`.
- [ ] **3.5** New `Sidebar.tsx` (labeled, ~220px, collapsible).
- [ ] **3.6** Group tabs row above Constellation grid; wire to a new `group` filter prop alongside the existing `activeTag`.
- [ ] **3.7** Swap `LeftRail` → `Sidebar` in `App.tsx:367`; update keyboard shortcuts if needed.
- [ ] **3.8** Update `AppSettings.theme` default to `dark` if the existing theme field gates `.dark` class.

### Phase 4 — Reduced motion (R6)

- [ ] **4.1** Port `prefers-reduced-motion` block.
- [ ] **4.2** Verify modal-pop and slide-up animations collapse to instant.

## Validation commands

Run from repo root after each phase, and all together before `task.py start`:

```powershell
# Frontend type-check + build
npm --prefix crates/tauri-app/ui run build
# Rust check + tests
cargo check --workspace
cargo test --workspace
# Grep guards for purple leakage
# (run via Grep tool, not shell)
```

## Risky files / rollback points

| File | Risk | Rollback |
| --- | --- | --- |
| `crates/multizen-core/src/profile.rs` | Schema change | git revert + down migration |
| `crates/profile-manager/migrations/` | Migration irreversibility on prod data | Add down migration that drops `group` col |
| `styles.css` | Token layer affects every screen | git revert single file |
| `App.tsx:367` | Shell swap | restore `LeftRail` import |
| `index.html:7-12` | Font CDN removal | restore `<link>` tags |

## Pre-`task.py start` checks

- [ ] `prd.md` converged (no duplicate facts, no resolved open questions lingering).
- [ ] All AC mapped to a phase checklist item.
- [ ] `cargo check --workspace` passes on `main` before starting (baseline green).
- [ ] Font WOFF2 files committed; `index.html` no longer references `fonts.googleapis.com`.
- [ ] Migration has an up *and* down path.

## Follow-up after implementation

- Run `trellis-check` skill for spec compliance.
- Manual browser smoke: launch a profile, open edit modal, scroll the grid, toggle group tabs, exercise ⌘K/⌘N/⌘1/⌘2/⌘,/⌘⇧A.
- Update `crates/tauri-app/ui/src/components/` spec docs if any new conventions emerged.