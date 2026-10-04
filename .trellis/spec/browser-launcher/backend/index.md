# browser-launcher Backend Guidelines

Owns browser/Node process handles, engine-specific launch inputs, browser-directory preparation, and proxy geo probing. CDP attachment and UI events belong to consumers.

**Chromix is the only browser engine** (CFT/CloakBrowser were deleted — see [Engine lifecycle](./lifecycle.md#architecture-decision-chromix-is-the-only-engine)). Every profile launches through the bundled Node SDK bridge; there is no engine selector and no native-flag launch path.

## Guides

- [Engine lifecycle (Chromix-only)](./lifecycle.md)
- [Chromix sidecar contract](./chromix.md)
- [Business directory isolation and running snapshots](./business-isolation.md)
- Boundaries: [profile storage](../../profile-manager/backend/index.md), [CDP](../../cdp-driver/backend/index.md), [Tauri adapter](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read both guides before changing shared launch behavior; Chromix owns fingerprint/proxy/session setup inside its SDK bridge, so the host does not translate legacy `Profile.fingerprint`.
- [ ] Trace `crates/browser-launcher/src/driver.rs::launch_with_chromix` through the sidecar function, registry, and Tauri caller.
- [ ] Read the affected tests and [shared thinking guides](../../guides/index.md). Check ownership on startup failure/cancellation/close.

## Quality Check

- [ ] From the repository root, `cargo test -p browser-launcher --locked` requires Rust stable/native toolchain and Node on PATH: non-ignored Chromix tests start a fake-SDK Node sidecar, not a browser. Local socket binding must be available.
- [ ] Narrow pure/loopback checks: `cargo test -p browser-launcher --locked --test args --test socks5_bridge --test proxy_geo --test version --test version_detect --test data_dir --test business_guard`.
- [ ] Bridge changes: run `npm test` in `crates/tauri-app/resources/chromix` with Node >=20 (CI uses 22) and existing runtime dependencies; see its `package.json` and [sidecar guide](./chromix.md).
- [ ] Real-browser / native acceptance for the Chromix path is a manual opt-in item (SDK download + a real desktop); there is no longer an ignored `--test driver` harness (the old `tests/driver.rs` was removed with the legacy engine). Do not claim readiness, graceful termination, complete proxy authentication, or anti-leak guarantees from argument/unit tests. `.github/workflows/build.yml` runs the locked workspace and Node bridge suites, not a live browser.
