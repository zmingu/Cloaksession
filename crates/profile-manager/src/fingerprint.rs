use multizen_core::{
    ClientHints, DeviceFamily, FingerprintConfig, ScreenSize, WebGlConfig,
};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};

/// Chrome major version baked into the generated UA / client hints. Kept in one
/// place so a browser bump is a one-line change.
const CHROME_MAJOR: &str = "148";

/// One coherent hardware persona. Everything that a real device would report
/// together (platform, UA, GPU, screen, cores, memory, DPR) lives in a single
/// entry, so a generated fingerprint is internally consistent instead of a
/// random mix that no real machine would have.
struct DeviceProfile {
    family: DeviceFamily,
    /// `navigator.platform`.
    platform: &'static str,
    ua: &'static str,
    /// `sec-ch-ua-platform` (and its version).
    ch_platform: &'static str,
    ch_platform_version: &'static str,
    ch_arch: &'static str,
    gpu_vendor: &'static str,
    gpu_renderer: &'static str,
    /// Candidate native screen sizes; one is picked per generation.
    screens: &'static [(u32, u32)],
    /// Candidate core counts.
    cores: &'static [u32],
    /// Candidate memory (GB).
    memory: &'static [u32],
    dpr: f64,
    fonts_dir: Option<&'static str>,
}

const WINDOWS_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";
const MACOS_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";
const LINUX_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";

/// The full persona catalog — one entry per [`DeviceFamily`]. Kept in sync with
/// the family list in `tauri-app/src/commands/fingerprint.rs` and
/// `mcp-server/src/tools.rs`; a family without an entry here cannot be chosen.
const DEVICE_PROFILES: &[DeviceProfile] = &[
    // --- macOS (Apple Silicon, DPR 2) -------------------------------------
    DeviceProfile {
        family: DeviceFamily::MacbookPro14M3,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3, Unspecified Version)",
        screens: &[(1512, 982), (1440, 900)],
        cores: &[8, 10],
        memory: &[8, 16],
        dpr: 2.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::MacbookPro14M3Pro,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3 Pro, Unspecified Version)",
        screens: &[(1512, 982), (1728, 1117)],
        cores: &[11, 12],
        memory: &[18, 36],
        dpr: 2.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::MacbookPro16M3Pro,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3 Pro, Unspecified Version)",
        screens: &[(1728, 1117), (2056, 1329)],
        cores: &[12],
        memory: &[18, 36],
        dpr: 2.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::MacbookAir13M3,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3, Unspecified Version)",
        screens: &[(1280, 832), (1440, 900)],
        cores: &[8],
        memory: &[8, 16],
        dpr: 2.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::MacbookAir15M3,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3, Unspecified Version)",
        screens: &[(1280, 832), (1440, 900)],
        cores: &[8],
        memory: &[8, 16],
        dpr: 2.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::Imac24M3,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M3, Unspecified Version)",
        screens: &[(2560, 1440), (2880, 1620)],
        cores: &[8, 10],
        memory: &[8, 16],
        dpr: 2.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::MacMiniM2,
        platform: "MacIntel",
        ua: MACOS_UA,
        ch_platform: "macOS",
        ch_platform_version: "14.0.0",
        ch_arch: "arm",
        gpu_vendor: "Google Inc. (Apple)",
        gpu_renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)",
        screens: &[(1920, 1080), (2560, 1440)],
        cores: &[8, 10],
        memory: &[8, 16],
        dpr: 2.0,
        fonts_dir: None,
    },
    // --- Windows (DPR 1) ---------------------------------------------------
    DeviceProfile {
        family: DeviceFamily::WindowsLaptopIntel,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (Intel)",
        gpu_renderer: "ANGLE (Intel, Intel(R) Iris(R) Xe Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (1536, 864)],
        cores: &[8, 12],
        memory: &[8, 16],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsLaptopIntelUhd,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (Intel)",
        gpu_renderer: "ANGLE (Intel, Intel(R) UHD Graphics 620 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (1366, 768)],
        cores: &[4, 8],
        memory: &[8, 16],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsLaptopAmd,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (AMD)",
        gpu_renderer: "ANGLE (AMD, AMD Radeon(TM) Graphics Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080)],
        cores: &[8, 16],
        memory: &[8, 16],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsLaptopNvidia,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (NVIDIA)",
        gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3050 Laptop GPU Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (2560, 1440)],
        cores: &[12, 16],
        memory: &[16, 32],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsLaptopNvidia4050,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (NVIDIA)",
        gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 4050 Laptop GPU Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (2560, 1440)],
        cores: &[12, 16],
        memory: &[16, 32],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsDesktopNvidia,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (NVIDIA)",
        gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (2560, 1440)],
        cores: &[12, 16],
        memory: &[16, 32],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsDesktopNvidia4080,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (NVIDIA)",
        gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 4080 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(2560, 1440), (3840, 2160)],
        cores: &[16, 24],
        memory: &[32, 64],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsDesktopAmd,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (AMD)",
        gpu_renderer: "ANGLE (AMD, AMD Radeon RX 6600 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (2560, 1440)],
        cores: &[12, 16],
        memory: &[16, 32],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    DeviceProfile {
        family: DeviceFamily::WindowsDesktopIntel,
        platform: "Win32",
        ua: WINDOWS_UA,
        ch_platform: "Windows",
        ch_platform_version: "10.0.0",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (Intel)",
        gpu_renderer: "ANGLE (Intel, Intel(R) UHD Graphics 630 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (1600, 900)],
        cores: &[8],
        memory: &[8, 16],
        dpr: 1.0,
        fonts_dir: Some(r"C:\Windows\Fonts"),
    },
    // --- Linux (DPR 1) -----------------------------------------------------
    DeviceProfile {
        family: DeviceFamily::LinuxDesktopIntel,
        platform: "Linux x86_64",
        ua: LINUX_UA,
        ch_platform: "Linux",
        ch_platform_version: "",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (Intel)",
        gpu_renderer: "ANGLE (Intel, Mesa Intel(R) UHD Graphics 630 (CFL GT2), OpenGL 4.6)",
        screens: &[(1920, 1080)],
        cores: &[8],
        memory: &[8, 16],
        dpr: 1.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::LinuxDesktopAmd,
        platform: "Linux x86_64",
        ua: LINUX_UA,
        ch_platform: "Linux",
        ch_platform_version: "",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (AMD)",
        gpu_renderer: "ANGLE (AMD, AMD Radeon RX 580 Series (POLARIS10, DRM 3.42.0), OpenGL 4.6)",
        screens: &[(1920, 1080)],
        cores: &[8, 16],
        memory: &[8, 16],
        dpr: 1.0,
        fonts_dir: None,
    },
    DeviceProfile {
        family: DeviceFamily::LinuxDesktopNvidia,
        platform: "Linux x86_64",
        ua: LINUX_UA,
        ch_platform: "Linux",
        ch_platform_version: "",
        ch_arch: "x86",
        gpu_vendor: "Google Inc. (NVIDIA)",
        gpu_renderer: "ANGLE (NVIDIA, NVIDIA GeForce GTX 1660 SUPER Direct3D11 vs_5_0 ps_5_0, D3D11)",
        screens: &[(1920, 1080), (2560, 1440)],
        cores: &[12, 16],
        memory: &[16, 32],
        dpr: 1.0,
        fonts_dir: None,
    },
];

/// Non-empty entropy for the "random" path (empty seed): process id + wall
/// clock + a monotonic counter, so two calls never collide.
fn fresh_entropy() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{n}", std::process::id())
}

/// Pick one item deterministically from `items`, using 4 bytes of the digest at
/// slot `index` (each slot has independent bytes, so successive picks vary
/// independently rather than all tracking the first byte).
fn pick<'a, T>(digest: &[u8; 32], index: usize, items: &'a [T]) -> &'a T {
    let offset = (index * 4) % 32;
    let n = u32::from_le_bytes([
        digest[offset],
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]) as usize;
    &items[n % items.len()]
}

/// Build a fingerprint.
///
/// - **Empty seed** → fresh entropy per call, so the UI's "random" button yields
///   a different (but internally coherent) persona every time.
/// - **Non-empty seed** (a profile id) → deterministic: the same seed always
///   yields the same fingerprint, so re-generation stays stable per profile.
///
/// The chosen persona is drawn from [`DEVICE_PROFILES`]: platform, UA, client
/// hints, GPU, screen, cores, memory and DPR all come from one real device, so
/// the result is self-consistent. `locale`/`timezone`/`country` stay at a
/// neutral `en-US` / `US` default — the caller realigns them to the proxy's exit
/// region (see `fingerprint_reconcile`), which avoids an IP-vs-locale mismatch.
pub fn default_fingerprint(seed: &str) -> FingerprintConfig {
    let entropy = if seed.trim().is_empty() {
        fresh_entropy()
    } else {
        seed.to_string()
    };
    let digest: [u8; 32] = Sha256::digest(entropy.as_bytes()).into();

    let profile = pick(&digest, 0, DEVICE_PROFILES);
    let screen = pick(&digest, 1, profile.screens);
    let cores = *pick(&digest, 2, profile.cores);
    let memory = *pick(&digest, 3, profile.memory);

    // Visible-area reserve below the screen: taskbar (Windows), menu bar (macOS)
    // or top bar (Linux). Only Windows uses it as an explicit launch arg, but
    // storing it keeps the reported `availScreen` coherent on every platform.
    let reserve = match profile.ch_platform {
        "Windows" => 40,
        "macOS" => 25,
        _ => 27,
    };

    let sec_ch_ua = format!(
        "\"Chromium\";v=\"{v}\", \"Google Chrome\";v=\"{v}\", \"Not?A_Brand\";v=\"99\"",
        v = CHROME_MAJOR
    );
    let sec_ch_ua_full_version_list = format!(
        "\"Chromium\";v=\"{v}.0.0.0\", \"Google Chrome\";v=\"{v}.0.0.0\", \"Not?A_Brand\";v=\"99.0.0.0\"",
        v = CHROME_MAJOR
    );

    FingerprintConfig {
        device: profile.family,
        user_agent: profile.ua.into(),
        platform: profile.platform.into(),
        client_hints: ClientHints {
            sec_ch_ua,
            sec_ch_ua_platform: profile.ch_platform.into(),
            sec_ch_ua_platform_version: profile.ch_platform_version.into(),
            sec_ch_ua_arch: profile.ch_arch.into(),
            sec_ch_ua_bitness: "64".into(),
            sec_ch_ua_mobile: "?0".into(),
            sec_ch_ua_model: "".into(),
            sec_ch_ua_full_version_list,
        },
        locale: "en-US".into(),
        languages: vec!["en-US".into(), "en".into()],
        accept_language: "en-US,en;q=0.9".into(),
        timezone: "America/New_York".into(),
        country: "US".into(),
        screen: ScreenSize {
            width: screen.0,
            height: screen.1,
        },
        avail_screen: Some(ScreenSize {
            width: screen.0,
            height: screen.1.saturating_sub(reserve),
        }),
        dpr: profile.dpr,
        webgl: WebGlConfig {
            vendor: profile.gpu_vendor.into(),
            renderer: profile.gpu_renderer.into(),
        },
        hardware_concurrency: cores,
        device_memory: memory,
        fonts_dir: profile.fonts_dir.map(|dir| dir.to_string()),
        storage_quota: Some(2_000_000_000),
        seed: Some(entropy),
    }
}
