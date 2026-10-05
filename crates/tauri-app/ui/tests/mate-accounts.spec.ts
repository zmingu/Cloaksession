import { expect, test, type Page } from "@playwright/test";
import {
  emitMateState,
  failNextMateAdd,
  failNextMateStart,
  installMateMock,
  MATE_PLACEHOLDER_LABEL,
  mateFixture,
  mateRequests,
  setMateAccounts,
} from "./mateAccountsMock";

/**
 * 快手账号 › 直播伴侣（伴侣账号管理页）契约套件。
 *
 * IPC 全部浏览器内伪造：不启动环境、不打开浏览器、不发起任何真实扫码。
 * 基座 fixture 语言为 English，断言使用 en 字典文案。
 */
test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
});

const row = (page: Page, id: string) => page.getByTestId(`mate-row-${id}`);
const status = (page: Page, id: string) => page.getByTestId(`mate-status-${id}`);
const dialog = (page: Page) => page.getByRole("dialog");

/** The mate account row button whose accessible name is exactly `name`. */
const rowButton = (page: Page, id: string, name: string) =>
  row(page, id).getByRole("button", { name, exact: true });

const LOGGED_IN = mateFixture({
  id: "mate-1",
  label: "主号",
  platformUserId: "1001",
  userName: "甲主播",
  loginAt: 1_700_000_000_000,
});
const IDLE = mateFixture({ id: "mate-2", label: "测试号" });

test("list renders every account with a stage badge and the logged-in identity", async ({ page }) => {
  await installMateMock(page, [LOGGED_IN, IDLE]);

  await expect(page.getByTestId("mate-list")).toBeVisible();
  await expect(row(page, "mate-1")).toContainText("主号");
  await expect(row(page, "mate-2")).toContainText("测试号");

  // Persisted loginAt → 已登录; the untouched account stays 未登录.
  await expect(status(page, "mate-1")).toHaveText("Logged in");
  await expect(status(page, "mate-2")).toHaveText("Not logged in");
  await expect(row(page, "mate-1")).toContainText("甲主播 · UID 1001");
  await expect(row(page, "mate-2")).toContainText("Not logged in");

  // The button flips label for a logged-in account.
  await expect(rowButton(page, "mate-1", "Log in again")).toBeVisible();
  await expect(rowButton(page, "mate-2", "Scan to log in")).toBeVisible();

  const lists = await mateRequests(page, "mate_accounts_list");
  expect(lists.length).toBeGreaterThanOrEqual(1);
  // Entering the page also pulls each account's login snapshot.
  expect((await mateRequests(page, "mate_login_state")).length).toBeGreaterThanOrEqual(2);
});

test("empty state offers the add entry point", async ({ page }) => {
  await installMateMock(page, []);
  await expect(page.getByText("No companion accounts yet.", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add account", exact: true })).toBeEnabled();
});

test("adding an account creates it without a label and goes straight to the QR dialog", async ({
  page,
}) => {
  await installMateMock(page, [IDLE]);

  await page.getByRole("button", { name: "Add account", exact: true }).click();

  // No alias prompt any more: the add flow opens the QR dialog directly.
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page)).toContainText("Add a Companion account by scanning");
  await expect(dialog(page)).toContainText(
    "The account is named from the nickname returned by the scan.",
  );

  // `mate_account_add` carries no label; the freshly minted id then starts the
  // QR flow (the mock keeps the backend placeholder alias for the new row).
  // IDLE already owns `mate-2`, so the fixture mints `mate-3`.
  expect(await mateRequests(page, "mate_account_add")).toEqual([
    { command: "mate_account_add", args: {} },
  ]);
  expect(await mateRequests(page, "mate_login_start")).toEqual([
    { command: "mate_login_start", args: { accountId: "mate-3" } },
  ]);
  await expect(page.getByTestId("mate-list")).toContainText(MATE_PLACEHOLDER_LABEL);

  // `starting` has no image yet; the driver's `awaiting-scan` push fills it in.
  await emitMateState(page, "mate-3", "awaiting-scan", {
    qrImageDataUrl: "data:image/png;base64,fixture",
  });
  await expect(dialog(page).getByAltText("Login QR code")).toBeVisible();
});

test("renaming an account updates the row through IPC", async ({ page }) => {
  await installMateMock(page, [IDLE]);

  await rowButton(page, "mate-2", "Rename").click();
  await expect(dialog(page)).toBeVisible();
  const field = dialog(page).getByLabel("Label");
  await expect(field).toHaveValue("测试号");
  await field.fill("测试号-改");
  await dialog(page).getByRole("button", { name: "Save", exact: true }).click();

  await expect(dialog(page)).toHaveCount(0);
  expect(await mateRequests(page, "mate_account_rename")).toEqual([
    { command: "mate_account_rename", args: { id: "mate-2", label: "测试号-改" } },
  ]);
  await expect(row(page, "mate-2")).toContainText("测试号-改");
});

test("delete requires a second confirmation before removing the account", async ({ page }) => {
  await installMateMock(page, [LOGGED_IN, IDLE]);

  await rowButton(page, "mate-1", "Delete").click();
  await expect(dialog(page)).toContainText('Delete account "主号"?');
  // First click only arms the destructive confirm; nothing is sent.
  expect(await mateRequests(page, "mate_account_remove")).toEqual([]);

  await dialog(page).getByRole("button", { name: "Delete", exact: true }).click();
  await expect.poll(async () => (await mateRequests(page, "mate_account_remove"))).toEqual([
    { command: "mate_account_remove", args: { id: "mate-1" } },
  ]);
  await expect(row(page, "mate-1")).toHaveCount(0);
  await expect(page.getByRole("status")).toContainText("Account deleted");
});

test("scan → QR → stage progression → success names the row from the nickname", async ({
  page,
}) => {
  await installMateMock(page, [IDLE]);

  await rowButton(page, "mate-2", "Scan to log in").click();
  await expect(dialog(page)).toBeVisible();
  // The row has never logged in, so the dialog uses the "new account" copy.
  await expect(dialog(page)).toContainText("Add a Companion account by scanning");
  expect(await mateRequests(page, "mate_login_start")).toEqual([
    { command: "mate_login_start", args: { accountId: "mate-2" } },
  ]);
  // `starting` is pushed back as the immediate snapshot.
  await expect(status(page, "mate-2")).toHaveText("Fetching QR code");

  // Driver pushes `awaiting-scan` with the QR payload.
  await emitMateState(page, "mate-2", "awaiting-scan", {
    qrImageDataUrl: "data:image/png;base64,fixture",
  });
  await expect(dialog(page).getByAltText("Login QR code")).toBeVisible();
  await expect(status(page, "mate-2")).toHaveText("Waiting for scan");

  // `awaiting-confirm` carries the scanned user.
  await emitMateState(page, "mate-2", "awaiting-confirm", {
    qrImageDataUrl: "data:image/png;base64,fixture",
    user: { userId: "1001", userName: "乙主播", avatarUrl: null },
  });
  await expect(status(page, "mate-2")).toHaveText("Confirm on your phone");
  await expect(dialog(page)).toContainText("Scanned: 乙主播 · UID 1001");

  // The backend stamps the platform nickname over the alias on the first
  // successful scan; the `success` push re-reads the list, so the row title
  // becomes the nickname while the identity line keeps the scanned user.
  const listsBefore = (await mateRequests(page, "mate_accounts_list")).length;
  await setMateAccounts(page, [
    mateFixture({
      id: "mate-2",
      label: "乙主播",
      platformUserId: "1001",
      userName: "乙主播",
      loginAt: 1_700_000_111_000,
    }),
  ]);
  await emitMateState(page, "mate-2", "success", {
    user: { userId: "1001", userName: "乙主播", avatarUrl: null },
  });

  await expect(status(page, "mate-2")).toHaveText("Logged in");
  await expect(dialog(page)).toContainText("Logged in. Credentials saved.");
  await expect(row(page, "mate-2")).toContainText("乙主播");
  await expect(row(page, "mate-2")).toContainText("乙主播 · UID 1001");
  // Logged in now → the dialog switches off the "new account" title.
  await expect(dialog(page)).toContainText("Scan to log in 乙主播");
  expect((await mateRequests(page, "mate_accounts_list")).length).toBeGreaterThan(listsBefore);
});

test("adding an account then scanning names the row from the scanned nickname", async ({
  page,
}) => {
  await installMateMock(page, [IDLE]);

  // Add: no label → the row shows the backend placeholder until the scan.
  // IDLE already owns `mate-2`, so the fixture mints `mate-3`.
  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(page.getByTestId("mate-list")).toContainText(MATE_PLACEHOLDER_LABEL);

  // The driver delivers the scanned identity, then success overwrites the alias.
  await emitMateState(page, "mate-3", "awaiting-confirm", {
    qrImageDataUrl: "data:image/png;base64,fixture",
    user: { userId: "1001", userName: "乙主播", avatarUrl: null },
  });
  await setMateAccounts(page, [
    IDLE,
    mateFixture({
      id: "mate-3",
      label: "乙主播",
      platformUserId: "1001",
      userName: "乙主播",
      loginAt: 1_700_000_222_000,
    }),
  ]);
  await emitMateState(page, "mate-3", "success", {
    user: { userId: "1001", userName: "乙主播", avatarUrl: null },
  });

  await expect(row(page, "mate-3")).toContainText("乙主播");
  await expect(row(page, "mate-3")).toContainText("乙主播 · UID 1001");
  await expect(status(page, "mate-3")).toHaveText("Logged in");
});

test("an expired QR offers a refresh button", async ({ page }) => {
  await installMateMock(page, [IDLE]);

  await rowButton(page, "mate-2", "Scan to log in").click();
  await emitMateState(page, "mate-2", "expired", { errorMessage: "二维码已过期" });

  await expect(status(page, "mate-2")).toHaveText("QR code expired");
  const refresh = dialog(page).getByRole("button", { name: "Refresh QR code", exact: true });
  await expect(refresh).toBeVisible();

  // Refreshing restarts the flow for the same account.
  await refresh.click();
  await expect.poll(async () => (await mateRequests(page, "mate_login_start")).length).toBe(2);
});

test("closing the QR dialog cancels the in-flight flow", async ({ page }) => {
  await installMateMock(page, [IDLE]);

  await rowButton(page, "mate-2", "Scan to log in").click();
  await expect(dialog(page)).toBeVisible();

  // The footer "Close" button (the header X shares the same word).
  await dialog(page).getByRole("button", { name: "Close", exact: true }).last().click();
  await expect(dialog(page)).toHaveCount(0);
  expect(await mateRequests(page, "mate_login_cancel")).toEqual([
    { command: "mate_login_cancel", args: { accountId: "mate-2" } },
  ]);
});

/**
 * 占位行自动清理：一次「添加账号」若始终没有登录成功，就不能把
 * 「未命名伴侣」这行留在列表里。以下四条覆盖失败 / 过期 / 关闭三条路径，
 * 以及「已存在的账号不受影响」这一边界。
 */

test("closing the QR dialog of a new add drops the placeholder row", async ({ page }) => {
  await installMateMock(page, [IDLE]);

  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(page.getByTestId("mate-list")).toContainText(MATE_PLACEHOLDER_LABEL);
  // IDLE already owns `mate-2`, so the fixture mints `mate-3`.
  await expect(row(page, "mate-3")).toBeVisible();

  // Closing the dialog abandons the add: cancel the flow and delete the row.
  await dialog(page).getByRole("button", { name: "Close", exact: true }).last().click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(row(page, "mate-3")).toHaveCount(0);
  await expect(page.getByTestId("mate-list")).not.toContainText(MATE_PLACEHOLDER_LABEL);
  expect(await mateRequests(page, "mate_login_cancel")).toEqual([
    { command: "mate_login_cancel", args: { accountId: "mate-3" } },
  ]);
  expect(await mateRequests(page, "mate_account_remove")).toEqual([
    { command: "mate_account_remove", args: { id: "mate-3" } },
  ]);
  // The pre-existing account is untouched.
  await expect(row(page, "mate-2")).toContainText("测试号");
});

test("an expired QR for a new add drops the placeholder row", async ({ page }) => {
  await installMateMock(page, [IDLE]);

  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(row(page, "mate-3")).toBeVisible();

  await emitMateState(page, "mate-3", "expired", { errorMessage: "二维码已过期" });

  await expect(dialog(page)).toHaveCount(0);
  await expect(row(page, "mate-3")).toHaveCount(0);
  await expect.poll(async () => (await mateRequests(page, "mate_account_remove"))).toEqual([
    { command: "mate_account_remove", args: { id: "mate-3" } },
  ]);
  // The unrelated account survives.
  await expect(row(page, "mate-2")).toContainText("测试号");
});

test("a login error for a new add drops the placeholder row", async ({ page }) => {
  await installMateMock(page, [IDLE]);

  await page.getByRole("button", { name: "Add account", exact: true }).click();
  await expect(row(page, "mate-3")).toBeVisible();

  await emitMateState(page, "mate-3", "error", { errorMessage: "登录失败" });

  await expect(dialog(page)).toHaveCount(0);
  await expect(row(page, "mate-3")).toHaveCount(0);
  await expect.poll(async () => (await mateRequests(page, "mate_account_remove"))).toEqual([
    { command: "mate_account_remove", args: { id: "mate-3" } },
  ]);
  await expect(row(page, "mate-2")).toContainText("测试号");
});

test("a failed mate_account_add leaves no row behind", async ({ page }) => {
  await installMateMock(page, [IDLE]);
  await failNextMateAdd(page);

  await page.getByRole("button", { name: "Add account", exact: true }).click();

  await expect(page.getByRole("alert")).toContainText("Operation failed");
  await expect(dialog(page)).toHaveCount(0);
  // Nothing was created, so nothing has to be cleaned up either.
  await expect(page.getByTestId("mate-list")).not.toContainText(MATE_PLACEHOLDER_LABEL);
  expect(await mateRequests(page, "mate_account_remove")).toEqual([]);
  await expect(row(page, "mate-2")).toContainText("测试号");
});

test("a failed mate_login_start rolls the freshly created row back", async ({ page }) => {
  await installMateMock(page, [IDLE]);
  await failNextMateStart(page);

  await page.getByRole("button", { name: "Add account", exact: true }).click();

  await expect(page.getByRole("alert")).toContainText("Operation failed");
  await expect(dialog(page)).toHaveCount(0);
  await expect(row(page, "mate-3")).toHaveCount(0);
  await expect(page.getByTestId("mate-list")).not.toContainText(MATE_PLACEHOLDER_LABEL);
  // The row was created then removed during rollback.
  expect(await mateRequests(page, "mate_account_remove")).toEqual([
    { command: "mate_account_remove", args: { id: "mate-3" } },
  ]);
  await expect(row(page, "mate-2")).toContainText("测试号");
});
