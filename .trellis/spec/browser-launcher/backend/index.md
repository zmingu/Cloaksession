# browser-launcher Backend Guidelines

Owns browser/Node process handles, engine-specific launch inputs, legacy proxy bridge, and browser-directory preparation. CDP attachment and UI events belong to consumers.

## Guides

- [Engine lifecycle and legacy launch](./lifecycle.md)
- [Chromix sidecar contract](./chromix.md)
- [Business directory isolation and running snapshots](./business-isolation.md)
- Boundaries: [profile storage](../../profile-manager/backend/index.md), [CDP](../../cdp-driver/backend/index.md), [Tauri adapter](../../tauri-app/backend/index.md).

## Pre-Development Checklist

- [ ] Read both guides before changing shared launch behavior; Chromix does not use legacy fingerprint/proxy/session setup.
- [ ] Trace `crates/browser-launcher/src/driver.rs` through the relevant argument/sidecar function, registry, and Tauri caller.
- [ ] Read the affected tests and [shared thinking guides](../../guides/index.md). Check platform branches and ownership on startup failure/cancellation/close.

## Quality Check

- [ ] From the repository root, `cargo test -p browser-launcher --locked` requires Rust stable/native toolchain and Node on PATH: non-ignored Chromix tests start a fake-SDK Node sidecar, not a browser. Local socket binding must be available.
- [ ] Narrow pure/loopback checks: `cargo test -p browser-launcher --locked --test args --test socks5_bridge --test proxy_geo --test version --test version_detect`.
- [ ] Bridge changes: run `npm test` in `crates/tauri-app/resources/chromix` with Node >=20 (CI uses 22) and existing runtime dependencies; see its `package.json` and [sidecar guide](./chromix.md).
- [ ] Optional real-browser check: set `RUN_CDP_INTEGRATION=1` and `MULTIZEN_TEST_BINARY` to a real CloakBrowser binary, then `cargo test -p browser-launcher --locked --test driver -- --ignored`. The test returns early without both variables; a green skip is not runtime evidence.
- [ ] Do not claim readiness, graceful termination, complete proxy authentication, or anti-leak guarantees from argument/unit tests. `.github/workflows/build.yml` runs the locked workspace and Node bridge suites, not the ignored browser test.
