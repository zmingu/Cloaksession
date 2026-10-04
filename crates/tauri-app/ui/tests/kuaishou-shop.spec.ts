import { expect, test } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/**
 * 小店账号列表（快手 › 小店）的最小 UI 契约。
 *
 * 桌面布局专用：密集表格在窄视口会依次丢弃低优先级列，快手ID / 操作列
 * 在移动项目里不再渲染，断言会失去意义。
 */
test("shop tab lists browser profiles as shop accounts with a launch control", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");

  await installTauriMock(page);
  // Land directly on the shop sub-tab; the base fixture parks on settings.
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
    localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("shop"));
  });
  await page.goto("/");

  const table = page.getByRole("region", { name: "Shop" });
  await expect(table).toBeVisible();
  await expect(table.getByRole("columnheader", { name: "Kuaishou ID" })).toBeVisible();
  await expect(table.getByRole("columnheader", { name: "Detection" })).toBeVisible();

  // Add account is the only header action; the refresh control is gone.
  await expect(page.getByRole("button", { name: "Add account", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Refresh", exact: true })).toHaveCount(0);

  const row = table.getByRole("row").filter({ hasText: "Regression profile" });
  await expect(row).toBeVisible();
  // Not running in the fixture, so the row offers Launch rather than Stop.
  await expect(row.getByRole("button", { name: "Launch" })).toBeVisible();
});
