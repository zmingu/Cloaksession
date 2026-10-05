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

  // One badge per status wire value, localized.
  await expect(status(page, "jinniu-1")).toHaveText("Connected");
  await expect(status(page, "jinniu-2")).toHaveText("Disconnected");
  await expect(status(page, "jinniu-3")).toHaveText("Awaiting sub-account");

  // Only the active account carries the marker.
  await expect(page.getByTestId("jinniu-active-jinniu-1")).toBeVisible();
  await expect(page.getByTestId("jinniu-active-jinniu-2")).toHaveCount(0);

  // The awaiting account surfaces the "pick a sub-account" hint; the others do not.
  await expect(row(page, "jinniu-3")).toContainText("pick the target sub-account");
  await expect(row(page, "jinniu-1")).not.toContainText("pick the target sub-account");

  // The sub-account is shown for the connected account.
  await expect(row(page, "jinniu-1")).toContainText("子户一");

  const lists = await jinniuRequests(page, "jinniu_accounts_list");
  expect(lists.length).toBeGreaterThanOrEqual(1);
});

test("empty state offers the add entry point", async ({ page }) => {
  await installJinniuMock(page, []);
  await expect(page.getByText("No accounts yet.", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add account", exact: true })).toBeEnabled();
});

test("adding a master account creates it through IPC and refreshes the list", async ({ page }) => {
  await installJinniuMock(page, [CONNECTED]);

  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(dialog(page)).toBeVisible();
  await dialog(page).getByLabel("Label").fill("新大户");
  await dialog(page).getByRole("button", { name: "Add", exact: true }).click();

  await expect(dialog(page)).toHaveCount(0);
  const adds = await jinniuRequests(page, "jinniu_account_add");
  expect(adds).toEqual([{ command: "jinniu_account_add", args: { label: "新大户" } }]);
  await expect(page.getByTestId("jinniu-list")).toContainText("新大户");
  await expect(page.getByRole("status")).toContainText("Account added: 新大户");
});

test("connect is confirm-gated and the pushed status snapshot flips the badge", async ({ page }) => {
  await installJinniuMock(page, [IDLE]);

  await row(page, "jinniu-2").getByRole("button", { name: "Connect", exact: true }).click();
  // The write is armed by the confirm dialog, not by the row click.
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page)).toContainText("Connect account \"大户乙\"?");
  expect(await jinniuRequests(page, "jinniu_login")).toEqual([]);

  await dialog(page).getByRole("button", { name: "Connect", exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  const logins = await jinniuRequests(page, "jinniu_login");
  expect(logins).toEqual([{ command: "jinniu_login", args: { id: "jinniu-2", options: null } }]);

  // The command's immediate snapshot shows `connecting` (scan ≠ connected).
  await expect(status(page, "jinniu-2")).toHaveText("Connecting");
  await expect(page.getByRole("status")).toContainText("Login started");

  // The backend later pushes `connected` (user picked a sub-account) — patch in place.
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
  await expect(status(page, "jinniu-2")).toHaveText("Connected");
  await expect(row(page, "jinniu-2")).toContainText("子户九");
  // A status push patches the row without a full list re-read.
  expect((await jinniuRequests(page, "jinniu_accounts_list")).length).toBe(listCallsBefore);
});

test("switch is confirm-gated and moves the single active marker", async ({ page }) => {
  await installJinniuMock(page, [CONNECTED, IDLE]);
  await expect(page.getByTestId("jinniu-active-jinniu-1")).toBeVisible();

  // The active row cannot switch onto itself.
  await expect(row(page, "jinniu-1").getByRole("button", { name: "Switch", exact: true })).toBeDisabled();

  await row(page, "jinniu-2").getByRole("button", { name: "Switch", exact: true }).click();
  await expect(dialog(page)).toContainText("Switch to account \"大户乙\"?");
  expect(await jinniuRequests(page, "jinniu_account_set_active")).toEqual([]);

  await dialog(page).getByRole("button", { name: "Switch", exact: true }).click();
  await expect.poll(async () => (await jinniuRequests(page, "jinniu_account_set_active"))).toEqual([
    { command: "jinniu_account_set_active", args: { id: "jinniu-2" } },
  ]);
  await expect(page.getByTestId("jinniu-active-jinniu-2")).toBeVisible();
  await expect(page.getByTestId("jinniu-active-jinniu-1")).toHaveCount(0);
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
