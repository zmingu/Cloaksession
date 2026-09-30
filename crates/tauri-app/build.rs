fn main() {
    tauri_build::build();
    // Driver unit tests link the native Tauri event runtime (and TaskDialogIndirect).
    // Unlike the app binary, Rust test harnesses need their own Common Controls v6 manifest.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
        // The application/bin-test already links tauri-build's resource.lib manifest.
        println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
    }
}
