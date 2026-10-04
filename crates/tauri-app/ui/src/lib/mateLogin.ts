/**
 * Mate (直播伴侣) QR login IPC (commands/mate_login.rs — 3 commands).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { MateLoginState } from "../types";

export const MATE_LOGIN_STATE_CHANGED = "mate-login-state-changed";

export const mateLogin = {
  /** `mate_login_start` → begins QR flow, returns first snapshot. */
  start: (accountId: string): Promise<MateLoginState> =>
    invoke<MateLoginState>("mate_login_start", { accountId }),

  /** `mate_login_cancel` → cancels an in-flight QR flow. */
  cancel: (accountId: string): Promise<MateLoginState> =>
    invoke<MateLoginState>("mate_login_cancel", { accountId }),

  /** `mate_login_state` → polls the current snapshot. */
  state: (accountId: string): Promise<MateLoginState> =>
    invoke<MateLoginState>("mate_login_state", { accountId }),
};

/** Subscribe to `mate-login-state-changed` (await register/cleanup). */
export function onMateLoginStateChanged(
  cb: (s: MateLoginState) => void,
): Promise<UnlistenFn> {
  return listen<MateLoginState>(MATE_LOGIN_STATE_CHANGED, (event) => {
    cb(event.payload);
  });
}
