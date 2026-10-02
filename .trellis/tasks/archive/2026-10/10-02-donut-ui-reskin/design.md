# Design — Donut-style UI reskin and sidebar layout

## Architecture & boundaries

The work splits into two layers with a clean seam:

1. **Token + material layer (R1–R3, R5–R6)** — pure CSS, no runtime logic. `styles.css` gains a `@theme inline` block plus utility classes. Components migrate from hardcoded rgba to tokens. This layer is reversible by reverting one file.
2. **Sidebar IA layer (R4)** — touches Rust domain types, SQLite, IPC, and a new shell component. This is the risky layer because it changes the on-disk schema.

The seam between them: the token layer changes *how things look*; the sidebar layer changes *what the shell contains*. They can be implemented and reviewed independently. R4 does not depend on R1–R3 being done first, but doing tokens first makes the sidebar visuals land correctly the first time.

## Token system (R1, R2)

### `styles.css` additions

Adopt Donut's `@theme inline` mapping verbatim, with `:root` (light, unused in MVP) and `.dark` (the only active theme) blocks. The active values — Donut Mono:

| Token | Value |
| --- | --- |
| `--background` | `#070707` |
| `--foreground` | `#ffffff` |
| `--card` | `#0e0e0e` |
| `--card-foreground` | `#e4e4e4` |
| `--popover` | `#0e0e0e` |
| `--popover-foreground` | `#e4e4e4` |
| `--primary` | `#ffffff` |
| `--primary-foreground` | `#070707` |
| `--secondary` | `#161616` |
| `--secondary-foreground` | `#e4e4e4` |
| `--muted` | `#161616` |
| `--muted-foreground` | `#a0a0a0` |
| `--accent` | `#1f1f1f` |
| `--accent-foreground` | `#ffffff` |
| `--destructive` | `#ec6a5e` |
| `--destructive-foreground` | `#070707` |
| `--success` | `#61c554` |
| `--success-foreground` | `#070707` |
| `--warning` | `#f4be4f` |
| `--warning-foreground` | `#070707` |
| `--border` | `rgba(255, 255, 255, 0.06)` |
| `--input` | `rgba(255, 255, 255, 0.1)` |
| `--ring` | `#6b6b6b` |
| `--radius` | `0.625rem` |
| `--ease-out` | `cubic-bezier(0.23, 1, 0.32, 1)` |
| `--sidebar` | `oklch(0.21 0.006 285.885)` |
| `--sidebar-foreground` | `oklch(0.985 0 0)` |
| `--sidebar-border` | `oklch(1 0 0 / 10%)` |

### Migration targets (purple → token)

| File:line | Current | New |
| --- | --- | --- |
| `styles.css:24` | `rgba(168, 85, 247, 0.4)` focus ring | `var(--ring)` |
| `styles.css:29` | `rgba(168, 85, 247, 0.5)` selection | `color-mix(in oklab, var(--foreground) 30%, transparent)` |
| `styles.css:118` | `.mz-button:hover[accent]` purple | `var(--accent)` |
| `styles.css:136-148` | `.btn-brand` gradient | `var(--primary)` bg / `var(--primary-foreground)` text |
| `LeftRail.tsx:47,50-51` | `text-purple-300`, `rgba(168,85,247,0.12/0.25)` | `text-primary`, `var(--accent)` + `var(--ring)` |
| `Constellation.tsx:202,205-208` | `text-purple-300`, `rgba(168,85,247,0.10/0.25)` tag chips | `text-accent-foreground`, `var(--accent)` |
| `Constellation.tsx:30-34` | ad-hoc `#34d399/#c084fc/#f87171/#94a3b8` dots | `var(--success)` / `var(--warning)` / `var(--destructive)` / `var(--muted-foreground)` |

The semantic dot colors are a deliberate remap: Donut has no "AI-driven" state, but Cloaksession's `ai` tile-state can map to `--warning` (amber) — distinct from `running` (success green) and `error` (destructive coral).

## Material + scroll-fade (R3)

Copy `surface-material`, `surface-material-popover`, `surface-material-card` and the `scroll-fade` / `scroll-fade-x` utilities verb-tiim from Donut's `globals.css`. Add a `useScrollFade()` hook (writes `data-fade-top` / `data-fade-bottom` on scroll) — Donut ships one; port it to `crates/tauri-app/ui/src/lib/useScrollFade.ts`. Apply:
- `surface-material` → `Modal`, `TopBar`, command palette, toast.
- `scroll-fade` → Constellation scroll body (`Constellation.tsx:229`), MCP activity log, settings scroll.

## Sidebar IA (R4)

### Data model change

`crates/multizen-core/src/profile.rs`: add `pub group: Option<String>` to `Profile` (and a nullable column on `ProfileSummary` for fast list rendering). `#[serde(rename_all = "camelCase")]` keeps the TS field as `group: string | null`.

`crates/profile-manager/src/` migration:
- New migration adding `group TEXT NULL` to the profiles table.
- Default existing rows to `NULL` (ungrouped).
- Group names are free-form strings stored on the profile row (Donut has a separate groups table; Cloaksession doesn't need cross-profile group settings yet, so a column is the minimal-additive choice). If group-rename needs to update all members, a follow-up task can promote groups to a table — out of scope here.

### IPC

New Tauri commands in `crates/tauri-app/src/commands/`:
- `list_groups() -> Vec<{name, count}>` — derived `SELECT DISTINCT group, COUNT(*) FROM profiles GROUP BY group`.
- `set_profile_group(id: ProfileId, group: Option<String>)` — UPDATE.
- `delete_group(name: String)` — set `group = NULL` where `group = name` (ungroup all members).

### UI

Replace `LeftRail` with `Sidebar` (new component, `components/screens/Sidebar.tsx`):
- Width ~220px (Donut shadcn-sidebar default), collapsible to icon-only.
- Labeled items: Profiles, MCP, Settings (icon + text).
- Above the profiles grid, a row of group tabs: "All" + one tab per distinct group + "Ungrouped".
- Group tabs filter the Constellation via a new `group` prop (mirrors the existing `activeTag` filter pattern at `Constellation.tsx:62`).
- MCP / Settings sections unchanged in content, just re-skinned.

### Backward compat

- Existing profiles with no group → `NULL` → appear under "Ungrouped" tab and in "All". No data loss.
- The legacy `tags` field is untouched and still works in parallel.
- MCP `list_profiles` response gains `group` field; existing clients ignore unknown fields, no break.

## Font (R5)

- Remove Google Fonts `<link>` from `index.html:7-12`.
- Add `@font-face` declarations for Geist Sans (weights 400/500/600/700) and Geist Mono (400/500/600) in `styles.css`, pointing at bundled WOFF2 in `crates/tauri-app/ui/public/fonts/`.
- Update `@theme`: `--font-sans: "Geist Sans", ...`, `--font-mono: "Geist Mono", ...`.
- WOFF2 source: `geist` npm package (Vercel) — install as devDep, copy WOFF2 to `public/fonts/` at build time, or commit the WOFF2 directly (preferred for offline — matches the existing `flag-icons` vendoring pattern).

## Reduced motion / transparency (R6)

- `@media (prefers-reduced-motion: reduce)` neutralizes animations globally (copy from Donut's `globals.css`).
- `@media (prefers-reduced-transparency: reduce)` collapses `surface-material*` to solid `var(--background)` / `var(--popover)` / `var(--card)`.
- Verify no layout shift: surface-material uses `color-mix` on `--background`, so the solid fallback is the same hue, only opacity changes.

## Trade-offs

- **Groups as a column vs. separate table**: column is minimal and migration-safe; costs a full-table scan on rename (acceptable at Cloaksession's profile counts). Donut uses a table for cross-profile group settings; deferring that is a fair MVP trade.
- **Geist vendored vs. CDN**: vendoring bloats the repo by ~150KB WOFF2 but preserves offline capability — non-negotiable for this app.
- **Token layer dark-only**: locking to `.dark` values means a future light theme needs a second pass; acceptable since Donut itself is dark-only in the redesign.

## Rollback

- Token layer: revert `styles.css` + the inlined component styles; one commit.
- Sidebar layer: revert the new `Sidebar.tsx`, restore `LeftRail`, and run a *down* migration dropping the `group` column. Schema change is additive, so a down migration is safe on any install that hasn't created groups yet; if groups exist, they're silently lost (acceptable — groups are organizational, not data).