import { expect, test, type Page } from "@playwright/test";
import {
  installJinniuMock,
  jinniuFixture,
  jinniuRequests,
  setJinniuAccounts,
} from "./jinniuAccountsMock";

/**
 * 磁力金牛账号页（多大户管理）契约套件。
 *
 * IPC 全部浏览器内伪造：不启动环境、不打开浏览器、不接触任何平台。
 * 基座 fixture 语言为 English，断言使用 en 字典文案。
 */
test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
});

const row = (page: Page, id: string) => page.getByTestId(`jinniu-row-${id}`);
const status = (page: Page, id: string) => page.getByTestId(`jinniu-status-${id}`);
const dialog = (page: Page) => page.getByRole("dialog");

const CONNECTED = jinniuFixture({
  id: "jinniu-1",
  label: "大户甲",
  status: "connected",
  isActive: true,
  masterName: "甲主",
  masterId: "1001",
  currentSubAccountId: "sub-1",
  currentSubAccountName: "子户一",
  balanceText: "¥ 12,345.00",
});
const IDLE = jinniuFixture({ id: "jinniu-2", label: "大户乙", status: "disconnected" });
const AWAITING = jinniuFixture({ id: "jinniu-3", label: "大户丙", status: "awaiting-sub-account" });

test("list renders every account with a status badge and the active marker", async ({ page }) => {
  await installJinniuMock(page, [CONNECTED, IDLE, AWAITING]);

  await expect(page.getByTestId("jinniu-list")).toBeVisible();
  await expect(row(page, "jinniu-1")).toContainText("大户甲");
  await expect(row(page, "jinniu-2")).toContainText("大户乙");
  await expect(row(page, "jinniu-3")).toContainText("大户丙");

  // One badge per status wire value, localized. Awaiting and connected both
  // present as "Running" — sub-account picking is not a visible stage.
  await expect(status(page, "jinniu-1")).toHaveText("Running");
  await expect(status(page, "jinniu-2")).toHaveText("Stopped");
  await expect(status(page, "jinniu-3")).toHaveText("Running");

  // Only the active account carries the marker.
  await expect(page.getByTestId("jinniu-active-jinniu-1")).toBeVisible();
  await expect(page.getByTestId("jinniu-active-jinniu-2")).toHaveCount(0);

  // The sub-account is shown for the connected account.
  await expect(row(page, "jinniu-1")).toContainText("子户一");

  const lists = await jinniuRequests(page, "jinniu_accounts_list");
  expect(lists.length).toBeGreaterThanOrEqual(1);
});

test("empty state offers the add entry point (placeholder copy removed)", async ({ page }) => {
  await installJinniuMock(page, []);
  // 空态文案已按产品要求删除；添加入口仍可用。
  await expect(page.getByText("No accounts yet.", { exact: false })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add account", exact: true })).toBeEnabled();
});

test("adding an account creates a placeholder row and starts recognition without a name", async ({
  page,
}) => {
  await installJinniuMock(page, [CONNECTED]);

  // No label prompt: the click goes straight to IPC with label = null.
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  const adds = await jinniuRequests(page, "jinniu_account_add");
  expect(adds).toEqual([{ command: "jinniu_account_add", args: { label: null } }]);

  // The fresh row starts recognition immediately (add → login on the new id),
  // without waiting for a sub-account selection.
  await expect.poll(async () => jinniuRequests(page, "jinniu_login")).toEqual([
    { command: "jinniu_login", args: { id: "jinniu-2", options: null } },
  ]);
  await expect(page.getByTestId("jinniu-list")).toContainText("未命名金牛");
  await expect(status(page, "jinniu-2")).toHaveText("Starting");
  await expect(page.getByRole("status")).toContainText("Account created");
});

test("start opens the session without a confirm dialog; a pushed snapshot flips the badge", async ({
  page,
}) => {
  await installJinniuMock(page, [IDLE]);

  await row(page, "jinniu-2").getByRole("button", { name: "Start", exact: true }).click();
  // No confirm gate: the session start fires straight away.
  await expect.poll(async () => jinniuRequests(page, "jinniu_login")).toEqual([
    { command: "jinniu_login", args: { id: "jinniu-2", options: null } },
  ]);
  // The command's immediate snapshot shows `Starting`.
  await expect(status(page, "jinniu-2")).toHaveText("Starting");

  // The backend later pushes the running snapshot — patch in place.
  const listCallsBefore = (await jinniuRequests(page, "jinniu_accounts_list")).length;
  await page.evaluate(() => {
    (window as any).__TEST_IPC__.emit("jinniu-status-changed", {
      accountId: "jinniu-2",
      status: "connected",
      targetAccountId: "sub-9",
      error: null,
      master: { name: "乙主", id: "2002", avatarUrl: null },
      currentSubAccountId: "sub-9",
      currentSubAccountName: "子户九",
      balanceText: "¥ 99.00",
    });
  });
  await expect(status(page, "jinniu-2")).toHaveText("Running");
  await expect(row(page, "jinniu-2")).toContainText("子户九");
  // A status push patches the row without a full list re-read.
  expect((await jinniuRequests(page, "jinniu_accounts_list")).length).toBe(listCallsBefore);
});

test("stop tears the session down without a confirm dialog", async ({ page }) => {
  await installJinniuMock(page, [CONNECTED, IDLE]);

  await row(page, "jinniu-1").getByRole("button", { name: "Stop", exact: true }).click();
  await expect.poll(async () => jinniuRequests(page, "jinniu_disconnect")).toEqual([
    { command: "jinniu_disconnect", args: { id: "jinniu-1" } },
  ]);
  await expect(status(page, "jinniu-1")).toHaveText("Stopped");
  await expect(page.getByRole("status")).toContainText("Stopped.");
});

test("delete requires a second confirmation before removing the account", async ({ page }) => {
  await installJinniuMock(page, [CONNECTED, IDLE]);

  await row(page, "jinniu-1").getByRole("button", { name: "Delete", exact: true }).click();
  await expect(dialog(page)).toContainText("Delete account \"大户甲\"?");
  // First click only arms the destructive confirm; nothing is sent.
  expect(await jinniuRequests(page, "jinniu_account_remove")).toEqual([]);

  await dialog(page).getByRole("button", { name: "Delete", exact: true }).click();
  await expect.poll(async () => (await jinniuRequests(page, "jinniu_account_remove"))).toEqual([
    { command: "jinniu_account_remove", args: { id: "jinniu-1" } },
  ]);
  await expect(row(page, "jinniu-1")).toHaveCount(0);
  await expect(page.getByRole("status")).toContainText("Account deleted");
});

test("a list-changed push re-reads the accounts list", async ({ page }) => {
  await installJinniuMock(page, [IDLE]);
  const before = (await jinniuRequests(page, "jinniu_accounts_list")).length;

  // Flip the fixture, then deliver the payload-free event the way Tauri would.
  await setJinniuAccounts(page, [IDLE, AWAITING]);
  await page.evaluate(() => {
    (window as any).__TEST_IPC__.emit("jinniu-accounts-changed", {});
  });

  await expect(row(page, "jinniu-3")).toBeVisible();
  expect((await jinniuRequests(page, "jinniu_accounts_list")).length).toBeGreaterThan(before);
});
