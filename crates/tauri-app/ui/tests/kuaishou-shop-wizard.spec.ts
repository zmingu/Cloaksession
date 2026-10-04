import { expect, test, type Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";

const PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lqQAAAAASUVORK5CYII=";

/** The prefilled, editable home page for a shop account. */
const SHOP_HOME_URL = "https://s.kwaixiaodian.com/zone/home";

/**
 * 小店账号建号向导的最小 UI 契约（desktop only）。
 *
 * 全部命令都是浏览器本地 fake：不启动浏览器、不访问快手。
 * 覆盖：向导第一步是分区式（主页/代理/扩展/指纹，无 General）、主页预填小店首页、
 * 代理触发 Chromix GeoIP 对齐、创建后隐藏启动、二维码轮询、识别后自动命名、
 * 完成/取消的关闭语义（AC3/AC4/AC5/AC9）。
 */
test("shop account wizard: partitioned form (no General), shop home, create, QR, detect, done, close", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installWizardMock(page, "zh-CN");

  await page.getByRole("button", { name: "添加账号", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("添加小店账号")).toBeVisible();

  // Partitioned form: no General section and no legacy random-fingerprint panel.
  await expect(dialog.getByRole("button", { name: "General", exact: true })).toHaveCount(0);
  await expect(dialog.getByText(/Random fingerprint|随机指纹/)).toHaveCount(0);

  // The four Chinese partitions are visible.
  await expect(dialog.getByRole("button", { name: "主页", exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "代理", exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "扩展", exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "指纹", exact: true })).toBeVisible();

  // Home is prefilled with the shop home page (editable, not just a placeholder).
  await expect(dialog.getByRole("textbox")).toHaveValue(SHOP_HOME_URL);

  // Create directly — nothing else to fill in.
  await dialog.getByRole("button", { name: "创建并登录", exact: true }).click();

  // AC3: the QR is captured from the hidden browser and shown inside the wizard.
  const qr = dialog.getByTestId("wizard-qr");
  await expect(qr.locator("img")).toBeVisible();
  // Hidden launch: the wizard asks for `hidden: true`, never a visible window.
  expect(await wizardCalls(page, "profiles_launch")).toEqual([
    { id: "fixture-profile", entry: "kuaishou-shop", hidden: true },
  ]);

  // The profile is created with a placeholder name and the shop home page; the
  // dead CloakBrowser fingerprint is no longer sent (Chromix owns identity).
  const createCalls = await wizardCalls(page, "profiles_create");
  expect(createCalls).toHaveLength(1);
  const input = createCalls[0].input;
  expect(input.name).toBe("未命名");
  expect(input.startUrl).toBe(SHOP_HOME_URL);
  expect(input).not.toHaveProperty("fingerprint");
  // No proxy → no GeoIP alignment requested.
  expect(input.chromixOptions ?? {}).not.toHaveProperty("geoip");

  // AC4: once the identity is read, the wizard shows the account and a Done button.
  // The label flips to "已按识别到的账号命名" right after the rename.
  await expect(dialog.getByText(/已识别到账号|已按识别到的账号命名/)).toBeVisible();
  await expect(dialog.getByText("00123456")).toBeVisible();
  await expect(dialog.getByText("本地小店")).toBeVisible();
  // ...and renames the profile from the detected nickname.
  await expect
    .poll(async () => (await wizardCalls(page, "profiles_update")).length)
    .toBeGreaterThan(0);
  expect(await wizardCalls(page, "profiles_update")).toEqual([
    { id: "fixture-profile", patch: { name: "本地小店" } },
  ]);

  // AC9/close: Done stops the hidden browser and closes the wizard.
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect((await wizardCalls(page, "profiles_close")).length).toBe(1);
});

/**
 * A proxy on the form makes Chromix resolve the exit region (GeoIP), which is the
 * native way to align timezone/locale — replacing the old fingerprint reconcile.
 */
test("shop account wizard: a proxy requests Chromix GeoIP alignment on create", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installWizardMock(page, "zh-CN");

  await page.getByRole("button", { name: "添加账号", exact: true }).click();
  const dialog = page.getByRole("dialog");

  await dialog.getByRole("button", { name: "代理", exact: true }).click();
  await dialog.getByRole("checkbox").check();
  await dialog.getByPlaceholder("host or host:port:user:pass").fill("1.2.3.4");

  await dialog.getByRole("button", { name: "创建并登录", exact: true }).click();
  await expect(dialog.getByTestId("wizard-qr").locator("img")).toBeVisible();

  const createCalls = await wizardCalls(page, "profiles_create");
  expect(createCalls).toHaveLength(1);
  const input = createCalls[0].input;
  expect(input.proxy).toEqual(expect.objectContaining({ host: "1.2.3.4" }));
  expect(input.chromixOptions).toEqual(expect.objectContaining({ geoip: true }));

  // Clean up: cancel stops the hidden browser.
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await expect(dialog).toHaveCount(0);
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
async function installWizardMock(page: Page, language: "en" | "zh-CN" = "en"): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language });
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
      if (command === "profiles_update") {
        // Auto-naming: the wizard renames the created profile from the
        // detected identity. Echo a merged profile.
        mock.calls.push({ command, args: structuredClone(args) });
        return {
          id: args.id,
          name: args.patch?.name ?? "Unnamed",
          tags: [],
          dataDir: "/fixture/profile",
          createdAt: "2026-10-04T00:00:00Z",
          updatedAt: "2026-10-04T00:00:00Z",
          group: null,
        };
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
