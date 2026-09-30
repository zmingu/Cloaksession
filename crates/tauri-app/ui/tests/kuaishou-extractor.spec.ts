import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";

// Execute the actual Rust-owned script against inert local DOM; never navigate to a platform.
const extractor = readFileSync(new URL("../../src/driver/identity/extract.js", import.meta.url), "utf8");
async function extract(page: Page, url = "https://s.kwaixiaodian.com/zone/home") {
  return page.evaluate(({ code, url }) => {
    const run = new Function("location", "document", `return ${code}`);
    return run({ href: url }, document) as { url: string; platformUserId: string | null; nickname: string | null; avatarUrl: string | null };
  }, { code: extractor, url });
}

test("extractor uses only the header and accepts complete ASCII IDs, not partial or product matches", async ({ page }) => {
  await page.setContent('<div class="username___fixture"><span class="id___fixture"></span><span class="nickName___fixture"> 本地   昵称 </span></div><p>ID: 99999999</p>');
  for (const [text, expected] of [
    ["ID: 00123456", "00123456"], ["ID：12345", "12345"], ["ID12345", "12345"],
    [`ID: ${"1".repeat(32)}`, "1".repeat(32)], [`ID: ${"1".repeat(33)}`, null],
    ["ID: 1234", null], ["ID: 123456x", null], ["ID: １２３４５６", null], ["ID: 12345 extra", null], ["", null],
  ] as const) {
    await page.locator('[class*="id___"]').evaluate((element, text) => { element.textContent = text; }, text);
    const result = await extract(page);
    expect(result.platformUserId, text).toBe(expected);
    expect(result.nickname).toBe(expected ? "本地 昵称" : null);
  }
});

test("extractor rejects fake origins and prefers currentSrc, with no nickname or avatar without an ID", async ({ page }) => {
  await page.setContent('<div class="username___fixture"><span class="id___fixture">ID: 12345678</span><span class="nickName___fixture">昵称</span></div><div class="avatar___fixture"><div class="seller-main-avatar"><img></div></div>');
  await page.locator("img").evaluate((image) => {
    // Getters only: these strings never become a network-capable src attribute.
    Object.defineProperty(image, "src", { configurable: true, value: "https://avatar.invalid/fallback.png" });
    Object.defineProperty(image, "currentSrc", { configurable: true, value: "https://avatar.invalid/current.webp" });
  });
  expect((await extract(page)).avatarUrl).toBe("https://avatar.invalid/current.webp");
  for (const url of ["https://s.kwaixiaodian.com.evil.invalid/", "http://s.kwaixiaodian.com/", "https://s.kwaixiaodian.com:444/", "https://user@s.kwaixiaodian.com/", "https://login.kwaixiaodian.com/"]) {
    expect(await extract(page, url)).toEqual({ url, platformUserId: null, nickname: null, avatarUrl: null });
  }
  await page.locator('[class*="nickName___"]').evaluate((element) => { element.textContent = "长".repeat(81); });
  expect((await extract(page)).nickname).toBeNull();
  await page.locator('[class*="id___"]').evaluate((element) => { element.textContent = ""; });
  expect(await extract(page)).toEqual({ url: "https://s.kwaixiaodian.com/zone/home", platformUserId: null, nickname: null, avatarUrl: null });
});
