import { expect, test } from "@playwright/test";
import { FIRST_PROFILE } from "./businessAccountsMock";
import { installIdentityMock } from "./kuaishouIdentityMock";
import { profilePatches } from "./tauriMock";

// IPC is mocked: these tests never launch the user's environment or request a platform.
test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
  await installIdentityMock(page, [], []);
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const mock = { calls: [] as any[], release: null as (() => void) | null, fail: "", hold: false };
    (window as any).__SHOP_LAUNCH__ = mock;
    internals.invoke = async (command: string, args: any) => {
      if (command !== "profiles_launch") return original(command, args);
      mock.calls.push(structuredClone(args));
      if (mock.hold) await new Promise<void>((resolve) => { mock.release = resolve; });
      if (mock.fail) throw mock.fail;
      return { id: args.id, cdpEndpoint: "http://127.0.0.1:19222", pid: 1, startedAt: "fixture" };
    };
  });
});

test("shop login is explicit, deduplicated, and does not edit a saved custom start URL", async ({ page }) => {
  const summary = page.getByTestId(`kuaishou-summary-${FIRST_PROFILE}`);
  const button = summary.getByRole("button", { name: "快手小店扫码", exact: true });
  await expect(button).toBeEnabled();
  await page.evaluate(() => { (window as any).__SHOP_LAUNCH__.hold = true; });
  await button.evaluate((element: HTMLButtonElement) => { element.click(); element.click(); });
  await expect(summary.getByRole("button", { name: "正在打开小店…" })).toBeDisabled();
  await expect.poll(() => page.evaluate(() => (window as any).__SHOP_LAUNCH__.calls)).toEqual([{ id: FIRST_PROFILE, entry: "kuaishou-shop" }]);
  await page.evaluate(() => (window as any).__SHOP_LAUNCH__.release());
  await expect(summary.getByRole("status")).toContainText("不代表已登录");
  expect(await profilePatches(page)).toEqual([]);
  expect(await page.evaluate(() => (window as any).__TEST_IPC__.profile().startUrl)).toBe("https://example.com/");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("backend Jinniu rejection is visible and retry remains possible", async ({ page }) => {
  await page.evaluate(() => { (window as any).__SHOP_LAUNCH__.fail = "金牛专用环境不能通过小店扫码入口启动"; });
  const summary = page.getByTestId(`kuaishou-summary-${FIRST_PROFILE}`);
  const button = summary.getByRole("button", { name: "快手小店扫码", exact: true });
  await button.click();
  await expect(summary.getByRole("status")).toContainText("金牛专用环境");
  await expect(button).toBeEnabled();
  expect(await profilePatches(page)).toEqual([]);
});

test("ordinary launch still sends only the profile ID", async ({ page }) => {
  await page.getByRole("button", { name: "Launch", exact: true }).first().click();
  await expect.poll(() => page.evaluate(() => (window as any).__SHOP_LAUNCH__.calls.length)).toBe(1);
  expect(await page.evaluate(() => (window as any).__SHOP_LAUNCH__.calls[0])).toEqual({ id: FIRST_PROFILE });
  expect(await profilePatches(page)).toEqual([]);
});
