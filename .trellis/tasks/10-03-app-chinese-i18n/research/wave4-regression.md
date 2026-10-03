# Wave4 regression — component fallback replacement + full regression

Date: 2026-10-03. Lane: worker (Wave4). Dictionary files NOT touched (codex owns
`i18n/*.ts`); no `Cargo.toml` changes.

## 1. Fallback replacements (all `t()` now, no local English)

| File | Before | After |
|---|---|---|
| `profile/ExtensionsSection.tsx` | save-first hint literal; ExtRow `on`; AddRow `Add`; top component had no `useT` | `t("extensions.saveFirst")`, `t("extensions.enabled")`, `t("extensions.add")`; added `const t = useT()` to `ExtensionsSection` |
| `profile/ExtensionCatalog.tsx` | `Adding` / `Add` literals | `t("catalog.adding")` / `t("catalog.add")` |
| `profile/ProxyTester.tsx` | `"Testing…"` busy literal | `t("proxy.testing")` |
| `onboarding/FirstRun.tsx` | step-3 title/body literals | `t("onboarding.nameTitle")` / `t("onboarding.nameBody")` |
| `profile/ProfileRow.tsx` `RowProxyHealth` | `"connected"` / `"unreachable"` / `"checking…"` labels; `` `${error} — click to retry` `` / `` `${label} — click to re-check` `` titles | `t("proxy.connected")` / `t("proxy.unreachable")` / `t("proxy.checking")`; `t("proxy.healthRetry", { error })` / `t("proxy.healthRecheck", { label })` |
| `profile/ProfileTile.tsx` `ProxyHealthRow` | same as Row, plus `"Checking…"` spinner label, `"retry"` affordance, `"proxy"` chip, `"Checking proxy…"` title | same key mapping; spinner → `t("proxy.checking")`, retry → `t("common.retry")`, chip → `t("profile.list.proxyLabel")`, title → `t("proxy.checking")` |
| `activity/ActivityDrawer.tsx` | `FALLBACK_CALLS` / `FALLBACK_LIVE` / `FALLBACK_EMPTY` consts + comment | `t("activity.liveCount", { n })` / `t("activity.callsCount", { n })` / `t("activity.feedEmptyLong")`; consts deleted |
| `mcp/McpPanel.tsx` | `CALLS_SUFFIX = "calls"`; `.replace("{Stdio clients}", …)` on `codexNote`; `.replace("{Auto-start…}", …)` on `disabledHint`; param-less `tokenHint`; hand-written English stdio sentence with `t("mcp.stdioNote")` inside `<code>` | `t("activity.callsCount", { n })`; `t("mcp.codexNote", { url: "url", stdioClients: t("mcp.stdioClients") })`; `t("mcp.disabledHint", { Settings: t("nav.settings"), autoStart: t("settings.mcp.autostart") })`; `t("mcp.tokenHint", { authorization: "Authorization: Bearer <token>" })`; `t("mcp.stdioNote", { tool: "mcp-remote", settings: t("nav.settings") })` as a plain text node |

Decisions:

- Tile/Row short labels reuse the capitalized `proxy.connected` / `proxy.unreachable`
  keys (wave3-findings asked for a call; values are identical strings, no layout
  change). Both rows' checking label/title converge on `t("proxy.checking")`.
- `mcp.stdioNote` renders as a plain text node now (the `<code>` highlights for
  `url` / tool name are gone). Rationale: design.md forbids HTML injection;
  interpolated values can only be text. Acceptable trade-off, flagged for design
  review if inline code styling is wanted later (would need a rich-text key).
- `tokenHint`'s `{{authorization}}` is fed `"Authorization: Bearer <token>"`,
  matching the neighbouring `settings.mcp.tokenHint` wording.

## 2. Regression results (all from repo root / `ui` as specified)

- `npm --prefix crates/tauri-app/ui run build` — pass (1.39 s).
- `node --experimental-strip-types crates/tauri-app/ui/src/i18n/dictionaries.test.mjs` —
  6/6 pass.
- `tsc -b` via `crates/tauri-app/ui/node_modules/.bin/tsc` — clean, no output.
  (Note: bare `npx tsc -b` resolves the wrong `tsc@2.0.4` npm package on this
  machine; use the local binary. The pre-existing `ExtensionsSection` type error
  from wave3-findings is gone.)
- `cargo check --workspace --locked` — pass.
- `cargo test --workspace --locked` — all suites `ok`, 0 failed.
- `npx playwright test --config playwright.config.ts` from `crates/tauri-app/ui` —
  environment note first: the config defaults to `channel: "chrome"` (real Google
  Chrome), which is NOT installed here, so all tests fail to launch. The config
  honors `PLAYWRIGHT_CHANNEL`, and bundled `chromium-1243` exists, so the
  supported invocation in this environment is
  `$env:PLAYWRIGHT_CHANNEL="chromium"; npx playwright test …`.
  With that: **98 passed, 10 failed**. The 10 failures are NOT from this lane:
  `tests/kuaishou-subject.spec.ts` (8, both projects — looks for a
  `textbox[name=搜索快手昵称/身份证]` that never appears),
  `tests/chromix-settings.spec.ts:225` (mobile narrow viewport),
  `tests/kuaishou-identity.spec.ts:245` (mobile timing). Stash-baseline check
  (my 8 files stashed, same 3 spec files rerun): 9 of the 10 fail identically
  without my edits; the 10th (identity:245) passed on rerun — flaky. None of the
  failing specs touch the Wave4-edited components.

## 3. Language-switch verification

New `crates/tauri-app/ui/tests/wave4-verify.spec.ts` (kept; 3 tests × 2 projects,
6/6 pass with `PLAYWRIGHT_CHANNEL=chromium`):

1. en → zh switch updates nav, profiles list (`直连——无代理`), extensions
   section (`添加`, `浏览目录`), MCP panel (`连接 AI 智能体`, `暂无智能体调用`),
   activity drawer (`暂无 MCP 调用…`, `0 次调用`); body-text sweep over the
   Profiles/MCP/Settings views finds none of `click to retry`, `click to
   re-check`, `Testing…`, `Checking…`, `Name your first profile`,
   `No MCP calls yet`, `Claude Desktop's config has no`, `{Stdio clients}`,
   `{{`.
2/3. Onboarding name step renders in en and in zh (`为你的第一个浏览器配置命名。`).

Profile-data invariance (observable 5): after the zh switch the fixture
profile's `fingerprint.locale` is still `en-US`, `profilePatches()` is empty
(language switch only emits `settings_update`), and `document.lang` is `zh-CN`.
By construction the Wave4 edits are display-only (string literal → `t()` swaps,
no data-flow changes), so `Profile.locale` and browser args cannot be affected.

## 4. Remaining gaps (for dictionary owner / other lanes)

1. **No key: LiveExtensions companion hint** (`ExtensionsSection.tsx:143-146`,
   "Or open the Chrome Web Store … Changes apply on the next launch."). Left
   verbatim (contains the `Add to Cloaksession` protocol exception). Codex: needs
   a key (e.g. `extensions.liveHint`) with the button label as a param/exception.
2. **Not Wave4 scope — sheet chrome still English**: `profileSheetKit.tsx` tab
   labels (`General/Browser/Proxy/Extensions/…`), `ProfileEditSheet`, and the
   `App.tsx` edit-modal title/subtitle (`Edit ${name}`, `Profile changes
   autosave…`) are unmigrated (another lane's ownership; `ProfileEditSheet.tsx`
   has no worktree changes). Verified visible in the zh error-context snapshot.
3. **Pre-existing literals, other lanes**: `PillForState` (`running`/`error`/
   `idle`, `ai-driven`) in `ProfileRow.tsx:218-235` predates Wave3 (not in
   findings); untouched.
4. **Not UI-reachable in the fixture**: `proxy.testing` busy state,
   `healthRetry`/`healthRecheck` error titles, and checking-state titles need a
   proxied profile + failing probe (mock has no proxy and no `proxy_detect_geo`
   handler). Verified by code inspection + `tsc` only.
5. **No key: raw-args stale-draft warning** (`ChromixFingerprintForm.tsx:212`,
   "options.args changed while this raw draft was open…"). Left as-is.
6. **Playwright env**: full suite requires `PLAYWRIGHT_CHANNEL=chromium` on this
   machine (real Chrome absent); the 9 pre-existing kuaishou/chromix failures
   above should be triaged by their owners.
