(() => {
  const url = location.href;
  const result = { url, platformUserId: null, nickname: null, avatarUrl: null };
  const locationUrl = new URL(url);
  if (locationUrl.protocol !== "https:" || locationUrl.hostname !== "s.kwaixiaodian.com" || (locationUrl.port && locationUrl.port !== "443") || locationUrl.username || locationUrl.password) return result;
  // Read only the account header. Never search body/product identifiers or auth storage.
  const text = document.querySelector('[class*="username___"] [class*="id___"]')?.textContent?.trim() || "";
  const match = /^ID[:：]?\s*([0-9]{5,32})$/.exec(text);
  if (!match) return result;
  result.platformUserId = match[1];
  const nickname = document.querySelector('[class*="username___"] [class*="nickName___"]')?.textContent?.replace(/\s+/g, " ").trim();
  if (nickname && Array.from(nickname).length <= 80) result.nickname = nickname;
  const image = document.querySelector('[class*="avatar___"] .seller-main-avatar img');
  const src = image?.currentSrc || image?.src;
  if (typeof src === "string" && src.length <= 2048) result.avatarUrl = src;
  return result;
})()
