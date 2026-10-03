//! 跟播助手上车 IPC 适配层（`zs.kwaixiaodian.com/page/helper`）。
//! 与 CPS 加货架命令严格隔离：本文件只转发 `driver::shop_helper` 方法。
//! 上车/下车成功后 emit `shop-helper:goods-changed`，供前端刷新商品列表。

use crate::driver::shop_helper::{HelperGoodActionResult, HelperGoodInfo};
use crate::AppState;
use cdp_driver::TaskCancel;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub async fn shop_helper_read_goods(
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
    tab: String,
) -> Result<Vec<HelperGoodInfo>, String> {
    state
        .driver
        .shop_helper_read_goods(&profile_id, &target_id, &tab, TaskCancel::new())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn shop_helper_switch_tab(
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
    tab: String,
) -> Result<Vec<HelperGoodInfo>, String> {
    state
        .driver
        .shop_helper_switch_tab(&profile_id, &target_id, &tab, TaskCancel::new())
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ShopHelperGoodsChanged {
    pub profile_id: String,
    pub goods_id: String,
    pub action: &'static str,
    pub ok: bool,
}

async fn emit_change(
    app: &AppHandle,
    profile_id: &str,
    result: &HelperGoodActionResult,
    action: &'static str,
) {
    let _ = app.emit(
        "shop-helper:goods-changed",
        ShopHelperGoodsChanged {
            profile_id: profile_id.to_string(),
            goods_id: result.goods_id.clone(),
            action,
            ok: result.ok,
        },
    );
}

#[tauri::command]
pub async fn shop_helper_add_to_cart(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
    goods_id: String,
) -> Result<HelperGoodActionResult, String> {
    let result = state
        .driver
        .shop_helper_add_to_cart(&profile_id, &target_id, &goods_id, TaskCancel::new())
        .await
        .map_err(|e| e.to_string())?;
    emit_change(&app, &profile_id, &result, "on").await;
    Ok(result)
}

#[tauri::command]
pub async fn shop_helper_remove_from_cart(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    target_id: String,
    goods_id: String,
) -> Result<HelperGoodActionResult, String> {
    let result = state
        .driver
        .shop_helper_remove_from_cart(&profile_id, &target_id, &goods_id, TaskCancel::new())
        .await
        .map_err(|e| e.to_string())?;
    emit_change(&app, &profile_id, &result, "off").await;
    Ok(result)
}
