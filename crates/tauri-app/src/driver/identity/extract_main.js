(() => {
  // Main-site viewer (互动账号) identity read, mirroring jieger
  // `extractSubAccountProfile`. The selectors are UNVERIFIED against the live
  // page and deliberately best-effort: a mismatch returns null fields rather
  // than throwing. The Rust side re-validates the origin and the id shape.
  const result = { url: location.href, platformUserId: null, nickname: null, avatarUrl: null };
  let locationUrl;
  try { locationUrl = new URL(location.href); } catch (_) { return result; }
  if (locationUrl.protocol !== "https:" || locationUrl.hostname !== "www.kuaishou.com" ||
      (locationUrl.port && locationUrl.port !== "443") || locationUrl.username || locationUrl.password) return result;

  const clean = (value) => (typeof value === "string" ? value.replace(/\s+/g, " ").trim() : "");
  const setId = (value) => {
    const text = clean(value).replace(/^@/, "");
    // Same shape as the Rust viewer-id validator: alphanumeric plus _ . - , 3..32.
    if (!result.platformUserId && /^[A-Za-z0-9_.-]{3,32}$/.test(text)) result.platformUserId = text;
  };
  const setName = (value) => {
    const text = clean(value).replace(/我的$/, "");
    if (!result.nickname && text && Array.from(text).length <= 80) result.nickname = text;
  };
  const setAvatar = (value) => {
    const text = typeof value === "string" ? value.trim() : "";
    if (!result.avatarUrl && text && !text.startsWith("data:") && text.length <= 2048) result.avatarUrl = text;
  };

  try {
    // Strategy 1: the cached principal id (jieger uses the raw localStorage value).
    setId(localStorage.getItem("userId"));

    // Strategy 2: the sidebar account node.
    const node = document.querySelector(".sidebar .user.item") ||
      document.querySelector('[class*="sidebar"] .user.item') ||
      document.querySelector(".navbar .user.item");
    if (!node) return result;

    setName((node.querySelector(".text-name") ||
      node.querySelector('[class*="text-name"]') ||
      node.querySelector(".text"))?.textContent);

    const image = node.querySelector("img[src]");
    if (image) {
      setAvatar(image.currentSrc || image.src);
      setName(image.alt || image.title);
    }

    // Strategy 3: the profile link inside the node.
    const link = node.closest("a[href]");
    if (link) {
      const match = link.href.match(/\/(?:profile|user|u)\/([^/?#]+)/);
      setId(match && match[1] ? decodeURIComponent(match[1]) : null);
    }
  } catch (_) { /* Best-effort: partial data is acceptable. */ }
  return result;
})()
