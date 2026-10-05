//! Tauri command modules. Each module groups related `#[tauri::command]`
//! functions. Commands are registered in `main.rs` via
//! `tauri::generate_handler![]`. Function names use snake_case (Tauri 2.x
//! does not accept colons); the frontend (P4.6) maps the old `ns:action`
//! IPC names to these.

pub mod activity;
pub mod archive;
pub mod bind_creator;
pub mod auto_message;
pub mod auto_popup;
pub mod auto_reply;
pub mod business_accounts;
pub mod comment_listener;
pub mod companion;
pub mod dialog;
pub mod extensions;
pub mod fingerprint;
pub mod groups;
pub mod huibo_live;
pub mod kuaishou_auth;
pub mod jinniu;
pub mod jinniu_promote;
pub mod kuaishou_identity;
pub mod live_launch;
pub mod live_room_monitor;
pub mod mate_login;
pub mod kuaishou_init;
pub mod native_text;
pub mod profiles;
pub mod proxy;
pub mod scene_play;
pub mod settings;
pub mod shop_helper;
pub mod shop_product_script;
pub mod sub_account;
pub mod system;
pub mod update;
