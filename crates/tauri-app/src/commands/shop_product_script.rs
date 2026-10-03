//! Thin IPC adapters for the jieger 商品话术库; SQLite and validation stay
//! on the launcher thread behind `TauriBrowserDriver` helpers.
use crate::AppState;
use profile_manager::{
    AddShopProductScriptLineInput, CreateShopProductScriptInput, ShopProductScript,
    ShopProductScriptDetail, ShopProductScriptLine, UpdateShopProductScriptInput,
    UpdateShopProductScriptLineInput,
};
use tauri::State;

fn shop_product_script_error(error: multizen_core::MultizenError) -> String {
    format!("商品话术库操作失败：{error}。请核对输入及话术行内容、刷新列表后重试；数据库或线程错误请重启应用后重试。")
}

#[tauri::command]
pub async fn shop_product_scripts_list(
    state: State<'_, AppState>,
) -> Result<Vec<ShopProductScript>, String> {
    state
        .driver
        .shop_product_scripts_list()
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_get(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<ShopProductScriptDetail>, String> {
    state
        .driver
        .shop_product_script_get(&id)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_create(
    state: State<'_, AppState>,
    input: CreateShopProductScriptInput,
) -> Result<ShopProductScript, String> {
    state
        .driver
        .shop_product_script_create(input)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_update(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateShopProductScriptInput,
) -> Result<ShopProductScript, String> {
    state
        .driver
        .shop_product_script_update(&id, patch)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .driver
        .shop_product_script_delete(&id)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_add_line(
    state: State<'_, AppState>,
    input: AddShopProductScriptLineInput,
) -> Result<ShopProductScriptLine, String> {
    state
        .driver
        .shop_product_script_add_line(input)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_update_line(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateShopProductScriptLineInput,
) -> Result<ShopProductScriptLine, String> {
    state
        .driver
        .shop_product_script_update_line(&id, patch)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_delete_line(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .driver
        .shop_product_script_delete_line(&id)
        .await
        .map_err(shop_product_script_error)
}

#[tauri::command]
pub async fn shop_product_script_reorder_lines(
    state: State<'_, AppState>,
    script_id: String,
    ordered_ids: Vec<String>,
) -> Result<Vec<ShopProductScriptLine>, String> {
    state
        .driver
        .shop_product_script_reorder_lines(&script_id, ordered_ids)
        .await
        .map_err(shop_product_script_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_errors_retain_cause_and_actionable_chinese_context() {
        let message = shop_product_script_error(multizen_core::MultizenError::Mcp(
            "launcher thread closed".into(),
        ));
        assert!(message.contains("launcher thread closed"));
        assert!(message.contains("商品话术库操作失败"));
        assert!(message.contains("重启应用"));
    }
}
