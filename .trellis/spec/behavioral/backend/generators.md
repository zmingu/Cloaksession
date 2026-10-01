# Generator Contracts and Tests

## Module boundaries

- `crates/behavioral/src/mouse.rs::humanized_path`: 12 quadratic Bezier samples, with bounded perpendicular control-point jitter and ease-out sampling. The output excludes the initial point and forces the final point exactly to the target.
- `crates/behavioral/src/keyboard.rs::humanized_keystroke_delays`: one `u64` millisecond delay per Rust Unicode scalar (`text.chars()`), not per UTF-8 byte or grapheme. A three-uniform approximation produces a clamped base, then whitespace adds 60 ms and `. , ! ?` add 90 ms. The base clamp is not a clamp on the final vector.
- `crates/behavioral/src/scroll.rs::humanized_scroll_steps`: positive seeded weights normalized to the signed input delta. Step count is `(6 + (abs(delta_y) / 120) as usize).clamp(3, 14)`; preserve floating-point tolerance when asserting sums.

Each module currently has the same small deterministic wrapping LCG (`seed.max(1)`). Seed 0 and 1 therefore alias. This is reproducible variation, not cryptographic randomness, and not proof that all distinct seeds produce different output. Keep pure functions returning vectors; don't put Tokio sleeps, CDP, logging, persistence, or implicit wall-clock seeds here.

## Real example and regression style

From `crates/behavioral/tests/mouse.rs::path_is_deterministic_for_same_seed`:

```rust
let a = humanized_path((0.0, 0.0), (200.0, 150.0), 7);
let b = humanized_path((0.0, 0.0), (200.0, 150.0), 7);
assert_eq!(a, b, "same seed → same path");
```

The same pattern appears in `crates/behavioral/tests/keyboard.rs::deterministic_for_same_seed` and `crates/behavioral/tests/scroll.rs::deterministic`. Pair it with useful invariants: mouse endpoint/progression, delay cardinality and punctuation effects, scroll sum and no dominating step. Current keyboard range tests use one ASCII phrase; do not turn that sample into a universal distribution claim.

## Runtime use and limitations

`crates/cdp-driver/src/tools.rs::click` creates a short mouse approach using coordinate-derived seed; `type_text` uses text byte length as seed, iterates characters, and sleeps using the generated millisecond values. Mouse samples are dispatched without per-sample sleeps. Scroll generation is exported but is not wired into those page tools.

Inputs are assumed to be ordinary finite coordinates/deltas; the functions do not validate NaN/infinity or expose a `Result`. Validate untrusted operation inputs at the calling boundary rather than claiming these math helpers sanitize them. No trained human-motion model or browser invisibility guarantee is implied by the names.
