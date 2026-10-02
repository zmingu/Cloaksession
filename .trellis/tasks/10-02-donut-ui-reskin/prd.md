# PRD — Donut-style UI reskin and sidebar layout

## Goal

Bring Cloaksession's management UI visual style and layout in line with [zhom/donutbrowser](https://github.com/zhom/donutbrowser): a monochrome design-token system, transluscent `surface-material`, CSS-mask scroll-fade, and a labeled sidebar with profile groups — replacing the current 56px icon rail, hardcoded purple brand color, and Space Grotesk typography.

The user wants the *look and feel* of Donut, not a feature-parity clone. Cloaksession's existing IA and feature set stay; only the visual layer and the sidebar IA change.

## Background

### Reference source (Donut Browser)

- Stack: Next.js + Tauri + Tailwind v4 + shadcn + Geist Sans/Mono + Sonner.
- Visual identity: **"Donut Mono" single-color palette** — dark `--background: #070707`, `--primary: #ffffff` (inverted white-on-black buttons), hairline border `rgba(255,255,255,0.06)`, semantic colors coral `#ec6a5e` / grass `#61c554` / amber `#f4be4f`. No brand hue.
- Material: `surface-material` = `color-mix(in oklab, var(--background) 85%, transparent)` + `backdrop-filter: blur(20px) saturate(1.4)` for modals/popovers/cards; solid fallback under `prefers-reduced-transparency`.
- Scroll: CSS `mask-image` edge fade (`scroll-fade` / `scroll-fade-x`) — explicitly *not* colored gradient overlays (overlay leaves visible bands over content); thin transparent scrollbars (`scrollbar-width: thin`).
- Theme switch via View Transitions API (200ms cross-fade); `prefers-reduced-motion` honored.
- Layout: shadcn sidebar mode (`sidebar-*` tokens, labeled items, group tabs).

### Cloaksession current state

- Stack matches the parts that matter: **Tailwind v4 + Tauri 2 + Vite** (not Next.js — irrelevant to styling), React 19, self-rolled atoms (`Button.tsx`, `Modal.tsx`, `Pill.tsx`, `Avatar.tsx`).
- **No design-token system**: `styles.css` has only `--font-sans` / `--font-mono` in `@theme`; all colors are hardcoded rgba inlined per-component (e.g. `LeftRail.tsx:32,50-51`, `TopBar.tsx:22,45-46`, `Constellation.tsx:108-112,172-173,205-208`).
- **Purple brand color everywhere**: `.btn-brand` gradient `#6366f1→#a855f7→#ec4899` (`styles.css:136`), focus ring `rgba(168,85,247,0.4)` (`styles.css:24`), selection `rgba(168,85,247,0.5)` (`styles.css:29`), `text-purple-300` active states (`LeftRail.tsx:47`, `Constellation.tsx:202`).
- **Fonts loaded via Google Fonts CDN** in `index.html:7-12`: Space Grotesk + JetBrains Mono.
- **LeftRail is a 56px icon-only rail** (`LeftRail.tsx:31,33`) with three items (Profiles/MCP/Settings) and a ⌘K button. No labels, no groups.
- **Profile data model has no `group` field**: `Profile` and `ProfileSummary` in `crates/multizen-core/src/profile.rs` (TS mirror `types.ts:121-151`) carry `tags: string[]` only. Donut's "profile groups" tabs require a new field and a SQLite migration.
- Existing atoms (`.mz-card`, `.mz-pill`, `.mz-button`) are re-skinnable in place; **shadcn adoption is explicitly out of scope (chose tier B, not C)**.

## Requirements

### R1 — Design-token system

Introduce a Donut-style CSS custom-property token layer in `styles.css` covering background, foreground, card, popover, primary (+foreground/text), secondary, muted(+foreground), accent(+foreground), destructive(+foreground/text), success(+foreground/text), warning(+foreground/text), border, input, ring, chart-1..5, and the `sidebar-*` family. Use the **Donut Mono dark values verbatim** as the sole theme (light theme is **out of scope** for this task — Donut itself ships dark-only as the redesign). Wire them through Tailwind v4 `@theme inline` so `bg-background`, `text-foreground`, `border-border`, `bg-primary` etc. work as utilities.

### R2 — Monochrome palette replacement

Replace every hardcoded purple brand usage with the new token system: `.btn-brand` becomes a `--primary`-driven inverted button (white bg / black text), focus ring switches to `--ring`, selection to a neutral alpha, all `text-purple-300` / `rgba(168,85,247,…)` inline styles migrate to tokens. Semantic colors (coral/grass/amber) replace the current ad-hoc `#34d399`/`#c084fc`/`#f87171`/`#94a3b8` tile-state dots.

### R3 — surface-material + scroll-fade

Port `surface-material`, `surface-material-popover`, `surface-material-card` (with `prefers-reduced-transparency` fallback) and the `scroll-fade` / `scroll-fade-x` mask utilities from Donut's `globals.css`. Apply surface-material to `Modal`, `TopBar`, toast, and floating chrome; apply scroll-fade to the Constellation scroll body and any long lists.

### R4 — Labeled sidebar with profile groups

Replace the 56px icon rail with a labeled sidebar (Donut shadcn-sidebar pattern): show icon + label for Profiles/MCP/Settings, and add **profile group tabs** above the profiles grid. This requires:
- New `group: string | null` field on `Profile` and `ProfileSummary` (Rust `crates/multizen-core/src/profile.rs` + TS `types.ts`).
- SQLite migration in `crates/profile-manager` adding the column.
- IPC commands for create/rename/delete group + list profiles-by-group.
- Sidebar UI rendering groups and a fallback "All / Ungrouped" tab.
- Group selection filters the Constellation grid.

### R5 — Geist font (match Donut)

Replace Space Grotesk + JetBrains Mono with **Geist Sans** (UI body) + **Geist Mono** (numbers/code/IDs/timestamps) — exactly what Donut uses. Geist is Vercel's open-source typeface (OFL-1.1 / SIL OFL), available as WOFF2 and as an npm package (`geist`), so it can be bundled locally via `@font-face` for offline use (Cloaksession ships offline; no runtime CDN fetch). Source: [vercel/geist-font](https://github.com/vercel/geist-font) or the `geist` npm package.

This follows the user's decision: "按照 donut 用的来" — adopt Donut's typography as-is. The earlier Maple Mono candidate is dropped; the monospace-font-as-UI-body question (former Q1) is resolved by using Geist Sans as the proportional body and Geist Mono only for monospace contexts — matching Donut exactly.

**Q1 resolved — see Decisions.**

### R6 — Reduced-motion / transparency parity

Honor `prefers-reduced-motion` (already partially in Donut's CSS) and `prefers-reduced-transparency` (surface-material fallback). Ensure no layout shift when transparency is reduced.

## Acceptance Criteria

- **AC1**: `styles.css` defines the full Donut Mono token set and `bg-background`/`text-foreground`/`border-border`/`bg-primary`/`text-primary-foreground` etc. compile as Tailwind utilities; no hardcoded `rgba(168,85,247,…)` or `text-purple-*` remains in the UI source.
- **AC2**: Modal, TopBar, toast, and command palette use `surface-material` (translucent + blur); under `prefers-reduced-transparency: reduce` they render solid with no broken layout.
- **AC3**: Constellation scroll body uses `scroll-fade`; scrolling produces the edge alpha fade without any colored band overlay.
- **AC4**: LeftRail is replaced by a labeled sidebar; each item shows icon + label; profile groups appear as tabs and correctly filter the grid; creating/renaming/deleting a group persists across restart.
- **AC5**: A new profile can be assigned a group; the group survives SQLite migration on an existing install; `Profile` and `ProfileSummary` carry `group` end-to-end through Rust → IPC → TS.
- **AC6**: Geist Sans + Geist Mono are bundled locally (not CDN-fetched at runtime) via `@font-face`; offline build renders correctly. `--font-sans` maps to Geist Sans, `--font-mono` to Geist Mono.
- **AC7**: `npm --prefix crates/tauri-app/ui run build` (tsc + vite) and `cargo check --workspace` both pass.
- **AC8**: Existing keyboard shortcuts (⌘K/⌘N/⌘1/⌘2/⌘⇧A/⌘,/ESC) keep working under the new shell.
- **AC9**: The 56px icon-rail layout no longer exists; the new sidebar is responsive on mobile viewports (Tauri webview resize).

## Out of Scope

- Light theme (Donut redesign is dark-only).
- Full shadcn component migration (tier C) — atoms stay self-rolled, only re-skinned.
- Donut's Next.js App Router, server components, i18n.
- Donut's sync server, WireGuard, DNS blocker, default-browser, Wayfern engine — none apply.
- New pages beyond the existing Profiles/MCP/Settings sections.
- Brand logo redesign (the `Cube` atom stays; only its colors token-migrate).

## Open Questions

None blocking. Q1 (font application scope) is resolved: use Geist Sans + Geist Mono, matching Donut.

## Decisions

1. **Scope** — Tier B (reskin + layout). Confirmed by user.
2. **Palette** — Full Donut Mono monochrome; drop purple brand color. Confirmed by user.
3. **Font** — Geist Sans (body) + Geist Mono (code/numbers), matching Donut exactly. Confirmed by user ("按照 donut 用的来"). Bundled locally via `@font-face`, not CDN. Maple Mono / Fusion-JetBrainsMapleMono candidates dropped.
4. **Light theme** — Out of scope (Donut redesign is dark-only).
5. **shadcn migration** — Out of scope (atoms stay self-rolled, only re-skinned).

## Technical Notes

- Donut's `globals.css` is the authoritative reference for token values, surface-material, and scroll-fade — already extracted to `.trellis/workspace/zmingu/donut-refs/` (screenshots) and committed to memory in this PRD.
- Cloaksession uses Vite, not Next.js — no impact on CSS porting.
- The `group` field is a minimal-additive schema change; `tags` already exists and is unrelated.
- Bundled fonts (local `@font-face`) are preferred over CDN to keep the app offline-capable (Cloaksession already ships offline).