import { expect, test } from "@playwright/test";
import { defaultSettings, installTauriMock, profilePatches } from "./tauriMock";

// Throwaway Wave4 verification (deleted after the run): language switch
// coverage for the fallback-replacement domains + profile-data invariance.
test("wave4: zh switch translates migrated domains, leaves profile data alone", async ({
  page,
}) => {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.goto("/");

  // --- English baseline: profiles list + extensions + MCP + activity --------
  await page.getByRole("button", { name: "Profiles", exact: true }).click();
  await expect(page.getByText("All profiles", { exact: true })).toBeVisible();
  await expect(page.getByText("Direct — no proxy", { exact: true }).first()).toBeVisible();

  await page.getByRole("button", { name: /Regression profile/ }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Edit Regression profile")).toBeVisible();
  await dialog.getByRole("button", { name: "Extensions", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Add", exact: true }).first()).toBeVisible();
  await dialog.getByRole("button", { name: "Browse catalog", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Add", exact: true }).first()).toBeVisible();
  await page.keyboard.press("Escape");

  await page.getByRole("button", { name: "MCP", exact: true }).click();
  await expect(page.getByText("Connect an agent", { exact: true })).toBeVisible();
  await expect(page.getByText("No agent calls yet", { exact: true })).toBeVisible();
  await expect(page.getByText(/bridges the endpoint through/)).toBeVisible();

  // The bottom activity drawer only renders outside the MCP section.
  await page.getByRole("button", { name: "Profiles", exact: true }).click();
  await page.getByText("MCP activity", { exact: true }).click();
  await expect(page.getByText(/No MCP calls yet\./)).toBeVisible();
  await expect(page.getByText("0 calls", { exact: true })).toBeVisible();

  // --- Switch to Chinese -----------------------------------------------------
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "简体中文", exact: true }).click();
  await expect(page.getByRole("region", { name: "设置", exact: true })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");

  // Nav + profiles list in Chinese. Collapse the (open) activity drawer
  // first so it cannot cover the profile row on narrow viewports.
  await page.getByRole("button", { name: "浏览器配置", exact: true }).click();
  if (await page.getByText(/暂无 MCP 调用|No MCP calls yet/).isVisible()) {
    await page.getByText("MCP 活动", { exact: true }).click();
  }
  await expect(page.getByText("全部浏览器配置", { exact: true })).toBeVisible();
  await expect(page.getByText("直连——无代理", { exact: true }).first()).toBeVisible();

  // Extensions section in Chinese.
  await page.getByRole("button", { name: /Regression profile/ }).click();
  const zhDialog = page.getByRole("dialog");
  // Sheet tabs are a different lane's ownership (still English); the
  // ExtensionsSection content inside is Wave4's.
  await zhDialog.getByRole("button", { name: "Extensions", exact: true }).click();
  await expect(zhDialog.getByRole("button", { name: "添加", exact: true }).first()).toBeVisible();
  await zhDialog.getByRole("button", { name: "浏览目录", exact: true }).click();
  await expect(zhDialog.getByRole("button", { name: "添加", exact: true }).first()).toBeVisible();
  await page.keyboard.press("Escape");

  // MCP panel in Chinese.
  await page.getByRole("button", { name: "MCP", exact: true }).click();
  await expect(page.getByText("连接 AI 智能体", { exact: true })).toBeVisible();
  await expect(page.getByText("暂无智能体调用", { exact: true })).toBeVisible();

  // Activity drawer in Chinese (drawer only renders outside MCP section).
  // It is still open from the English pass; only toggle if it got closed.
  await page.getByRole("button", { name: "浏览器配置", exact: true }).click();
  if (!(await page.getByText(/暂无 MCP 调用/).isVisible())) {
    await page.getByText("MCP 活动", { exact: true }).click();
  }
  await expect(page.getByText(/暂无 MCP 调用/)).toBeVisible();
  await expect(page.getByText("0 次调用", { exact: true })).toBeVisible();

  // Sweep the rendered Chinese UI for leftover English fallbacks.
  for (const title of ["浏览器配置", "MCP", "设置"]) {
    await page.getByRole("button", { name: title, exact: true }).click();
    const text = await page.evaluate(() => document.body.innerText);
    for (const leftover of [
      "click to retry",
      "click to re-check",
      "Testing…",
      "Checking…",
      "Name your first profile",
      "No MCP calls yet",
      "Claude Desktop's config has no",
      "{Stdio clients}",
      "{{",
    ]) {
      expect(text, `${title} should not contain ${leftover}`).not.toContain(leftover);
    }
  }

  // Profile data invariance across the switch.
  const profile = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("cloaksession.test.profile")!),
  );
  expect(profile.fingerprint.locale).toBe("en-US");
  expect(await profilePatches(page)).toEqual([]);
});

test("wave4: onboarding name step renders in both languages", async ({ page }) => {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.addInitScript(() => localStorage.setItem("multizen.ui.onboarded", "false"));
  await page.goto("/");
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.getByRole("button", { name: "Not now", exact: true }).click();
  await expect(page.getByText("Name your first profile.", { exact: true })).toBeVisible();
  await expect(page.getByText(/Just a label to find it later\./)).toBeVisible();
});

test("wave4: onboarding name step renders in Chinese", async ({ page }) => {
  await installTauriMock(page, { ...defaultSettings, language: "zh-CN" });
  await page.addInitScript(() => localStorage.setItem("multizen.ui.onboarded", "false"));
  await page.goto("/");
  await page.getByRole("button", { name: "继续", exact: true }).click();
  await page.getByRole("button", { name: "暂不", exact: true }).click();
  await expect(page.getByText("为你的第一个浏览器配置命名。", { exact: true })).toBeVisible();
  await expect(page.getByText(/只是一个便于日后查找的名称/)).toBeVisible();
  const text = await page.evaluate(() => document.body.innerText);
  expect(text).not.toContain("Name your first profile");
  expect(text).not.toContain("Just a label");
});
