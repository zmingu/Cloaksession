/**
 * 磁力金牛大户管理 IPC (commands/jinniu.rs — 8 commands + 2 events).
 *
 * 多大户但单选切换：同时只允许一个大户会话存活。扫码完成 ≠ connected——
 * 只有用户在浏览器弹窗中手动选子户、URL 出现 `__accountId__` 才置 connected。
 *
 * Invoke channel = snake_case Rust function name; argument keys are camelCase
 * (repo-wide Tauri convention, see `lib/ipc.ts`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  JinniuAccountWithStatus,
  JinniuLoginOptions,
  JinniuStatePayload,
} from "../types";

export type {
  JinniuAccountWithStatus,
  JinniuLoginOptions,
  JinniuMaster,
  JinniuStatePayload,
  JinniuStatus,
} from "../types";

/** 状态变化推送事件（payload = `JinniuStatePayload`）。 */
export const JINNIU_STATUS_CHANGED = "jinniu-status-changed";
/** 大户列表变化推送事件（无 payload，收到后刷新列表）。 */
export const JINNIU_ACCOUNTS_CHANGED = "jinniu-accounts-changed";

export const jinniu = {
  /** `jinniu_accounts_list` → 全部金牛大户（含运行时状态与活跃标记）。 */
  list: (): Promise<JinniuAccountWithStatus[]> =>
    invoke<JinniuAccountWithStatus[]>("jinniu_accounts_list"),

  /** `jinniu_account_add` → 新建大户（首个大户自动置为活跃）。 */
  add: (label: string): Promise<JinniuAccountWithStatus> =>
    invoke<JinniuAccountWithStatus>("jinniu_account_add", { label }),

  /** `jinniu_account_remove` → 删除大户（断开 + 清环境 + 删记录 + 活跃补位）。 */
  remove: (id: string): Promise<void> =>
    invoke<void>("jinniu_account_remove", { id }),

  /** `jinniu_account_set_active` → 单选切换（`null` 清空活跃），返回新活跃 ID。 */
  setActive: (id: string | null): Promise<string | null> =>
    invoke<string | null>("jinniu_account_set_active", { id }),

  /** `jinniu_account_get_active` → 当前活跃大户 ID。 */
  getActive: (): Promise<string | null> =>
    invoke<string | null>("jinniu_account_get_active"),

  /**
   * `jinniu_login` → 打开/驱动金牛登录流程。扫码后仍需用户手动选子户；
   * 返回即时快照，后续状态由 `jinniu-status-changed` 推送。
   */
  login: (id: string, options?: JinniuLoginOptions): Promise<JinniuStatePayload> =>
    invoke<JinniuStatePayload>("jinniu_login", { id, options: options ?? null }),

  /** `jinniu_disconnect` → 断开指定大户会话。 */
  disconnect: (id: string): Promise<JinniuStatePayload> =>
    invoke<JinniuStatePayload>("jinniu_disconnect", { id }),

  /** `jinniu_status` → 读取指定大户的状态快照。 */
  status: (id: string): Promise<JinniuStatePayload> =>
    invoke<JinniuStatePayload>("jinniu_status", { id }),
};

/** 订阅 `jinniu-status-changed`（await 注册/清理）。 */
export function onJinniuStatusChanged(
  cb: (payload: JinniuStatePayload) => void,
): Promise<UnlistenFn> {
  return listen<JinniuStatePayload>(JINNIU_STATUS_CHANGED, (event) => {
    cb(event.payload);
  });
}

/** 订阅 `jinniu-accounts-changed`（无 payload；收到后刷新列表）。 */
export function onJinniuAccountsChanged(cb: () => void): Promise<UnlistenFn> {
  return listen(JINNIU_ACCOUNTS_CHANGED, () => {
    cb();
  });
}
