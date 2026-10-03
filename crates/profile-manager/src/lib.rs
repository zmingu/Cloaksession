mod business_accounts;
pub mod fingerprint;
mod kuaishou_account;
mod kuaishou_identity;
mod auto_reply;
mod jinniu_authorize;
mod kuaishou_init;
pub mod scenes;
pub mod live_events;
pub mod manager;
pub mod shop_product_script;
pub mod migrate;
pub mod row;

pub use auto_reply::AutoReplyRecord;
// Re-export authorize types at the crate root for driver/IPC use.
pub use jinniu_authorize::{JinniuAuthorizeItem, JinniuAuthorizeRecord};
pub use kuaishou_init::KuaishouInitLease;
pub use manager::ProfileManager;
pub use shop_product_script::{
    AddShopProductScriptLineInput, CreateShopProductScriptInput, ScriptLineAction,
    ShopProductScript, ShopProductScriptDetail, ShopProductScriptLine,
    UpdateShopProductScriptInput, UpdateShopProductScriptLineInput,
};
pub use scenes::{RecordInteractionInput, SubAccountInteraction};
