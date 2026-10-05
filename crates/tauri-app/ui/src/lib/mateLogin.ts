/**
 * Mate (直播伴侣) account CRUD + QR login IPC
 * (commands/mate_login.rs — 7 commands + 1 event).
 *
 * 伴侣账号**不绑定浏览器环境**（独立表 `mate_accounts`，无 `profileId`）：
 * 登录是纯 HTTP 4 步扫码，成功后 token 由后端写入同一行；本模块的账号 CRUD 与
 * 登录状态机因此完全解耦——先有账号，才能对它发起扫码。
 *
 * Invoke channel = snake_case Rust function name; argument keys are camelCase
 * (repo-wide Tauri convention, see `lib/ipc.ts`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { MateAccount, MateLoginState } from "../types";

export const MATE_LOGIN_STATE_CHANGED = "mate-login-state-changed";

export const mateLogin = {
  /** `mate_accounts_list` → 全部伴侣账号（按创建时间升序）。 */
  list: (): Promise<MateAccount[]> => invoke<MateAccount[]>("mate_accounts_list"),

  /**
   * `mate_account_add` → 新建账号（UUID 主键）。
   *
   * 不传 `label`（`undefined`）时后端落占位别名（"未命名伴侣"）；该账号**首次**
   * 扫码登录成功后，由后端自动以平台昵称覆盖别名（重登不覆盖）。
   */
  add: (label?: string): Promise<MateAccount> =>
    invoke<MateAccount>("mate_account_add", label === undefined ? {} : { label }),

  /** `mate_account_remove` → 删除账号（后端先取消该账号在途登录流程）。 */
  remove: (id: string): Promise<void> => invoke<void>("mate_account_remove", { id }),

  /** `mate_account_rename` → 重命名别名（返回更新后的行）。 */
  rename: (id: string, label: string): Promise<MateAccount> =>
    invoke<MateAccount>("mate_account_rename", { id, label }),

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
