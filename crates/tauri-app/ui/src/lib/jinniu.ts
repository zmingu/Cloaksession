/**
 * 磁力金牛账户管理 IPC (commands/jinniu.rs — 8 commands + 2 events).
 *
 * 账户模型：启动 / 停止。启动打开金牛浏览器（复用登录态或扫码），并自动停止
 * 其它账户会话（启动即单选）。wire 状态机保留 jieger 五态，但 UI 只呈现
 * 未启动/启动中/已启动/错误——connected 与 awaiting-sub-account 都是「已启动」，
 * 选子户不是可见阶段。
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

  /**
   * `jinniu_account_add` → 新建账户（无需手输名称：先落「未命名金牛」占位行，
   * 登录识别到右上角主账号后自动命名；首个账户自动置为活跃）。
   */
  add: (): Promise<JinniuAccountWithStatus> =>
    invoke<JinniuAccountWithStatus>("jinniu_account_add", { label: null }),

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
   * `jinniu_login` → 启动账户会话：打开金牛浏览器（复用登录态或扫码），
   * 同时停止其它账户会话并把本账户置为当前。返回即时快照，后续状态由
   * `jinniu-status-changed` 推送。
   */
  login: (id: string, options?: JinniuLoginOptions): Promise<JinniuStatePayload> =>
    invoke<JinniuStatePayload>("jinniu_login", { id, options: options ?? null }),

  /** `jinniu_disconnect` → 停止指定账户会话。 */
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
