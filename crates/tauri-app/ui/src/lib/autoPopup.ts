import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AutoPopUpConfig,
  AutoPopUpConfigPatch,
  AutoPopUpEvent,
  AutoPopUpStatus,
  PopupGoodsInfo,
  PopupScanReport,
  ShortcutRegisterResult,
} from "../types";

/**
 * 自动弹品 (jieger `autoPopUp`) 的 TS 封装。
 *
 * 10 commands (crates/tauri-app/src/commands/auto_popup.rs):
 * - auto_popup_start / stop / status / update_config
 * - auto_popup_goods / scan / explain_once
 * - auto_popup_register_shortcuts / unregister_shortcuts / trigger_shortcut
 *
 * Arg keys are camelCase on the wire (`profileId`, `goodsId`, `accelerator`);
 * the Rust side declares snake_case. `explainGoods` 只在中控页、以主播身份
 * 执行：金牛 / 非小店绑定由后端直接拒绝。
 */
export const autoPopup = {
  /** `auto_popup_start` → 启动并广播 `auto-popup:state`。已在运行则先停旧任务再启动。 */
  start: (profileId: string, config: AutoPopUpConfig): Promise<AutoPopUpStatus> =>
    invoke<AutoPopUpStatus>("auto_popup_start", { profileId, config }),

  /** `auto_popup_stop` → 停止（未运行也成功），广播 `auto-popup:state`。写操作，UI 需二次确认。 */
  stop: (profileId: string, reason?: string): Promise<AutoPopUpStatus> =>
    invoke<AutoPopUpStatus>("auto_popup_stop", {
      profileId,
      reason: reason ?? null,
    }),

  /** `auto_popup_status` → 当前状态快照（同步读表，无副作用）。 */
  status: (profileId: string): Promise<AutoPopUpStatus> =>
    invoke<AutoPopUpStatus>("auto_popup_status", { profileId }),

  /** `auto_popup_update_config` → 热更新运行中配置并重建队列，广播 `auto-popup:state`。 */
  updateConfig: (
    profileId: string,
    patch: AutoPopUpConfigPatch,
  ): Promise<AutoPopUpStatus> =>
    invoke<AutoPopUpStatus>("auto_popup_update_config", { profileId, patch }),

  /** `auto_popup_goods` → 一次性读取中控页商品列表（全行 serial/title/price）。 */
  goods: (profileId: string): Promise<PopupGoodsInfo[]> =>
    invoke<PopupGoodsInfo[]>("auto_popup_goods", { profileId }),

  /** `auto_popup_scan` → 商品知识扫描（读列表 + 与来源比对；空列表时后端报错）。 */
  scan: (profileId: string): Promise<PopupScanReport> =>
    invoke<PopupScanReport>("auto_popup_scan", { profileId }),

  /** `auto_popup_explain_once` → 单次讲解（门禁 + 中控页 + 重试）。写操作，UI 需二次确认。 */
  explainOnce: (profileId: string, goodsId: string): Promise<void> =>
    invoke<void>("auto_popup_explain_once", { profileId, goodsId }),

  /**
   * `auto_popup_register_shortcuts` → 注册“账号 → 快捷键 → 商品”映射。
   * 只做占用校验与内存记录，不做 OS 级全局注册（需用户明确授权后由应用层接线）。
   */
  registerShortcuts: (
    profileId: string,
    bindings: Record<string, string>,
  ): Promise<ShortcutRegisterResult> =>
    invoke<ShortcutRegisterResult>("auto_popup_register_shortcuts", {
      profileId,
      bindings,
    }),

  /** `auto_popup_unregister_shortcuts` → 注销该账号的全部快捷键。 */
  unregisterShortcuts: (profileId: string): Promise<void> =>
    invoke<void>("auto_popup_unregister_shortcuts", { profileId }),

  /** `auto_popup_trigger_shortcut` → 查表得商品序号并立即讲解一次，返回命中的 goodsId。 */
  triggerShortcut: (profileId: string, accelerator: string): Promise<string> =>
    invoke<string>("auto_popup_trigger_shortcut", { profileId, accelerator }),
};

/**
 * `auto-popup:state` 推送事件：每个命令直接 emit 的运行状态快照。
 */
export function onAutoPopupState(
  cb: (status: AutoPopUpStatus) => void,
): Promise<UnlistenFn> {
  return listen<AutoPopUpStatus>("auto-popup:state", (event) => {
    cb(event.payload);
  });
}

/**
 * `auto-popup:event` 推送事件：运行事件桥接
 * (`started` / `stopped` / `explained` / `explain-failed` / `config-updated` / `shortcut-triggered`)。
 */
export function onAutoPopupEvent(
  cb: (event: AutoPopUpEvent) => void,
): Promise<UnlistenFn> {
  return listen<AutoPopUpEvent>("auto-popup:event", (event) => {
    cb(event.payload);
  });
}
