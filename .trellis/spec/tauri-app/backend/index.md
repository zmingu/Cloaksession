# tauri-app Backend Guidelines

Owns desktop startup/state, launcher-thread integration, IPC/events, embedded MCP dispatch, extension/archive commands and update orchestration. SQLite, JSON settings, launch mechanics and CDP remain in their library owners.

## Guides

- [Runtime ownership and lifecycle](./runtime.md)
- [Kuaishou read-only identity, polling and avatar contract](./kuaishou-identity.md)
- [Kuaishou initialization, concurrency and archive contract](./kuaishou-initialization.md)
- [IPC, events, and verification](./ipc.md)
- [Archives, extensions, and updates](./resources.md)

## Pre-Development Checklist

- [ ] Read the runtime and IPC guides, `crates/tauri-app/src/lib.rs::run` and the affected command/driver methods; old README/module comments omit implemented features.
- [ ] Read the resource guide for any archive/extension/update/file-boundary change.
- [ ] Follow the relevant owner: [models](../../multizen-core/backend/index.md), [profile DB](../../profile-manager/backend/index.md), [settings](../../settings-store/backend/index.md), [launcher](../../browser-launcher/backend/index.md), [CDP](../../cdp-driver/backend/index.md), [MCP](../../mcp-server/backend/index.md).
- [ ] Read [shared thinking guides](../../guides/index.md), `crates/tauri-app/ui/src/lib/ipc.ts` and `crates/tauri-app/ui/src/types.ts` for consumed fields, not just Rust signatures.

## Quality Check

- [ ] Follow the command/prerequisite table in [IPC verification](./ipc.md); do not mistake browser-free tests for a desktop end-to-end check.
- [ ] Match handler registration, IPC args, serde fields and listener payloads. Rust enum variant renaming does not rename variant fields.
- [ ] Check both Tauri IPC and embedded MCP consumers when changing driver behavior, plus startup versus live-settings effects.
- [ ] No install/browser/build action is implied by these docs. The bootstrap itself is docs-only; future runtime verification must be authorized and reported as actually run or skipped.
