# tauri-app Backend Guidelines

Owns desktop startup/state, launcher-thread integration, IPC/events, embedded MCP dispatch, extension/archive commands and update orchestration. SQLite, JSON settings, launch mechanics and CDP remain in their library owners.

## Guides

- [Runtime ownership and lifecycle](./runtime.md)
- [Kuaishou read-only identity, polling and avatar contract](./kuaishou-identity.md)
- [Kuaishou initialization, concurrency and archive contract](./kuaishou-initialization.md)
- [IPC, events, and verification](./ipc.md)
- [Archives, extensions, and updates](./resources.md)
- [Internationalization (i18n)](./i18n.md)

## Pre-Development Checklist

- [ ] Read the runtime and IPC guides, `crates/tauri-app/src/lib.rs::run` and the affected command/driver methods; old README/module comments omit implemented features.
- [ ] Read the resource guide for any archive/extension/update/file-boundary change.
- [ ] Follow the relevant owner: [models](../../multizen-core/backend/index.md), [profile DB](../../profile-manager/backend/index.md), [settings](../../settings-store/backend/index.md), [launcher](../../browser-launcher/backend/index.md), [CDP](../../cdp-driver/backend/index.md), [MCP](../../mcp-server/backend/index.md).
- [ ] Read [shared thinking guides](../../guides/index.md), `crates/tauri-app/ui/src/lib/ipc.ts` and `crates/tauri-app/ui/src/types.ts` for consumed fields, not just Rust signatures.

## UI conventions (`crates/tauri-app/ui`)

- **A closed `Modal` is not an unmounted component.** `components/atoms/Modal` returns `null` when `open` is false, but its children stay mounted, so a `useEffect` whose deps are only `[step]`/`[profileId]` will **not** clean up on close. Any interval/timeout/subscription inside a modal must gate on `open` (and include it in the deps), e.g. the shop-account wizard's QR/identity poll: `if (!open || step !== "waiting" || !profileId) return;`, deps `[open, step, profileId]`. Symptom of missing it: the timer keeps polling (extra `screenshot`/`detect` calls) after the dialog is gone.
- Optional IPC parameters stay optional **on the wire**: wrappers send the field only when set (see [IPC](./ipc.md)). An optional param that always serializes a default is a contract change, not a no-op.
- Sidebar/nav buttons carry `title={navLabel(id)}` (the bare label, no shortcut suffix). Playwright tests target them with `getByRole("button", { name: "<label>", exact: true })`, never `getByTitle("Profiles · ⌘1")`.
- **Shop accounts are named from the detected identity, not typed up front.** The wizard creates the profile with the placeholder `kuaishou.shop.unnamed` and, once `kuaishou_identity_detect` returns a `platformUserId`, renames it via `profiles_update({ name })` to the nickname (falling back to the Kuaishou ID). There is no name field. If the user cancels before sign-in, the profile keeps the placeholder name and can be renamed later. The rename is guarded by a ref so it fires at most once per wizard run.
- **No engine selector, no CloakBrowser "Fingerprint" bar.** Chromix is the only engine (see [engine decision](../../browser-launcher/backend/lifecycle.md#architecture-decision-chromix-is-the-only-engine)): `Settings.tsx` renders `ChromixSettingsEditor` directly (the engine radio group and `settings.engine.*` i18n keys were removed), and profile create/edit uses only the Chromix options/fingerprint components. The wizard's first step is a **sectioned form** — 主页 / 代理 / 扩展 / 指纹 (localized `kuaishou.wizard.section.*`, all Chinese) — reusing `BrowserSection`/`ExtensionsSection`/`ChromixProfileOptions`; the home section defaults to `https://s.kwaixiaodian.com/zone/home` and the create call sends **no** `fingerprint` field (it sets `chromixOptions.geoip = true` instead when a proxy is entered).

## Quality Check

- [ ] Follow the command/prerequisite table in [IPC verification](./ipc.md); do not mistake browser-free tests for a desktop end-to-end check.
- [ ] Match handler registration, IPC args, serde fields and listener payloads. Rust enum variant renaming does not rename variant fields.
- [ ] Check both Tauri IPC and embedded MCP consumers when changing driver behavior, plus startup versus live-settings effects.
- [ ] No install/browser/build action is implied by these docs. The bootstrap itself is docs-only; future runtime verification must be authorized and reported as actually run or skipped.
