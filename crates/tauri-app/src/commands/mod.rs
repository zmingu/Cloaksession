//! Tauri command modules. Each module groups related `#[tauri::command]`
//! functions. Commands are registered in `main.rs` via
//! `tauri::generate_handler![]`. Function names use snake_case (Tauri 2.x
//! does not accept colons); the frontend (P4.6) maps the old `ns:action`
//! IPC names to these.

pub mod activity;
pub mod archive;
pub mod business_accounts;
pub mod companion;
pub mod dialog;
pub mod extensions;
pub mod fingerprint;
pub mod groups;
pub mod kuaishou_identity;
pub mod kuaishou_init;
pub mod native_text;
pub mod profiles;
pub mod proxy;
pub mod settings;
pub mod system;
pub mod update;
