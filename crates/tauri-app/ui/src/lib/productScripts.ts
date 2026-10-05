import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddShopProductScriptLineInput,
  CreateShopProductScriptInput,
  ProductScriptPlayState,
  ShopProductScript,
  ShopProductScriptDetail,
  ShopProductScriptLine,
  ShopScriptPlayback,
  UpdateShopProductScriptInput,
  UpdateShopProductScriptLineInput,
} from "../types";

/**
 * 商品话术库 (jieger `shopProductScript`, 264 行) 的 TS 封装。
 *
 * 11 commands (crates/tauri-app/src/commands/shop_product_script.rs):
 * - shop_product_scripts_list / get / create / update / delete
 * - shop_product_script_add_line / update_line / delete_line / reorder_lines
 * - shop_product_script_play / stop
 *
 * Arg keys are camelCase on the wire (`scriptId`, `orderedIds`); the Rust
 * side declares snake_case (`script_id`, `ordered_ids`) and Tauri maps them.
 * `None` comes back as `null`.
 *
 * 播放（`play`）是真实写动作（上/下车 + 讲解/取消讲解）：UI 必须二次确认。
 * 播放状态经 `product-script-state-changed` 推送。
 */
export const productScripts = {
  /** `shop_product_scripts_list` → `Vec<ShopProductScript>` (updatedAt desc). */
  list: (): Promise<ShopProductScript[]> =>
    invoke<ShopProductScript[]>("shop_product_scripts_list"),

  /** `shop_product_script_get` → `Option<ShopProductScriptDetail>` (null = missing). */
  get: (id: string): Promise<ShopProductScriptDetail | null> =>
    invoke<ShopProductScriptDetail | null>("shop_product_script_get", { id }),

  /** `shop_product_script_create` → `ShopProductScript`. */
  create: (input: CreateShopProductScriptInput): Promise<ShopProductScript> =>
    invoke<ShopProductScript>("shop_product_script_create", { input }),

  /**
   * `shop_product_script_update` → `ShopProductScript`.
   * `patch.description` is tri-state: `undefined` = keep, `null` = clear, string = set.
   */
  update: (id: string, patch: UpdateShopProductScriptInput): Promise<ShopProductScript> =>
    invoke<ShopProductScript>("shop_product_script_update", { id, patch }),

  /** `shop_product_script_delete` → `()`. Missing id counts as success. */
  delete: (id: string): Promise<void> =>
    invoke<void>("shop_product_script_delete", { id }),

  /** `shop_product_script_add_line` → `ShopProductScriptLine`. */
  addLine: (input: AddShopProductScriptLineInput): Promise<ShopProductScriptLine> =>
    invoke<ShopProductScriptLine>("shop_product_script_add_line", { input }),

  /**
   * `shop_product_script_update_line` → `ShopProductScriptLine`.
   * `patch.goodsName` is tri-state: `undefined` = keep, `null` = clear, string = set.
   */
  updateLine: (id: string, patch: UpdateShopProductScriptLineInput): Promise<ShopProductScriptLine> =>
    invoke<ShopProductScriptLine>("shop_product_script_update_line", { id, patch }),

  /** `shop_product_script_delete_line` → `()`. Missing id counts as success. */
  deleteLine: (id: string): Promise<void> =>
    invoke<void>("shop_product_script_delete_line", { id }),

  /**
   * `shop_product_script_reorder_lines` → reordered `Vec<ShopProductScriptLine>`.
   * `orderedIds` must be exactly the script's full line-id set (no gaps/dups/foreign ids).
   */
  reorderLines: (scriptId: string, orderedIds: string[]): Promise<ShopProductScriptLine[]> =>
    invoke<ShopProductScriptLine[]>("shop_product_script_reorder_lines", {
      scriptId,
      orderedIds,
    }),

  /**
   * `shop_product_script_play` → 播放句柄。**真实写动作**（上/下车 + 讲解/取消讲解），
   * UI 必须二次确认。`goodsIds` 缺省播放全部行；`startedAtMs` 缺省用当前墙钟。
   * 播放状态经 `product-script-state-changed` 广播。
   */
  play: (
    profileId: string,
    scriptId: string,
    goodsIds?: string[] | null,
    startedAtMs?: number | null,
  ): Promise<ShopScriptPlayback> =>
    invoke<ShopScriptPlayback>("shop_product_script_play", {
      profileId,
      scriptId,
      goodsIds: goodsIds ?? null,
      startedAtMs: startedAtMs ?? null,
    }),

  /** `shop_product_script_stop` → 是否确有正在播放的脚本（未播放也成功）。 */
  stop: (scriptId: string): Promise<boolean> =>
    invoke<boolean>("shop_product_script_stop", { scriptId }),
};

/**
 * `product-script-state-changed` 推送事件：播放状态快照。
 * 返回 `Promise<UnlistenFn>`，调用方负责在卸载时注销。
 */
export function onProductScriptState(
  cb: (state: ProductScriptPlayState) => void,
): Promise<UnlistenFn> {
  return listen<ProductScriptPlayState>("product-script-state-changed", (event) => {
    cb(event.payload);
  });
}
