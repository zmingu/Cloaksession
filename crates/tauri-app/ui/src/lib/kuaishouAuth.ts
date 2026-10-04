/**
 * Kuaishou shop auth IPC (commands/kuaishou_auth.rs — 4 commands, no prior TS).
 * Invoke channel = Rust snake_case fn name; args camelCase.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { EnsureAuthResult, KuaishouAuthPhaseEvent } from "../types";

export const KUAISHOU_AUTH_PHASE_EVENT = "kuaishou-auth-phase";

export const kuaishouAuth = {
  /** `kuaishou_connect` → true when already authenticated, false when scan required. */
  connect: (profileId: string, targetId: string): Promise<boolean> =>
    invoke<boolean>("kuaishou_connect", { profileId, targetId }),

  /** `kuaishou_login` → waits for the user to finish the shop QR scan. */
  login: (profileId: string, targetId: string): Promise<void> =>
    invoke<void>("kuaishou_login", { profileId, targetId }),

  /** Full flow: cookie-reuse verify first, otherwise wait for a scan. */
  ensureAuth: (profileId: string, targetId: string): Promise<EnsureAuthResult> =>
    invoke<EnsureAuthResult>("ensure_kuaishou_auth", { profileId, targetId }),

  /**
   * `kuaishou_login_qr` → the profile's current page as a base64 PNG, or
   * `null` while the profile has no running session yet. The account wizard
   * polls this to show the login QR without a visible browser window.
   */
  loginQr: (profileId: string): Promise<string | null> =>
    invoke<string | null>("kuaishou_login_qr", { profileId }),
};

/** Subscribe to `kuaishou-auth-phase` (await register/cleanup per spec). */
export function onKuaishouAuthPhase(
  cb: (e: KuaishouAuthPhaseEvent) => void,
): Promise<UnlistenFn> {
  return listen<KuaishouAuthPhaseEvent>(KUAISHOU_AUTH_PHASE_EVENT, (event) => {
    cb(event.payload);
  });
}
