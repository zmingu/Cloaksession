import { expect, test, type Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

const PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lqQAAAAASUVORK5CYII=";

/**
 * 小店账号建号向导的最小 UI 契约（desktop only）。
 *
 * 全部命令都是浏览器本地 fake：不启动浏览器、不访问快手。
 * 覆盖：向导可打开、名称必填、创建后显示二维码、识别到身份后显示完成、
 * 关闭时隐藏的浏览器被关闭（AC3/AC4/AC5/AC9）。
 */
test("shop account wizard: create, show QR, detect, done, close hidden browser", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installWizardMock(page);

  await page.getByRole("button", { name: "Add account", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Add shop account")).toBeVisible();

  // AC2: name is required before the wizard will create anything.
  await dialog.getByRole("button", { name: "Create & sign in", exact: true }).click();
  await expect(dialog.getByText("Enter an account name")).toBeVisible();

  await dialog.getByPlaceholder("e.g. Evening shop").fill("晚场小店");
  await dialog.getByRole("button", { name: "Create & sign in", exact: true }).click();

  // AC3: the QR is captured from the hidden browser and shown inside the wizard.
  const qr = dialog.getByTestId("wizard-qr");
  await expect(qr.locator("img")).toBeVisible();
  // Hidden launch: the wizard asks for `hidden: true`, never a visible window.
  expect(await wizardCalls(page, "profiles_launch")).toEqual([
    { id: "fixture-profile", entry: "kuaishou-shop", hidden: true },
  ]);

  // AC4: once the identity is read, the wizard shows the account and a Done button.
  await expect(dialog.getByText("Account detected")).toBeVisible();
  await expect(dialog.getByText("00123456")).toBeVisible();
  await expect(dialog.getByText("本地小店")).toBeVisible();

  // AC9/close: Done stops the hidden browser and closes the wizard.
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect((await wizardCalls(page, "profiles_close")).length).toBe(1);
});

async function wizardCalls(page: Page, command: string) {
  return page.evaluate(
    (name) => (window as any).__TEST_WIZARD__.calls.filter((call: any) => call.command === name).map((call: any) => call.args),
    command,
  );
}

/**
 * AC9: cancelling the wizard after the profile was created must stop the QR
 * capture / identity polling loop (no leaked timer keeps screenshotting).
 */
test("shop account wizard: cancelling the wait stops the QR/identity polling", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installWizardMock(page);

  await page.getByRole("button", { name: "Add account", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await page.evaluate(() => { (window as any).__TEST_WIZARD__.neverDetect = true; });
  await dialog.getByPlaceholder("e.g. Evening shop").fill("晚场小店");
  await dialog.getByRole("button", { name: "Create & sign in", exact: true }).click();
  // Wait until the polling loop is actually running.
  await expect(dialog.getByTestId("wizard-qr").locator("img")).toBeVisible();

  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);

  const qrAfterClose = (await wizardCalls(page, "kuaishou_login_qr")).length;
  const detectAfterClose = (await wizardCalls(page, "kuaishou_identity_detect")).length;
  // Longer than two poll intervals: a leaked timer would fire again here.
  await page.waitForTimeout(4500);
  expect((await wizardCalls(page, "kuaishou_login_qr")).length).toBe(qrAfterClose);
  expect((await wizardCalls(page, "kuaishou_identity_detect")).length).toBe(detectAfterClose);
});

/** Browser-local fake: no profile launch, no platform request. */
async function installWizardMock(page: Page): Promise<void> {
  await installTauriMock(page);
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
    localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("shop"));
  });
  await page.goto("/");
  await page.evaluate((png) => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const mock = {
      calls: [] as Array<{ command: string; args: Record<string, any> }>,
      detects: 0,
    };
    Object.assign(window, { __TEST_WIZARD__: mock });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "profiles_create") {
        mock.calls.push({ command, args: structuredClone(args) });
        return {
          id: "fixture-profile",
          name: args.input.name,
          tags: [],
          dataDir: "/fixture/profile",
          createdAt: "2026-10-04T00:00:00Z",
          updatedAt: "2026-10-04T00:00:00Z",
          group: null,
        };
      }
      if (command === "profiles_launch" || command === "profiles_close") {
        mock.calls.push({ command, args: structuredClone(args) });
        if (command === "profiles_launch") return { id: args.id, cdpEndpoint: "http://127.0.0.1:9", pid: 1, startedAt: "2026-01-01T00:00:00Z" };
        return undefined;
      }
      if (command === "kuaishou_login_qr") {
        mock.calls.push({ command, args: structuredClone(args) });
        // base64 payload only (no data: prefix) — matches the Rust contract.
        return png.slice("data:image/png;base64,".length);
      }
      if (command === "kuaishou_identity_detect") {
        mock.calls.push({ command, args: structuredClone(args) });
        // Stay "not detected" on the first poll so the waiting + QR state is
        // observable, then report the signed-in identity. `neverDetect` keeps
        // it in the waiting state for the AC9 cancellation test.
        mock.detects += 1;
        if (mock.neverDetect || mock.detects < 2) {
          return {
            profileId: args.profileId,
            status: "not-detected",
            platformUserId: null,
            nickname: null,
            avatarKey: null,
            checkedAt: "2026-10-04T00:00:00Z",
            lastSeenAt: null,
            message: null,
          };
        }
        return {
          profileId: args.profileId,
          status: "detected",
          platformUserId: "00123456",
          nickname: "本地小店",
          avatarKey: null,
          checkedAt: "2026-10-04T00:00:00Z",
          lastSeenAt: "2026-10-04T00:00:00Z",
          message: null,
        };
      }
      return original(command, args);
    };
  }, PNG);
}
