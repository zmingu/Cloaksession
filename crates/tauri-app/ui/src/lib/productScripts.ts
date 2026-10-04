import { invoke } from "@tauri-apps/api/core";
import type {
  AddShopProductScriptLineInput,
  CreateShopProductScriptInput,
  ShopProductScript,
  ShopProductScriptDetail,
  ShopProductScriptLine,
  UpdateShopProductScriptInput,
  UpdateShopProductScriptLineInput,
} from "../types";

/**
 * 商品话术库 (jieger `shopProductScript`, 264 行) 的 TS 封装。
 *
 * 9 commands (crates/tauri-app/src/commands/shop_product_script.rs):
 * - shop_product_scripts_list / get / create / update / delete
 * - shop_product_script_add_line / update_line / delete_line / reorder_lines
 *
 * Arg keys are camelCase on the wire (`scriptId`, `orderedIds`); the Rust
 * side declares snake_case (`script_id`, `ordered_ids`) and Tauri maps them.
 * `None` comes back as `null`.
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
};
