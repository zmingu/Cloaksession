import { expect, test } from "@playwright/test";
import { defaultSettings, installTauriMock, settingsPatches, storedSettings } from "./tauriMock";

// Language-agnostic handle: `role=region` survives the aria-label change, so it
// can be probed across a language switch. (Sidebar/nav is Wave3 ownership, so
// this spec asserts Settings-page reactivity only.)
const region = (page: import("@playwright/test").Page) => page.locator('[role="region"]');

test("language switch updates Settings-page strings without remounting App", async ({ page }) => {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.goto("/");

  // English first paint (entry gate read settings before mounting).
  await expect(page.getByRole("region", { name: "Settings", exact: true })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.getByText("Browser engine", { exact: true })).toBeVisible();

  // Mark the App-owned settings container. A remount would replace the DOM
  // node; a context-only update keeps it (and any unsaved form input with it).
  await region(page).evaluate((el) => el.setAttribute("data-remount-probe", "1"));

  await page.getByRole("button", { name: "简体中文", exact: true }).click();

  // Global language updates only after the server confirmed the save; the
  // Settings page strings re-render in Chinese through context.
  await expect(page.getByRole("region", { name: "设置", exact: true })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.getByText("浏览器引擎", { exact: true })).toBeVisible();
  await expect(region(page)).toHaveAttribute("data-remount-probe", "1");

  const saved = await storedSettings(page);
  expect(saved.language).toBe("zh-CN");
  expect((await settingsPatches(page)).at(-1)).toEqual({ language: "zh-CN" });
});

test("repeated submits are disabled while a language save is in flight", async ({ page }) => {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Settings", exact: true })).toBeVisible();

  await page.evaluate(() => {
    const mock = (window as any).__TEST_IPC__;
    const original = (window as any).__TAURI_INTERNALS__.invoke;
    let release: (() => void) | null = null;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    mock.releaseSave = release;
    (window as any).__TAURI_INTERNALS__.invoke = async (command: string, args: any) => {
      if (command === "settings_update" && args?.patch?.language) {
        mock.languageSaveCalls = (mock.languageSaveCalls ?? 0) + 1;
        await gate;
      }
      return original(command, args);
    };
  });

  const zhButton = page.getByRole("button", { name: "简体中文", exact: true });
  await zhButton.click();
  await expect(zhButton).toBeDisabled();
  await expect(page.getByText("Saving…", { exact: true })).toBeVisible();

  // A second click while disabled must not enqueue another save.
  await zhButton.click({ force: true });
  expect(await page.evaluate(() => (window as any).__TEST_IPC__.languageSaveCalls)).toBe(1);

  await page.evaluate(() => (window as any).__TEST_IPC__.releaseSave());
  await expect(page.getByRole("region", { name: "设置", exact: true })).toBeVisible();
});

test("failed language save keeps the previous language and does not fake success", async ({ page }) => {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Settings", exact: true })).toBeVisible();

  await page.evaluate(() => { (window as any).__TEST_IPC__.failNextSave = true; });
  await page.getByRole("button", { name: "简体中文", exact: true }).click();

  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.getByRole("region", { name: "Settings", exact: true })).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("Could not change language");
});
test("settings-load failure falls back to Chinese with an actionable retry", async ({ page }) => {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  // Fail every settings_get until the test explicitly allows it. StrictMode
  // double-invokes the gate effect, so a one-shot failure would be masked by
  // the second call succeeding.
  await page.addInitScript(() => {
    (window as any).__ALLOW_SETTINGS_GET__ = false;
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    internals.invoke = async (command: string, args: any) => {
      if (command === "settings_get" && !(window as any).__ALLOW_SETTINGS_GET__) {
        throw new Error("settings file unreadable");
      }
      return original(command, args);
    };
  });
  await page.goto("/");

  // Falls back to Chinese, shows an actionable message, and offers retry.
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
  await expect(page.getByText("无法加载设置", { exact: true })).toBeVisible();
  await expect(page.getByRole("alert")).toContainText("应用无法读取已保存的设置");
  const retry = page.getByRole("button", { name: "重试", exact: true });
  await expect(retry).toBeVisible();

  // Retry succeeds and mounts the full app using the saved language.
  await page.evaluate(() => { (window as any).__ALLOW_SETTINGS_GET__ = true; });
  await retry.click();
  await expect(page.getByRole("region", { name: "Settings", exact: true })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
});
