use super::extract::trusted_url;
use base64::Engine;
use futures_util::StreamExt;
use multizen_core::{BrowserEngine, ChromixSettings, Profile, ProxyConfig};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) const MAX_BYTES: usize = super::extract::MAX_AVATAR_BYTES;
#[derive(Clone)]
pub(super) enum AvatarRoute {
    Proxy(ProxyConfig),
    Skip,
}
impl AvatarRoute {
    pub fn from_profile(
        profile: &Profile,
        engine: BrowserEngine,
        chromix: &ChromixSettings,
        environment_uncertain: bool,
    ) -> Self {
        // Environment proxy/no_proxy semantics differ between Chromium, SDK and reqwest.
        // Do not silently interpret a bypass as permission to connect directly.
        let env_sensitive = |key: &str| {
            let k = key.to_ascii_lowercase();
            k.contains("proxy") || k == "node_options"
        };
        if environment_uncertain
            || chromix
                .environment
                .iter()
                .any(|(k, v)| env_sensitive(k) && !v.is_empty())
        {
            return Self::Skip;
        }
        if engine == BrowserEngine::Chromix {
            let config = chromix.with_profile_options(&profile.chromix_options);
            // The bridge suppresses Profile.proxy if any layer specifies proxy or proxy flags.
            // SDK-shaped overrides/PAC/bypass are deliberately not guessed.
            let mut explicit = None;
            for (index, layer) in [
                Some(&config.options),
                config
                    .options
                    .get("launchOptions")
                    .and_then(|v| v.as_object()),
                config
                    .options
                    .get("contextOptions")
                    .and_then(|v| v.as_object()),
            ]
            .into_iter()
            .enumerate()
            .filter_map(|(i, l)| l.map(|l| (i, l)))
            {
                if layer.keys().any(|k| {
                    (k.to_ascii_lowercase().contains("proxy") && k != "proxy") || k == "env"
                }) {
                    return Self::Skip;
                }
                if let Some(proxy) = layer.get("proxy") {
                    if index != 0 {
                        return Self::Skip;
                    }
                    explicit = Some(parse_sdk_proxy(proxy));
                }
                if layer.get("ignoreDefaultArgs").is_some() {
                    return Self::Skip;
                }
                if layer
                    .get("args")
                    .and_then(|v| v.as_array())
                    .is_some_and(|args| {
                        args.iter().any(|a| {
                            a.as_str()
                                .is_none_or(|s| s.to_ascii_lowercase().contains("proxy"))
                        })
                    })
                {
                    return Self::Skip;
                }
            }
            if let Some(proxy) = explicit {
                return proxy.map(Self::Proxy).unwrap_or(Self::Skip);
            }
        }
        // No explicit proxy means Chromium can use OS/PAC settings we cannot resolve here.
        profile.proxy.clone().map(Self::Proxy).unwrap_or(Self::Skip)
    }
    fn client(&self) -> Result<reqwest::Client, String> {
        let Self::Proxy(p) = self else {
            return Err("头像未缓存：无法确认浏览器有效代理/系统代理配置，未直连".into());
        };
        if !matches!(p.proxy_type.as_str(), "http" | "socks5")
            || p.port == 0
            || p.host.is_empty()
            || p.host
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '/' | '@' | '?' | '#' | '\\'))
            || p.username.is_some() != p.password.is_some()
        {
            return Err("头像未缓存：代理配置不受支持".into());
        }
        let scheme = if p.proxy_type == "socks5" {
            "socks5h"
        } else {
            "http"
        };
        let mut url = reqwest::Url::parse("http://localhost").unwrap();
        url.set_host(Some(&p.host))
            .map_err(|_| "头像代理主机无效")?;
        url.set_port(Some(p.port)).map_err(|_| "头像代理端口无效")?;
        let proxy_url = url.as_str().replacen("http:", &format!("{scheme}:"), 1);
        let mut proxy = reqwest::Proxy::all(&proxy_url).map_err(|_| "头像代理无效")?;
        if let (Some(u), Some(p)) = (&p.username, &p.password) {
            proxy = proxy.basic_auth(u, p);
        }
        reqwest::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|_| "头像客户端创建失败".into())
    }
}

fn parse_sdk_proxy(value: &serde_json::Value) -> Option<ProxyConfig> {
    let object = value.as_object()?;
    if object
        .keys()
        .any(|k| !matches!(k.as_str(), "server" | "username" | "password" | "bypass"))
        || object.get("bypass").is_some_and(|v| v.as_str() != Some(""))
    {
        return None;
    }
    let raw = object.get("server")?.as_str()?;
    if raw.contains('@') {
        return None;
    }
    let url = reqwest::Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "socks5")
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let text = |key| -> Option<Option<String>> {
        match object.get(key) {
            None => Some(None),
            Some(v) => Some(Some(v.as_str()?.to_owned())),
        }
    };
    let username = text("username")?;
    let password = text("password")?;
    if username.is_some() != password.is_some() {
        return None;
    }
    Some(ProxyConfig {
        proxy_type: url.scheme().into(),
        host: url.host_str()?.into(),
        port: url.port_or_known_default()?,
        username,
        password,
    })
}

fn image_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return None;
    }
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        Some(("png", "png"))
    } else if bytes.len() >= 4
        && bytes.starts_with(&[0xff, 0xd8, 0xff])
        && bytes.ends_with(&[0xff, 0xd9])
    {
        Some(("jpg", "jpeg"))
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(("webp", "webp"))
    } else if bytes.len() >= 13 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        Some(("gif", "gif"))
    } else {
        None
    }
}
fn decode_base64_image(content: &str) -> Option<Vec<u8>> {
    // Reject before decoding/allocation, including an encoded size rounded up to 4.
    if content.is_empty() || content.len() > MAX_BYTES.div_ceil(3) * 4 || !content.len().is_multiple_of(4) {
        return None;
    }
    let padding = content.bytes().rev().take_while(|&b| b == b'=').count();
    if padding > 2 || content.len() / 4 * 3 - padding > MAX_BYTES {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content)
        .ok()?;
    image_type(&bytes)?;
    Some(bytes)
}

pub(super) fn valid_key(key: &str) -> bool {
    key.split_once('.').is_some_and(|(hash, ext)| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && matches!(ext, "png" | "jpg" | "webp" | "gif")
    })
}
fn regular(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}

pub(super) struct AvatarCache {
    root: PathBuf,
}
impl AvatarCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    async fn root(&self) -> Option<PathBuf> {
        let meta = tokio::fs::symlink_metadata(&self.root).await.ok()?;
        if !meta.is_dir() || !regular(&meta) {
            return None;
        }
        tokio::fs::canonicalize(&self.root).await.ok()
    }
    pub async fn download(&self, url: &str, route: &AvatarRoute) -> Result<String, String> {
        if !trusted_url(url) {
            return Err("头像未缓存：不可信头像地址".into());
        }
        let response = route
            .client()?
            .get(url)
            .header(
                reqwest::header::ACCEPT,
                "image/png,image/jpeg,image/webp,image/gif",
            )
            .send()
            .await
            .map_err(|_| "头像下载失败或超时".to_string())?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > MAX_BYTES as u64)
        {
            return Err("头像下载状态或大小无效".into());
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "头像读取失败".to_string())?;
            if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
                return Err("头像超过2MiB上限".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        self.store_bytes(&bytes).await
    }
    /// Browser content is untrusted; cap the encoded allocation before strict decoding.
    /// Text responses from Page.getResourceContent must never reach this path.
    pub async fn store_base64(&self, content: &str) -> Result<String, String> {
        let bytes = decode_base64_image(content).ok_or("浏览器头像内容或大小无效")?;
        self.store_bytes(&bytes).await
    }
    async fn store_bytes(&self, bytes: &[u8]) -> Result<String, String> {
        let (ext, _) =
            image_type(bytes).ok_or_else(|| "头像格式无效，仅支持PNG/JPEG/WebP/GIF".to_string())?;
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|_| "头像缓存目录不可用")?;
        let root = self.root().await.ok_or("头像缓存目录不安全")?;
        let key = format!("{:x}.{ext}", Sha256::digest(bytes));
        if self.read(&key).await.is_some() {
            return Ok(key);
        }
        let temp = root.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .await
                .map_err(|_| "头像临时文件创建失败")?;
            file.write_all(bytes).await.map_err(|_| "头像写入失败")?;
            file.sync_all().await.map_err(|_| "头像写入失败")?;
            drop(file);
            tokio::fs::rename(&temp, root.join(&key))
                .await
                .map_err(|_| "头像原子保存失败")?;
            Ok::<_, String>(key)
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp).await;
        }
        result
    }
    pub async fn read(&self, key: &str) -> Option<String> {
        if !valid_key(key) {
            return None;
        }
        let root = self.root().await?;
        let path = root.join(key);
        let meta = tokio::fs::symlink_metadata(&path).await.ok()?;
        if !meta.is_file() || !regular(&meta) || meta.len() > MAX_BYTES as u64 {
            return None;
        }
        if tokio::fs::canonicalize(&path).await.ok()?.parent() != Some(root.as_path()) {
            return None;
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        // Open the link itself on Windows, then recheck handle metadata. No arbitrary symlink reads.
        #[cfg(windows)]
        {
            options.custom_flags(0x00200000);
        }
        #[cfg(target_os = "linux")]
        {
            options.custom_flags(0x20000);
        } // O_NOFOLLOW
        #[cfg(target_os = "macos")]
        {
            options.custom_flags(0x100);
        } // O_NOFOLLOW
        let file = options.open(&path).await.ok()?;
        let meta = file.metadata().await.ok()?;
        if !meta.is_file() || !regular(&meta) || meta.len() > MAX_BYTES as u64 {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        let (ext, mime) = image_type(&bytes)?;
        if format!("{:x}.{ext}", Sha256::digest(&bytes)) != key {
            return None;
        }
        Some(format!(
            "data:image/{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_policy_uses_launch_snapshot_and_never_falls_back() {
        let temp = tempfile::TempDir::new().unwrap();
        let pm = profile_manager::ProfileManager::new(
            &temp.path().join("p.db"),
            &temp.path().join("profiles"),
        )
        .unwrap();
        let mut p = pm
            .create(multizen_core::CreateProfileInput {
                name: "P".into(),
                ..Default::default()
            })
            .unwrap();
        let mut settings = ChromixSettings::default();
        assert!(matches!(
            AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false),
            AvatarRoute::Skip
        ));
        p.proxy = Some(ProxyConfig {
            proxy_type: "socks5".into(),
            host: "127.0.0.1".into(),
            port: 1080,
            username: Some("u".into()),
            password: Some("p".into()),
        });
        let route = AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false);
        assert!(matches!(route, AvatarRoute::Proxy(_)));
        assert!(route.client().is_ok());
        assert!(matches!(
            AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, true),
            AvatarRoute::Skip
        ));
        settings.options.insert(
            "proxy".into(),
            serde_json::json!({"server":"http://127.0.0.1:8888"}),
        );
        let AvatarRoute::Proxy(proxy) =
            AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false)
        else {
            panic!("simple static SDK proxy")
        };
        assert_eq!(proxy.port, 8888);
        p.chromix_options.insert(
            "proxy".into(),
            serde_json::json!({"server":"http://127.0.0.1:9999"}),
        );
        let AvatarRoute::Proxy(proxy) =
            AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false)
        else {
            panic!("profile override")
        };
        assert_eq!(proxy.port, 9999);
        for value in [
            serde_json::json!(null),
            serde_json::json!({"server":"file:///tmp/x"}),
            serde_json::json!({"server":"http://u:p@localhost:1234"}),
            serde_json::json!({"server":"http://localhost:1234","bypass":"*"}),
        ] {
            p.chromix_options.insert("proxy".into(), value);
            assert!(matches!(
                AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false),
                AvatarRoute::Skip
            ));
        }
        p.chromix_options.clear();
        settings.options.insert(
            "args".into(),
            serde_json::json!(["--proxy-pac-url=https://local.invalid/pac"]),
        );
        assert!(matches!(
            AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false),
            AvatarRoute::Skip
        ));
        settings.options.clear();
        settings.environment.insert("NO_PROXY".into(), "*".into());
        assert!(matches!(
            AvatarRoute::from_profile(&p, BrowserEngine::Chromix, &settings, false),
            AvatarRoute::Skip
        ));
        assert!(AvatarRoute::Skip.client().is_err());
    }
    #[tokio::test]
    async fn cache_rejects_symlink_and_oversized_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("avatars");
        tokio::fs::create_dir_all(&root).await.unwrap();
        let cache = AvatarCache::new(root.clone());
        let bytes = b"GIF89a0000000";
        let key = format!("{:x}.gif", Sha256::digest(bytes));
        let outside = temp.path().join("outside.gif");
        tokio::fs::write(&outside, bytes).await.unwrap();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside, root.join(&key));
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, root.join(&key));
        if linked.is_ok() {
            assert!(cache.read(&key).await.is_none());
            tokio::fs::remove_file(root.join(&key)).await.unwrap();
        } else {
            eprintln!(
                "symlink fixture unavailable on this OS/account; no symlink claim from this branch"
            );
        }
        tokio::fs::write(root.join(&key), vec![0; MAX_BYTES + 1])
            .await
            .unwrap();
        assert!(cache.read(&key).await.is_none());
    }
    #[test]
    fn only_trusted_https_avatar_urls() {
        assert!(trusted_url("https://p4-pro.a.yximgs.com/uhead/a.jpg"));
        for u in [
            "https://yximgs.com.evil.test/a",
            "https://evilyximgs.com/a",
            "http://yximgs.com/a",
            "https://u:p@yximgs.com/a",
            "https://yximgs.com:444/a",
            "data:image/png;base64,a",
            "https://yximgs.com./a",
        ] {
            assert!(!trusted_url(u), "{u}");
        }
    }
    #[test]
    fn safe_keys_and_image_limits() {
        for k in ["../a.png", "a.png", "C:\\a.png", "/tmp/a.png"] {
            assert!(!valid_key(k));
        }
        assert!(image_type(b"<svg/>").is_none());
        assert!(image_type(b"<html>").is_none());
        assert!(image_type(&vec![0; MAX_BYTES + 1]).is_none());
        assert!(image_type(b"RIFF0000WEBP").is_some());
        assert!(image_type(b"GIF89a0000000").is_some());
        assert!(image_type(&[0xff, 0xd8, 0xff, 0xd9]).is_some());
    }
    #[test]
    fn browser_base64_is_bounded_strict_and_raster_only() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"GIF89a0000000");
        assert_eq!(decode_base64_image(&encoded).unwrap(), b"GIF89a0000000");
        for value in [
            "",
            "AAAA=",
            "<svg/>",
            "data:image/png;base64,AAAA",
            "R0lGODlhMDAwMDAwMA==\n",
        ] {
            assert!(decode_base64_image(value).is_none());
        }
        assert!(decode_base64_image(&"A".repeat(MAX_BYTES.div_ceil(3) * 4 + 4)).is_none());
        let mut bytes = vec![0; MAX_BYTES + 1];
        bytes[..6].copy_from_slice(b"GIF89a");
        assert!(
            decode_base64_image(&base64::engine::general_purpose::STANDARD.encode(&bytes))
                .is_none()
        );
        bytes.truncate(MAX_BYTES);
        assert_eq!(
            decode_base64_image(&base64::engine::general_purpose::STANDARD.encode(&bytes))
                .unwrap()
                .len(),
            MAX_BYTES
        );
        for bytes in [b"<svg/>".as_slice(), b"<html/>".as_slice()] {
            assert!(
                decode_base64_image(&base64::engine::general_purpose::STANDARD.encode(bytes))
                    .is_none()
            );
        }
    }
    #[tokio::test]
    async fn browser_content_uses_same_hash_cache_without_a_network_route() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = AvatarCache::new(temp.path().join("avatars"));
        let content = base64::engine::general_purpose::STANDARD.encode(b"GIF89a0000000");
        let key = cache.store_base64(&content).await.unwrap();
        assert_eq!(key, cache.store_bytes(b"GIF89a0000000").await.unwrap());
        assert!(cache
            .read(&key)
            .await
            .unwrap()
            .starts_with("data:image/gif;base64,"));
        assert!(cache.store_base64("<svg/>").await.is_err());
    }
    #[tokio::test]
    async fn cache_roundtrip_hash_and_tamper_rejection() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = AvatarCache::new(temp.path().join("avatars"));
        let key = cache.store_bytes(b"GIF89a0000000").await.unwrap();
        assert!(valid_key(&key));
        assert!(cache
            .read(&key)
            .await
            .unwrap()
            .starts_with("data:image/gif;base64,"));
        assert_eq!(cache.store_bytes(b"GIF89a0000000").await.unwrap(), key);
        tokio::fs::write(temp.path().join("avatars").join(&key), b"GIF89a1111111")
            .await
            .unwrap();
        assert!(cache.read(&key).await.is_none());
        assert!(cache.store_bytes(b"<svg/>").await.is_err());
    }
}
