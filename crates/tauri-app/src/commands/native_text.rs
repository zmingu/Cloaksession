//! Small static dictionary for Rust-side native prompt text.
//!
//! Only strings the Rust side itself hands to an OS-native surface live
//! here: file-dialog filter labels and native message-dialog titles/bodies.
//! All React UI copy is owned by the frontend i18n dictionaries. Rust
//! `Result<_, String>` error text and `UpdateStatus::Error.message` stay
//! raw/technical on purpose, so the frontend can pair a translated
//! operation label with the untranslated detail instead of freezing a
//! translated sentence into stored state.
//!
//! OS-provided buttons, window decorations, file-picker chrome and
//! installer UI are NOT translatable here; they follow the OS display
//! language. See the batch report for the full exception list.

use multizen_core::AppLanguage;

use crate::AppState;

/// Every native-prompt string the Rust side renders. Keep this list in
/// sync with the match arms in [`text`]; the unit test below asserts both
/// languages are present for every key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTextKey {
    /// File-dialog filter label for the managed browser executable.
    DialogBrowserBinaryFilter,
    /// Title of the native dialog offering a manual update download.
    UpdateDownloadDialogTitle,
    /// Body of the native update download dialog. `{version}` and `{url}`
    /// are substituted by the caller.
    UpdateDownloadDialogBody,
}

/// All keys, for exhaustive coverage tests.
pub const ALL_KEYS: &[NativeTextKey] = &[
    NativeTextKey::DialogBrowserBinaryFilter,
    NativeTextKey::UpdateDownloadDialogTitle,
    NativeTextKey::UpdateDownloadDialogBody,
];

/// Look up `key` in the static zh-CN / en dictionary.
pub fn text(lang: AppLanguage, key: NativeTextKey) -> &'static str {
    match lang {
        AppLanguage::ZhCn => match key {
            NativeTextKey::DialogBrowserBinaryFilter => "浏览器程序",
            NativeTextKey::UpdateDownloadDialogTitle => "下载更新",
            NativeTextKey::UpdateDownloadDialogBody => {
                "从以下地址下载 Cloaksession {version}：\n{url}"
            }
        },
        AppLanguage::En => match key {
            NativeTextKey::DialogBrowserBinaryFilter => "Browser binary",
            NativeTextKey::UpdateDownloadDialogTitle => "Download update",
            NativeTextKey::UpdateDownloadDialogBody => {
                "Download Cloaksession {version} from:\n{url}"
            }
        },
    }
}

/// Read the persisted app language and release the settings guard before
/// returning, so callers may open a blocking OS dialog afterwards.
pub async fn persisted_language(state: &AppState) -> AppLanguage {
    let mut store = state.settings.lock().await;
    store.load().map(|s| s.language).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use multizen_core::AppLanguage;

    #[test]
    fn every_key_has_en_and_zh_cn_text() {
        for &key in ALL_KEYS {
            let en = text(AppLanguage::En, key);
            let zh = text(AppLanguage::ZhCn, key);
            assert!(!en.trim().is_empty(), "missing en text for {key:?}");
            assert!(!zh.trim().is_empty(), "missing zh-CN text for {key:?}");
            assert_ne!(en, zh, "en and zh-CN must differ for {key:?}");
        }
    }

    #[test]
    fn download_body_keeps_placeholders() {
        for lang in [AppLanguage::En, AppLanguage::ZhCn] {
            let body = text(lang, NativeTextKey::UpdateDownloadDialogBody);
            assert!(body.contains("{version}"), "{lang:?} missing version placeholder");
            assert!(body.contains("{url}"), "{lang:?} missing url placeholder");
        }
    }
}
