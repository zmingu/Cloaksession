//! Native dialog commands backed by `tauri-plugin-dialog`. The plugin
//! exposes `AppHandle::dialog()` returning a `DialogExt` extension; from
//! there `.file()` returns a `FileDialogBuilder` with `blocking_pick_file`
//! / `blocking_pick_folder` for use from async or sync contexts. These
//! commands are sync from the frontend's perspective (the plugin handles
//! the platform dialog on a background thread under the hood).
//!
//! User-visible native strings (file-dialog filter labels) come from the
//! static Rust dictionary in `commands::native_text`, keyed by the
//! persisted app language. The settings lock is released before the
//! blocking picker opens.

use std::path::PathBuf;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::native_text::{persisted_language, text, NativeTextKey};
use crate::AppState;

#[tauri::command]
pub async fn dialog_pick_browser_binary(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Option<PathBuf>, String> {
    let lang = persisted_language(&state).await;
    let filter = text(lang, NativeTextKey::DialogBrowserBinaryFilter);
    let path = app
        .dialog()
        .file()
        .add_filter(filter, &["exe", "app", "sh"])
        .blocking_pick_file();
    path.map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

#[tauri::command]
pub async fn dialog_pick_directory(
    _state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Option<PathBuf>, String> {
    let path = app.dialog().file().blocking_pick_folder();
    path.map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}
