# Batch 5 report — native prompt localization (Rust side)

Date: 2026-10-03. Branch: `feat/app-chinese-i18n`. Status: implemented, verified.

## What changed

- New module `crates/tauri-app/src/commands/native_text.rs`: a small static
  Rust dictionary keyed by `multizen_core::AppLanguage` (zh-CN | en). It holds
  only the native-prompt keys this batch needs:
  - `DialogBrowserBinaryFilter` — file-dialog filter label ("Browser binary").
  - `UpdateDownloadDialogTitle` / `UpdateDownloadDialogBody` — the native
    message dialog shown on the non-Windows manual-download path.
- `crates/tauri-app/src/commands/dialog.rs`: `dialog_pick_browser_binary`
  now resolves the persisted language, looks up the filter label, and only
  then opens the blocking picker.
- `crates/tauri-app/src/commands/update.rs`: `update_download` now resolves
  the persisted language and renders the native message dialog title/body
  from the dictionary. The GitHub URL, browser-open commands and all update
  logic are unchanged.
- `crates/tauri-app/src/commands/mod.rs`: registers `pub mod native_text;`.

## Language source (no parallel settings path)

Both call sites use `native_text::persisted_language(&state)`, which locks the
existing `AppState.settings` (`tokio::sync::Mutex<SettingsStore>`), calls the
existing `SettingsStore::load()` getter, copies `AppSettings.language`, and
returns. No new settings path or cache is introduced.

## Lock discipline

`persisted_language` acquires and releases the settings guard inside the
helper before returning. The blocking OS calls (`blocking_pick_file`,
`app.dialog().message(...).title(...)`) therefore run with the settings lock
NOT held. Verified by inspection of both call sites.

## Dictionary coverage

`ALL_KEYS` enumerates every native-prompt key. Unit tests in `native_text.rs`
assert that each key has non-empty, distinct zh-CN and en entries, and that the
download body keeps its `{version}` / `{url}` placeholders in both languages.
`cargo test -p tauri-app --locked native_text` → 2 passed.

## Deliberately NOT translated (design decision 6)

- `UpdateStatus::Error.message` strings (`GitHub API request failed: …`,
  `Download failed: …`, etc.) stay raw/technical. They are stored in
  `UpdateState` and replayed; freezing a translated sentence would show a
  stale language after a UI language switch. The React UI already pairs a
  translated operation label with this raw detail.
- `Err(String)` returns such as `No downloaded installer to install` /
  `Failed to launch installer: …` remain raw for the same reason.
- Shared `multizen_core::MultizenError` `Display` is untouched.

## Exception list — OS-controlled native strings (not app-translatable)

These surfaces are rendered by the OS / installer and follow the OS display
language; the app cannot localize them and this batch does not attempt to:

1. File-picker dialog buttons and chrome (Open / Cancel / Save, title bar,
   sidebar, "All files" grouping) — Windows/macOS/Linux native file dialog.
2. The native message-dialog OK button and window decoration around the update
   download prompt — OS-provided.
3. Windows NSIS installer UI (progress, buttons, finish page) launched by
   `update_install` — third-party/OS installer, not app text.
4. macOS "open in browser" / Windows `start` shell behavior — OS shell.
5. Any OS accessibility / tooltip strings the platform injects.

## Known app-owned native strings OUTSIDE this batch's ownership

Reported, not changed (ownership was restricted to dialog.rs + update.rs):

- `crates/tauri-app/src/commands/archive.rs:221,354` —
  `add_filter("Cloaksession archive", …)`.
- `crates/tauri-app/src/commands/extensions.rs:353,520` —
  `add_filter("Extension", …)`.

Recommend a small follow-up (same `native_text` dictionary + two more keys) to
cover these file-dialog filter labels.

## Verification

- `cargo check --workspace --locked` → Finished (pass).
- `cargo test --workspace --locked` → all suites pass, 0 failed.
- `cargo test -p tauri-app --locked native_text` → 2 passed.
- No change to `crates/tauri-app/Cargo.toml`; no behavior/link/launch-arg
  changes; frontend passes no display text to Rust.