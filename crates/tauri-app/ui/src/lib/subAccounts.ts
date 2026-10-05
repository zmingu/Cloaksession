import { invoke } from "@tauri-apps/api/core";

import type {
  BusinessAccount,
  SaveBusinessAccountInput,
} from "./businessAccounts";

/**
 * 小号互动体系 (jieger `tasks/subAccount` port — 1753 lines).
 *
 * Covers all 8 `*_sub_account*` commands registered in
 * `crates/tauri-app/src/commands/sub_account.rs`.
 * Invoke channel = snake_case Rust function name; argument keys are
 * camelCase (repo-wide Tauri convention, see `lib/ipc.ts`).
 *
 * `BusinessAccount` / `SaveBusinessAccountInput` are **reused** from
 * `./businessAccounts` (both camelCase on the wire) — never redefined here.
 * 小号复用 `business_accounts` 存储，不新增账号表。
 */

/** `SubAccountLoginResult` — camelCase (`rename_all = "camelCase"`). */
export interface SubAccountLoginResult {
  accountId: string;
  ok: boolean;
  error?: string | null;
}

/** Wire `SceneLineAction` — lowercase (`rename_all = "lowercase"`). */
export type SubAccountAction = "danmaku" | "like" | "follow";

/** `SubAccountInteraction` — camelCase (`rename_all = "camelCase"`). */
export interface SubAccountInteraction {
  id: number;
  accountId: string;
  sceneId?: number | null;
  action: SubAccountAction;
  message?: string | null;
  liveRoomUrl?: string | null;
  ok: boolean;
  error?: string | null;
  durationMs?: number | null;
  createdAt: number;
}

export const subAccounts = {
  /** `save_sub_account` → created/updated record (upsert by `input.id`). */
  save: (input: SaveBusinessAccountInput): Promise<BusinessAccount> =>
    invoke<BusinessAccount>("save_sub_account", { input }),

  /** `list_sub_accounts` → all viewer sub-account records. */
  list: (): Promise<BusinessAccount[]> =>
    invoke<BusinessAccount[]>("list_sub_accounts"),

  /** `unbind_sub_account` → detaches the record (retained, not deleted). */
  unbind: (id: string): Promise<void> =>
    invoke<void>("unbind_sub_account", { id }),

  /** delete_sub_account → removes the interact record entirely. */
  delete: (id: string): Promise<void> => invoke<void>("delete_sub_account", { id }),

  /** `sub_account_login` → per-account login outcome (never throws on failure). */
  login: (accountId: string): Promise<SubAccountLoginResult> =>
    invoke<SubAccountLoginResult>("sub_account_login", { accountId }),

  /** `batch_login_sub_accounts` → one outcome per requested account. */
  batchLogin: (accountIds: string[]): Promise<SubAccountLoginResult[]> =>
    invoke<SubAccountLoginResult[]>("batch_login_sub_accounts", { accountIds }),

  /** `sub_account_enter_live_room` → drives the account into the live room. */
  enterLiveRoom: (accountId: string, liveUrl: string): Promise<void> =>
    invoke<void>("sub_account_enter_live_room", { accountId, liveUrl }),

  /**
   * `sub_account_send_danmaku` → persisted interaction row.
   * Per-account serialized server-side (`sendLocks` equivalent).
   */
  sendDanmaku: (accountId: string, content: string): Promise<SubAccountInteraction> =>
    invoke<SubAccountInteraction>("sub_account_send_danmaku", { accountId, content }),

  /** `sub_account_interactions` → interaction history for one account. */
  interactions: (accountId: string): Promise<SubAccountInteraction[]> =>
    invoke<SubAccountInteraction[]>("sub_account_interactions", { accountId }),
};
