# Wave3 Batch 4d findings — Sections I (mcp.*) / J (activity.*) / K (settings.* remaining) / L (update.*)

Date: 2026-10-03. Lane: codex#2. Files owned: `components/mcp/McpPanel.tsx`,
`components/activity/ActivityDrawer.tsx`, `components/screens/Settings.tsx`,
`components/UpdateBanner.tsx`.

## Status

- `McpPanel.tsx`, `ActivityDrawer.tsx`, `UpdateBanner.tsx`: migrated to `t()`.
- `Settings.tsx`: **already fully migrated by Wave2** before this lane ran. Verified
  by reading the whole file: no remaining hardcoded UI text (title/desc/aria/
  placeholder/JSX text all go through `t()`), and the Wave2 language-switch UI is
  intact. This lane therefore made **no changes** to `Settings.tsx`.
- MCP tool names (`multizen.*`), the LLM clipboard prompt, and all TOML/JSON/header
  config samples were left as protocol/original, per PRD.

## Dictionary defects that block the "live language switch" observable (Wave4 must fix)

The pre-populated dictionaries are **defective inside this lane's domains**. Per the
batch plan ("if a key is missing, use an in-component fallback and log it; do not
edit `i18n/*.ts` from a component lane"), this lane did **not** edit the dictionaries.
Wave4 must repair these:

1. **Sentinel values** — `activity.callsCount` and `activity.liveCount` are the
   literal string `"/"` in **both** `en.ts` and `zh-CN.ts`. They carry `{{n}}` in the
   inventory but no placeholder now. Impact: the drawer's call/live counters never
   translate. This lane used local `FALLBACK_CALLS`/`FALLBACK_LIVE` (English only).
   Fix: `en` `"{{n}} calls"` / `"{{n}} live"`, `zh` `"{{n}} 次调用"` / `"{{n}} 进行中"`.

2. **Missing key** — `activity.feedEmptyLong` does not exist in either dictionary
   (inventory §J requires it: `No MCP calls yet. Connect an agent (see the MCP tab)
   and its tool calls stream here.`). This lane used `FALLBACK_EMPTY` (English only).
   Fix: add to both dictionaries.

3. **Single-brace placeholders** — `mcp.codexNote`, `mcp.disabledHint`,
   `mcp.tokenHint` use `{...}` instead of `{{...}}`, so `translate()` cannot
   substitute them. `mcp.codexNote`'s `{Stdio clients}` also inlines a nested key.
   Fix: convert to `{{...}}` named params (and make the Settings reference a param).

4. **Untranslated `zh-CN` (~35 keys in-domain)** — `zh-CN.ts` still holds English for
   most of `mcp.*` (hero, feedHint, steps.*, configGroups.*, copyForLlm.title,
   llmPrompt.*, codexNote, disabledHint, legacyEndpoint, tokenHint), several
   `update.*` (availableBanner, readyBanner, downloading, download, nativeDownload*,
   ioFailed.*, noInstaller, installerMissing, statusFailed), and `mcp.stdioNote`.
   Impact: switching to Chinese does not change these strings, failing AC2 for these
   domains. Also `mcp.feedEmpty` is mis-populated in zh as `"MCP tab"`.

5. **`update.nativeDownloadBody`** in `en.ts` is `"Download update"` — identical to
   the title; the inventory expects a body with the version + URL. Fix value.

6. Stale extras not referenced by the inventory (`settings.engine.options.selected`,
   `settings.mcp.*Token.aria`) exist; harmless, left alone.

7. **`mcp.stdioNote` value is only the tool name** (`"mcp-remote"`), but the
   inventory assigns the whole Stdio-client sentence to it. There is no key for the
   connective copy, so `McpPanel.tsx` keeps that sentence as a local JSX fallback
   (protocol tokens `mcp-remote` and the Settings label are still injected from
   their real keys). Fix: give `mcp.stdioNote` the full sentence with `{{tool}}`/
   `{{settings}}` params, then drop the local fallback.

## Cross-lane note

`npx tsc -b` currently fails only in `components/profile/ExtensionsSection.tsx`
(lane 4c, actively editing: `extensions.saveFirst` / `extensions.enabled` /
`extensions.add` keys missing from the dictionary, plus an unused `t`). This lane's
four files type-check cleanly on their own.

## 4c — codex — Sections E/H/M (2026-10-03)

Migrated `ProxyTester.tsx` (§E), `ExtensionsSection.tsx` + `ExtensionCatalog.tsx` +
`data/extensionCatalog.ts` (§H), `FirstRun.tsx` (§M) to `useT()` /
pre-populated keys only. No dictionary files touched; no other Wave3 domains touched.
Third-party catalog titles/descriptions in `data/extensionCatalog.ts` are left
verbatim (external); only the app-owned category labels moved to keys, via
`labelKey: TranslationKey` on `CatalogCategory` (the old `label` field is gone).
`npm run build` passes.

### A. Hardcoded fallbacks kept (no usable key) — need Wave4 keys

| # | Location | Current English | Proposed key |
|---|---|---|---|
| 1 | `ExtensionsSection.tsx` save-first hint | `Save the profile first — then you can add extensions (.crx / .zip / folder, or by Chrome Web Store link) and they&apos;ll load in this profile only.` | `extensions.saveFirst` |
| 2 | `ExtensionsSection.tsx` ExtRow enable label | `on` | `extensions.enabled` |
| 3 | `ExtensionsSection.tsx` AddRow submit | `Add` | `extensions.add` |
| 4 | `ExtensionCatalog.tsx` busy state | `Adding` | `catalog.adding` |
| 5 | `ExtensionCatalog.tsx` idle state | `Add` | `catalog.add` |
| 6 | `ProxyTester.tsx` busy state | `Testing…` | `proxy.testing` |
| 7 | `FirstRun.tsx` step-3 title | `Name your first profile.` | `onboarding.nameTitle` |
| 8 | `FirstRun.tsx` step-3 body | `Just a label to find it later. Tags, proxy, and fingerprint are editable anytime from the profile panel.` | `onboarding.nameBody` |

### A2. §E proxy-health in ProfileRow/ProfileTile — exact keys used, defective keys left as fallbacks

`ProfileRow.RowProxyHealth` and `ProfileTile.ProxyHealthRow` are §E files, so the
exact-match strings were migrated: `proxy.direct`, `proxy.directNone`,
`proxy.connected`, `proxy.unreachable`, `proxy.checking`. The remaining
proxy-health strings have no usable key and were kept as in-component fallbacks:

| # | Location | Current English | Problem |
|---|---|---|---|
| 1 | ProfileRow:264 / ProfileTile:274 | `{{error}} — click to retry` | `proxy.healthRetry` value is the sentinel `Health Retry` (no `{{error}}`) |
| 2 | ProfileRow:266 / ProfileTile:276 | `{{label}} — click to re-check` | `proxy.healthRecheck` value is the sentinel `Health Recheck` (no `{{label}}`) |
| 3 | ProfileRow:259/261, ProfileTile:269/272 | `connected` / `unreachable` / `Checking…` (short label) | only capitalized keys exist (`proxy.connected`/`proxy.unreachable`); the row uses a lowercase short form — kept literal to avoid changing the dense-row layout |

Wave4 fix: give `proxy.healthRetry` / `proxy.healthRecheck` real templates with
`{{error}}` / `{{label}}`, and decide whether the row's short label should reuse the
capitalized keys.

### B. Deliberately untranslated (per inventory, no action)

- `Add to Cloaksession` companion reference in the LiveExtensions hint (protocol/companion exception, §T1).
- Platform chips `macOS / Windows / Linux / Chromium / MCP HTTP` in FirstRun (keep-original, §M).
- `data/extensionCatalog.ts` third-party names/descriptions (external, §H).
- Dynamic backend/geo error text (`{error}`), `IP`/`TZ` result labels, `host DNS`.
- `ExtensionsSection` loading preset text now reuses existing `common.loading`.
