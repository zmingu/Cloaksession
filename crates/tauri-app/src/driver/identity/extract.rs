use cdp_driver::{session::BrowserSession, TaskCancel};
use multizen_core::valid_kuaishou_user_id;
use serde::Deserialize;
use std::time::Duration;

pub(super) const EXTRACTOR: &str = include_str!("extract.js");
const LOCK_WAIT: Duration = Duration::from_millis(500);
const READ_WAIT: Duration = Duration::from_secs(2);
pub(super) const MAX_AVATAR_BYTES: usize = cdp_driver::avatar_resource::MAX_AVATAR_BYTES;
const MAX_ENCODED: usize = MAX_AVATAR_BYTES.div_ceil(3) * 4;
const MAX_DIMENSION: u32 = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PageIdentity {
    pub url: String,
    pub platform_user_id: Option<String>,
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
}

pub(super) fn shop_url(raw: &str) -> bool {
    if raw.len() > 8192
        || raw.split_once("://").is_some_and(|(_, v)| {
            v.split(['/', '?', '#'])
                .next()
                .is_some_and(|v| v.contains('@'))
        })
    {
        return false;
    }
    reqwest::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("s.kwaixiaodian.com")
            && u.port_or_known_default() == Some(443)
            && u.username().is_empty()
            && u.password().is_none()
    })
}
pub(super) fn trusted_url(raw: &str) -> bool {
    cdp_driver::avatar_resource::trusted_avatar_url(raw)
}
pub(super) fn decode(value: serde_json::Value) -> Result<PageIdentity, String> {
    let mut page: PageIdentity =
        serde_json::from_value(value).map_err(|_| "身份页面返回结构无效".to_string())?;
    if !shop_url(&page.url) {
        return Err("读取期间页面已离开准确的小店域名".into());
    }
    if page
        .platform_user_id
        .as_deref()
        .is_some_and(|id| !valid_kuaishou_user_id(id))
    {
        return Err("身份页面返回ID格式无效".into());
    }
    page.nickname = page
        .nickname
        .filter(|v| !v.is_empty() && v.chars().count() <= 80 && !v.chars().any(char::is_control));
    if page.avatar_url.as_ref().is_some_and(|v| v.len() > 2048) {
        page.avatar_url = None;
    }
    Ok(page)
}

#[derive(Debug)]
pub(super) enum Detection {
    NoPage,
    NoId,
    Conflict,
    Found(PageIdentity),
}

pub(super) fn combine(pages: Vec<PageIdentity>) -> Detection {
    if pages.is_empty() {
        return Detection::NoPage;
    }
    let mut selected: Option<PageIdentity> = None;
    for page in pages.into_iter().filter(|p| p.platform_user_id.is_some()) {
        if let Some(previous) = &mut selected {
            if previous.platform_user_id != page.platform_user_id {
                return Detection::Conflict;
            }
            if previous.nickname.is_none() {
                previous.nickname = page.nickname;
            }
            if previous.avatar_url.is_none() {
                previous.avatar_url = page.avatar_url;
            }
        } else {
            selected = Some(page);
        }
    }
    selected.map(Detection::Found).unwrap_or(Detection::NoId)
}

/// One synchronous, main-frame-only read of the already-loaded account image.
/// Never use getResourceContent here: local Chrome testing observed it re-requesting
/// the main document. Canvas taint is an unavailable image, not a reason to reload.
pub(super) fn avatar_probe_expression(identity: &str, url: &str, pixels: bool) -> String {
    if !valid_kuaishou_user_id(identity) || !trusted_url(url) {
        return "null".into();
    }
    let identity = serde_json::to_string(identity).unwrap();
    let url = serde_json::to_string(url).unwrap();
    format!(
        r#"(() => {{
      if (window !== window.top) return null;
      const expectedId = {identity}, expectedUrl = {url};
      const readIdentity = () => {EXTRACTOR};
      const selector = '[class*="avatar___"] .seller-main-avatar img';
      const observe = () => {{
        const page = readIdentity();
        const image = document.querySelector(selector);
        if (!(image instanceof HTMLImageElement) || !image.isConnected || image.ownerDocument !== document ||
            page.platformUserId !== expectedId || page.avatarUrl !== expectedUrl || image.currentSrc !== expectedUrl) return null;
        return {{ page, documentEpoch: String(performance.timeOrigin), currentSrc: image.currentSrc,
          complete: image.complete, width: image.naturalWidth, height: image.naturalHeight }};
      }};
      const before = observe(), image = document.querySelector(selector);
      if (!before) return null;
      let content = null;
      if ({pixels} && before.complete && before.width > 0 && before.height > 0 &&
          before.width <= {MAX_DIMENSION} && before.height <= {MAX_DIMENSION}) {{
        try {{
          const canvas = document.createElement('canvas');
          canvas.width = before.width; canvas.height = before.height;
          const context = canvas.getContext('2d');
          if (context) {{
            context.drawImage(image, 0, 0);
            const data = canvas.toDataURL('image/png');
            const prefix = 'data:image/png;base64,';
            if (data.startsWith(prefix) && data.length <= prefix.length + {MAX_ENCODED})
              content = data.slice(prefix.length);
          }}
        }} catch (_) {{ /* Cross-origin taint and decoding failure are expected. */ }}
      }}
      const after = observe();
      if (!after || document.querySelector(selector) !== image || JSON.stringify(before) !== JSON.stringify(after)) return null;
      return {{ ...after, content }};
    }})()"#
    )
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AvatarProbe {
    page: PageIdentity,
    document_epoch: String,
    current_src: String,
    complete: bool,
    width: u32,
    height: u32,
    content: Option<String>,
}

impl AvatarProbe {
    fn decode(value: serde_json::Value, identity: &str, url: &str) -> Result<Self, String> {
        let probe: Self =
            serde_json::from_value(value).map_err(|_| "账号或头像已变化，已跳过头像")?;
        if !shop_url(&probe.page.url)
            || !trusted_url(url)
            || probe.document_epoch.is_empty()
            || probe.document_epoch.len() > 64
            || !probe
                .document_epoch
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'.')
            || probe.page.platform_user_id.as_deref() != Some(identity)
            || probe.page.avatar_url.as_deref() != Some(url)
            || probe.current_src != url
            || probe.content.as_ref().is_some_and(|v| {
                v.len() > MAX_ENCODED
                    || !probe.complete
                    || probe.width == 0
                    || probe.height == 0
                    || probe.width > MAX_DIMENSION
                    || probe.height > MAX_DIMENSION
            })
        {
            return Err("账号或头像状态无效，已跳过头像".into());
        }
        Ok(probe)
    }
}

/// Retains the original target lease through local caching / explicit proxy fallback.
/// No raw page escape; every read remains a controlled &mut TaskPage operation.
pub(super) struct AvatarPage<'a> {
    task: cdp_driver::TaskPage<'a>,
    before: AvatarProbe,
    identity: String,
    url: String,
}

impl<'a> AvatarPage<'a> {
    pub async fn open(
        session: &'a BrowserSession,
        target: &str,
        identity: &str,
        url: &str,
        cancel: TaskCancel,
    ) -> Result<Self, String> {
        if !valid_kuaishou_user_id(identity) || !trusted_url(url) {
            return Err("头像未缓存：不可信头像地址或ID".into());
        }
        let mut task = session
            .task_page(target, cancel, LOCK_WAIT)
            .await
            .map_err(|_| "头像页面忙、已关闭或已取消")?;
        let value = task
            .evaluate(&avatar_probe_expression(identity, url, true), READ_WAIT)
            .await
            .map_err(|_| "浏览器已加载头像读取失败或超时")?;
        let before = AvatarProbe::decode(value, identity, url)?;
        Ok(Self {
            task,
            before,
            identity: identity.into(),
            url: url.into(),
        })
    }

    pub fn content(&self) -> Option<&str> {
        self.before.content.as_deref()
    }

    /// A single browser GET when already-loaded pixels are unavailable. Recheck both
    /// sides of the request; bytes never outlive a changed account/document candidate.
    pub async fn read_network(&mut self) -> Result<String, String> {
        self.revalidate().await?;
        if !self.before.complete
            || self.before.width == 0
            || self.before.height == 0
            || self.before.width > MAX_DIMENSION
            || self.before.height > MAX_DIMENSION
        {
            return Err("头像尚未加载或尺寸无效".into());
        }
        let content = self
            .task
            .read_public_avatar(&self.url, Duration::from_secs(4))
            .await
            .map_err(|_| "头像未缓存：浏览器受控读取失败、超时或已取消")?;
        self.revalidate().await?;
        Ok(content)
    }

    pub async fn revalidate(&mut self) -> Result<(), String> {
        let value = self
            .task
            .evaluate(
                &avatar_probe_expression(&self.identity, &self.url, false),
                READ_WAIT,
            )
            .await
            .map_err(|_| "头像读取后页面已关闭、取消或超时")?;
        let after = AvatarProbe::decode(value, &self.identity, &self.url)?;
        if self.before.page.url != after.page.url
            || self.before.document_epoch != after.document_epoch
            || self.before.complete != after.complete
            || self.before.width != after.width
            || self.before.height != after.height
        {
            return Err("头像读取期间页面或图片加载状态已变化，已丢弃头像".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(dead_code)] // Called by the standalone CDP peer/browser test binaries.
pub(super) async fn detect(
    session: &BrowserSession,
    cancel: TaskCancel,
) -> Result<Detection, String> {
    Ok(detect_with_avatar_target(session, cancel).await?.0)
}

/// Keep routing metadata outside the four-field extractor DTO.
pub(super) async fn detect_with_avatar_target(
    session: &BrowserSession,
    cancel: TaskCancel,
) -> Result<(Detection, Option<String>), String> {
    detect_targets(session, cancel, None).await
}

/// The supplied lease is read directly, never reacquired or omitted from identity
/// verification. Every other shop target is still checked for conflicts/errors.
/// This is initialization's live check while its shared profile reservation keeps
/// the ordinary monitor from contending on the same cooperative TaskPage lock.
pub(super) async fn verify_initialization_identity(
    session: &BrowserSession,
    owned: &mut cdp_driver::TaskPage<'_>,
    expected: &str,
    cancel: TaskCancel,
) -> Result<(), String> {
    let before = decode(owned.evaluate(EXTRACTOR, READ_WAIT).await.map_err(|_| "任务页身份读取失败".to_string())?)?;
    let (peers, _) = detect_targets(session, cancel, Some(owned.target_id())).await?;
    verify_owned_and_peers(&before, peers, expected)?;
    let after = decode(owned.evaluate(EXTRACTOR, READ_WAIT).await.map_err(|_| "任务页身份回读失败".to_string())?)?;
    if after.platform_user_id.as_deref() != Some(expected) { return Err("任务页账号已变化".into()); }
    Ok(())
}
fn verify_owned_and_peers(owned: &PageIdentity, peers: Detection, expected: &str) -> Result<(), String> {
    if owned.platform_user_id.as_deref() != Some(expected) { return Err("任务页账号已变化".into()); }
    match peers {
        Detection::NoPage | Detection::NoId => Ok(()), // owned page supplies the positive identity evidence
        Detection::Found(peer) if peer.platform_user_id.as_deref() == Some(expected) => Ok(()),
        _ => Err("其他小店页面身份冲突，已拒绝初始化".into()),
    }
}
async fn detect_targets(
    session: &BrowserSession,
    cancel: TaskCancel,
    already_leased: Option<&str>,
) -> Result<(Detection, Option<String>), String> {
    let pages = session
        .browser
        .pages()
        .await
        .map_err(|_| "无法列出浏览器页面".to_string())?;
    let mut observations = Vec::new();
    for page in pages {
        if cancel.is_cancelled() {
            return Err("环境检测已取消".into());
        }
        if already_leased == Some(page.target_id().as_ref()) { continue; }
        let url = page
            .url()
            .await
            .map_err(|_| "无法读取页面地址".to_string())?;
        if !url.as_deref().is_some_and(shop_url) {
            continue;
        }
        if observations.len() >= 16 - usize::from(already_leased.is_some()) {
            return Err("小店页面过多，无法在有界检测内确认身份".into());
        }
        let mut task = session
            .task_page(page.target_id().as_ref(), cancel.clone(), LOCK_WAIT)
            .await
            .map_err(|_| "小店页面忙、已关闭或等待超时".to_string())?;
        let value = task
            .evaluate(EXTRACTOR, READ_WAIT)
            .await
            .map_err(|_| "小店身份读取失败或超时".to_string())?;
        observations.push((task.target_id().to_owned(), decode(value)?));
    }
    let detection = combine(observations.iter().map(|(_, page)| page.clone()).collect());
    let target = match &detection {
        Detection::Found(found) => observations.iter().find_map(|(target, page)| {
            (page.platform_user_id == found.platform_user_id && page.avatar_url == found.avatar_url)
                .then(|| target.clone())
        }),
        _ => None,
    };
    Ok((detection, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn page(id: Option<&str>) -> PageIdentity {
        decode(json!({"url":"https://s.kwaixiaodian.com/zone/home","platformUserId":id,"nickname":"N","avatarUrl":null})).unwrap()
    }
    #[test]
    fn initialization_checks_owned_identity_and_all_peer_conflicts() {
        let owned = page(Some("12345"));
        assert!(verify_owned_and_peers(&owned, Detection::NoPage, "12345").is_ok());
        assert!(verify_owned_and_peers(&owned, combine(vec![page(None), page(Some("12345"))]), "12345").is_ok());
        assert!(verify_owned_and_peers(&owned, combine(vec![page(Some("23456"))]), "12345").is_err());
        assert!(verify_owned_and_peers(&owned, combine(vec![page(Some("12345")), page(Some("23456"))]), "12345").is_err());
        assert!(verify_owned_and_peers(&page(None), Detection::Found(owned.clone()), "12345").is_err());
        assert!(verify_owned_and_peers(&page(Some("23456")), Detection::Found(owned), "12345").is_err());
    }
    #[test]
    fn exact_https_origin() {
        assert!(shop_url("https://s.kwaixiaodian.com:443/zone/home"));
        for url in [
            "http://s.kwaixiaodian.com",
            "https://s.kwaixiaodian.com.evil.test",
            "https://evil.test/s.kwaixiaodian.com",
            "https://s.kwaixiaodian.com@evil.test",
            "https://u@s.kwaixiaodian.com",
            "https://s.kwaixiaodian.com:444",
            "https://s.kwaixiaodian.com.",
        ] {
            assert!(!shop_url(url), "{url}");
        }
    }
    #[test]
    fn strict_decode_no_id_and_malformed() {
        assert!(page(None).platform_user_id.is_none());
        for id in [
            "1234",
            "12345a",
            "123456789012345678901234567890123",
            "12345\n",
        ] {
            assert!(decode(json!({"url":"https://s.kwaixiaodian.com","platformUserId":id,"nickname":null,"avatarUrl":null})).is_err());
        }
        assert!(decode(json!({"url":"https://evil.test","platformUserId":"12345"})).is_err());
        assert!(
            decode(json!({"url":"https://s.kwaixiaodian.com","platformUserId":12345})).is_err()
        );
    }
    #[test]
    fn aggregates_all_targets_without_arbitrary_account_selection() {
        assert!(matches!(combine(vec![]), Detection::NoPage));
        assert!(matches!(combine(vec![page(None)]), Detection::NoId));
        assert!(matches!(
            combine(vec![page(Some("12345")), page(Some("12345"))]),
            Detection::Found(_)
        ));
        assert!(matches!(
            combine(vec![page(Some("12345")), page(Some("54321"))]),
            Detection::Conflict
        ));
    }
}
