/**
 * Chromix fingerprint-seed helpers for the Kuaishou onboarding wizards.
 *
 * Chromix derives one coherent device persona (GPU, screen, hardware, canvas,
 * fonts, …) from a single `--fingerprint=<seed>` launch argument, so "a random
 * device" is really "a random seed". These helpers keep that one flag coherent
 * inside the free-form `chromixOptions` JSON a profile create call carries.
 *
 * Everything here is defensive: `chromixOptions` is user-editable JSON, so a
 * malformed `args` array must never make the wizard throw — it is left as-is.
 */
import { setChromixFlag } from "./chromixFingerprint";

/** Documented `--fingerprint` default range (10000–99999). */
export function randomFingerprintSeed(): number {
  return 10000 + Math.floor(Math.random() * 90000);
}

/** A copy of `options` carrying a fresh random `--fingerprint=<seed>`. */
export function withRandomFingerprintSeed(
  options: Record<string, unknown>,
): Record<string, unknown> {
  try {
    return setChromixFlag(options, "--fingerprint", String(randomFingerprintSeed()));
  } catch {
    // `args` is not a string array (hand-edited SDK JSON): keep it untouched.
    return options;
  }
}

/** The seed currently set in `options.args`, or null when unset/off. */
export function fingerprintSeedOf(options: Record<string, unknown>): string | null {
  const args = Array.isArray(options.args) ? (options.args as unknown[]) : [];
  const arg = args.find(
    (item): item is string =>
      typeof item === "string" && (item === "--fingerprint" || item.startsWith("--fingerprint=")),
  );
  if (!arg) return null;
  const eq = arg.indexOf("=");
  return eq < 0 ? "" : arg.slice(eq + 1);
}
