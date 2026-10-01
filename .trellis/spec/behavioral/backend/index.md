# behavioral Backend Guidelines

Small deterministic, IO-free generators for mouse points, typing delays, and scroll deltas. Runtime dispatch and sleeping belong to cdp-driver, not this crate.

## Guides

- [Generator contracts and tests](./generators.md)
- Consumer: [CDP sessions and operations](../../cdp-driver/backend/index.md).

## Pre-Development Checklist

- [ ] Read the affected module in `crates/behavioral/src/` and [generator contracts](./generators.md), then its matching integration test.
- [ ] Trace use in `crates/cdp-driver/src/tools.rs`; do not confuse generated timing with browser-level human behavior guarantees.
- [ ] Read [shared thinking guides](../../guides/index.md) and check existing generator helpers before adding randomness or IO dependencies.

## Quality Check

- [ ] Run `cargo test -p behavioral --locked` from the root with Rust stable and existing dependencies (`crates/behavioral/Cargo.toml`). No browser, filesystem fixture, Node, or network required.
- [ ] Assert determinism and mathematical/output invariants with fixed seeds, not one randomly chosen path or wall-clock timings.
- [ ] Cover changed edge behavior (empty/Unicode text, zero/negative scroll, identical endpoints) if modifying it; current tests do not cover all of these.
- [ ] Preserve units and consumer expectations. Existing tests under `crates/behavioral/tests/` are computation checks, not anti-detection evidence.
