//! Browser directory policy, shared by real launch inputs and business isolation checks.
//! No directories are created here. Unsupported/ambiguous paths fail closed.
use multizen_core::{BrowserEngine, ChromixSettings, MultizenError, Profile, Result};
use serde_json::{Map, Value};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub fn default_data_dir(profile: &Profile, engine: BrowserEngine) -> PathBuf {
    let root = Path::new(&profile.data_dir);
    match engine {
        BrowserEngine::Cft => root.to_path_buf(),
        BrowserEngine::Cloakbrowser => root.join("engines").join("cloakbrowser"),
        BrowserEngine::Chromix => root.join("engines").join("chromix"),
    }
}

fn invalid(message: impl std::fmt::Display) -> MultizenError {
    MultizenError::Config(format!("无法验证业务数据目录隔离：{message}"))
}

/// `config` must already be the exact shallow-merged config passed to the bridge.
/// Preserve opaque SDK options; only interpret directory-affecting host contracts.
pub fn effective_data_dir(
    profile: &Profile,
    engine: BrowserEngine,
    config: &ChromixSettings,
) -> Result<PathBuf> {
    if engine != BrowserEngine::Chromix {
        return Ok(default_data_dir(profile, engine));
    }
    validate_layer(&config.options, false)?;
    for name in ["launchOptions", "contextOptions"] {
        if let Some(value) = config.options.get(name) {
            let layer = value
                .as_object()
                .ok_or_else(|| invalid(format!("{name}必须是对象")))?;
            validate_layer(layer, true)?;
        }
    }
    match config.options.get("userDataDir") {
        None => Ok(default_data_dir(profile, engine)),
        Some(Value::String(path)) if !path.trim().is_empty() => Ok(PathBuf::from(path)),
        _ => Err(invalid("Chromix userDataDir必须是非空字符串")),
    }
}

fn validate_layer(layer: &Map<String, Value>, nested: bool) -> Result<()> {
    if nested && layer.contains_key("userDataDir") {
        return Err(invalid(
            "嵌套userDataDir不被Chromix桥支持，请使用顶层userDataDir",
        ));
    }
    for key in ["args", "ignoreDefaultArgs"] {
        let Some(value) = layer.get(key) else {
            continue;
        };
        if key == "ignoreDefaultArgs" && value.is_boolean() {
            continue;
        }
        let args = value
            .as_array()
            .ok_or_else(|| invalid(format!("{key}必须是字符串数组")))?;
        for value in args {
            let arg = value
                .as_str()
                .ok_or_else(|| invalid(format!("{key}必须是字符串数组")))?
                .trim()
                .to_ascii_lowercase();
            let flag = arg
                .trim_start_matches('-')
                .split(['=', ' ', '\t', '\n', '\r'])
                .next()
                .unwrap_or("");
            if arg.starts_with('-') && (flag == "user-data-dir" || flag == "profile-directory") {
                return Err(invalid(
                    "禁止raw user-data-dir/profile-directory参数，请使用顶层userDataDir",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct VerifiedDataDir {
    components: Vec<OsString>,
}

impl VerifiedDataDir {
    pub fn overlaps(&self, other: &Self) -> bool {
        self.components
            .iter()
            .zip(&other.components)
            .all(|(a, b)| component_eq(a, b))
    }
    pub fn same_directory(&self, other: &Self) -> bool {
        self.components.len() == other.components.len() && self.overlaps(other)
    }
}

#[cfg(windows)]
fn component_eq(a: &OsString, b: &OsString) -> bool {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn CompareStringOrdinal(
            a: *const u16,
            a_len: i32,
            b: *const u16,
            b_len: i32,
            ignore_case: i32,
        ) -> i32;
    }
    let a: Vec<u16> = a.encode_wide().collect();
    let b: Vec<u16> = b.encode_wide().collect();
    // Paths are bounded far below i32::MAX. Windows ordinal casing, not Unicode lowercase heuristics.
    unsafe { CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1) == 2 }
}
#[cfg(not(windows))]
fn component_eq(a: &OsString, b: &OsString) -> bool {
    a == b
}

/// Resolve existing components (including junctions/symlinks) and append the missing suffix.
/// `..` across a reparse point is refused: Windows/SDK lexical resolution can differ from POSIX.
pub fn verify_data_dir(path: &Path) -> Result<VerifiedDataDir> {
    let path = portable_path(path)?;
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let has_parent = absolute.components().any(|c| c == Component::ParentDir);
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) => resolved.push(component.as_os_str()),
            Component::RootDir => {
                resolved.push(component.as_os_str());
                resolved = portable_path(
                    &std::fs::canonicalize(&resolved)
                        .map_err(|e| invalid(format!("根目录无法解析：{e}")))?,
                )?;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() {
                    return Err(invalid("路径越过根目录"));
                }
            }
            Component::Normal(name) => {
                validate_name(name)?;
                resolved.push(name);
                match std::fs::symlink_metadata(&resolved) {
                    Ok(meta) => {
                        #[cfg(windows)]
                        let reparse = {
                            use std::os::windows::fs::MetadataExt;
                            meta.file_attributes() & 0x400 != 0
                        };
                        #[cfg(not(windows))]
                        let reparse = meta.file_type().is_symlink();
                        if has_parent && reparse {
                            return Err(invalid(
                                "含符号链接/重解析点的路径不能同时使用..；请填写规范绝对路径",
                            ));
                        }
                        let canonical = std::fs::canonicalize(&resolved)
                            .map_err(|e| invalid(format!("路径解析失败：{e}")))?;
                        if !std::fs::metadata(&canonical)
                            .map_err(|e| invalid(format!("目录不可读取：{e}")))?
                            .is_dir()
                        {
                            return Err(invalid("数据路径或其父项不是目录"));
                        }
                        resolved = portable_path(&canonical)?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(invalid(format!("目录无法读取：{e}"))),
                }
            }
        }
    }
    if !resolved.is_absolute() {
        return Err(invalid("必须能够解析为绝对路径"));
    }
    Ok(VerifiedDataDir {
        components: resolved
            .components()
            .map(|c| c.as_os_str().to_owned())
            .collect(),
    })
}

fn portable_path(path: &Path) -> Result<PathBuf> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid("路径必须是有效Unicode"))?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(invalid("路径为空或包含控制字符"));
    }
    if text.encode_utf16().count() > 32_767 {
        return Err(invalid("路径超过支持的长度"));
    }
    #[cfg(windows)]
    {
        let text = text.replace('/', "\\");
        let extended = text.starts_with("\\\\?\\");
        let text = text.strip_prefix("\\\\?\\").unwrap_or(&text);
        let bytes = text.as_bytes();
        if extended
            && !(bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\')
        {
            return Err(invalid("extended路径仅支持本地盘符绝对路径"));
        }
        if text.starts_with('\\') || text.starts_with("UNC\\") {
            return Err(invalid(
                "不支持UNC、设备路径或根相对路径；请使用本地磁盘绝对路径",
            ));
        }
        if text.contains(':')
            && !(bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\'
                && !text[2..].contains(':'))
        {
            return Err(invalid("不支持盘符相对路径、设备路径或备用数据流"));
        }
        Ok(PathBuf::from(text))
    }
    #[cfg(not(windows))]
    {
        Ok(path.to_path_buf())
    }
}

fn validate_name(name: &std::ffi::OsStr) -> Result<()> {
    #[cfg(windows)]
    {
        let name = name
            .to_str()
            .ok_or_else(|| invalid("路径必须是有效Unicode"))?;
        let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
        if name.ends_with(['.', ' '])
            || name.contains([':', '*', '?', '"', '<', '>', '|'])
            || matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.chars().count() == 4)
        {
            return Err(invalid("不支持尾随点/空格、DOS设备名或特殊路径字符"));
        }
    }
    #[cfg(not(windows))]
    let _ = name;
    Ok(())
}
