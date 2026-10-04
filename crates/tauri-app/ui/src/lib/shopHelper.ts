import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  HelperGoodActionResult,
  HelperGoodInfo,
  HelperGoodTab,
  ShopHelperGoodsChanged,
} from "../types";

/**
 * 跟播助手上车 (jieger `shopHelper`, zs.kwaixiaodian.com/page/helper) 的 TS 封装。
 *
 * 4 commands (crates/tauri-app/src/commands/shop_helper.rs):
 * - shop_helper_read_goods / switch_tab / add_to_cart / remove_from_cart
 *
 * 与 CPS 加货架 (cps.kwaixiaodian.com) 严格隔离：本模块只认跟播助手页。
 * 上车/下车是真实商品上架写动作：UI 层必须二次确认，真号验收单独确认。
 */
export const shopHelper = {
  /** `shop_helper_read_goods` → 当前 Tab 的商品列表。`tab`: `inCart` | `toAdd`。 */
  readGoods: (
    profileId: string,
    targetId: string,
    tab: HelperGoodTab | string,
  ): Promise<HelperGoodInfo[]> =>
    invoke<HelperGoodInfo[]>("shop_helper_read_goods", { profileId, targetId, tab }),

  /** `shop_helper_switch_tab` → 切换 Tab 并重读列表。 */
  switchTab: (
    profileId: string,
    targetId: string,
    tab: HelperGoodTab | string,
  ): Promise<HelperGoodInfo[]> =>
    invoke<HelperGoodInfo[]>("shop_helper_switch_tab", { profileId, targetId, tab }),

  /** `shop_helper_add_to_cart` → 上车（写动作，需二次确认），成功后 emit `shop-helper:goods-changed`。 */
  addToCart: (
    profileId: string,
    targetId: string,
    goodsId: string,
  ): Promise<HelperGoodActionResult> =>
    invoke<HelperGoodActionResult>("shop_helper_add_to_cart", {
      profileId,
      targetId,
      goodsId,
    }),

  /** `shop_helper_remove_from_cart` → 下车（写动作，需二次确认），成功后 emit `shop-helper:goods-changed`。 */
  removeFromCart: (
    profileId: string,
    targetId: string,
    goodsId: string,
  ): Promise<HelperGoodActionResult> =>
    invoke<HelperGoodActionResult>("shop_helper_remove_from_cart", {
      profileId,
      targetId,
      goodsId,
    }),
};

/**
 * `shop-helper:goods-changed` 推送事件。
 * 上车/下车成功后由后端 emit，供前端刷新商品列表。
 */
export function onShopHelperGoodsChanged(
  cb: (change: ShopHelperGoodsChanged) => void,
): Promise<UnlistenFn> {
  return listen<ShopHelperGoodsChanged>("shop-helper:goods-changed", (event) => {
    cb(event.payload);
  });
}
