import { expect, test, type Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";

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

  // Account-as-profile: deleting an account removes the browser profile, behind
  // a destructive confirm dialog.
  const calls: string[] = [];
  await page.exposeFunction("__recordShopCall", (command: string) => {
    calls.push(command);
  });
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "profiles_close" || command === "profiles_delete") {
        (window as any).__recordShopCall(command);
      }
      return original(command, args);
    };
  });

  await row.getByRole("button", { name: "Delete" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Delete this account?")).toBeVisible();
  await expect(dialog.getByText(/Regression profile/)).toBeVisible();
  await dialog.getByRole("button", { name: "Yes, delete" }).click();
  await expect(dialog).toHaveCount(0);
  expect(calls).toContain("profiles_close");
  expect(calls).toContain("profiles_delete");
});

/** An identity snapshot as the Rust backend reports it (detected, with an id). */
function identity(profileId: string, platformUserId: string, nickname: string | null) {
  return {
    profileId, status: "detected", platformUserId, nickname, avatarKey: null,
    checkedAt: "2026-10-04T00:00:00Z", lastSeenAt: "2026-10-04T00:00:00Z", message: null,
  };
}

/** One persisted `kuaishou_init_steps` row. */
function initStep(platformUserId: string, step: "subject" | "slice", state: string, lastErrorCode: string | null = null) {
  return { platformUserId, step, state, attempts: 1, nextRetryAt: null, lastErrorCode, completedAt: null, updatedAt: "2026-10-04T00:00:00Z" };
}

type ShopRow = { id: string; name: string };

/**
 * Local shop fixture: profiles + identity snapshots + init steps, injected before
 * the app boots so the first (and only) provider read already sees them. No
 * profile is launched and no platform request is made.
 */
async function installShopMock(
  page: Page,
  data: { profiles: ShopRow[]; snapshots: unknown[]; stepsByUser?: Record<string, unknown[]> },
): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.addInitScript((payload) => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
    localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("shop"));
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const mock = { calls: [] as Array<{ command: string; args: Record<string, any> }> };
    Object.assign(window, { __TEST_SHOP__: mock });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "profiles_list") {
        return payload.profiles.map((profile) => ({
          id: profile.id, name: profile.name, tags: [], group: null, icon: null,
          isRunning: false, timezone: "Asia/Shanghai",
        }));
      }
      if (command === "kuaishou_identity_list") return structuredClone(payload.snapshots);
      if (command === "kuaishou_identity_avatar") return null;
      if (command === "kuaishou_init_steps") {
        mock.calls.push({ command, args: structuredClone(args) });
        return structuredClone(payload.stepsByUser[args.platformUserId] ?? []);
      }
      return original(command, args);
    };
  }, { profiles: data.profiles, snapshots: data.snapshots, stepsByUser: data.stepsByUser ?? {} });
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Shop" })).toBeVisible();
}

/**
 * (A) After the wizard renames the environment from the detected nickname, the
 * name and the nickname are identical — the row must not show it twice.
 */
test("shop list shows the nickname only when it differs from the environment name", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [
      { id: "same-name", name: "本地小店" },
      { id: "other-name", name: "环境名不同" },
    ],
    snapshots: [
      identity("same-name", "00123456", "本地小店"), // nickname === row.name
      identity("other-name", "00999999", "平台昵称"), // nickname !== row.name
    ],
  });

  const table = page.getByRole("region", { name: "Shop" });
  const sameRow = table.getByRole("row").filter({ hasText: "本地小店" });
  await expect(sameRow).toBeVisible();
  // Exactly one occurrence: the duplicate secondary nickname is gone.
  expect(await sameRow.getByText("本地小店", { exact: true }).count()).toBe(1);

  // A genuinely different nickname is still shown as the secondary name.
  const otherRow = table.getByRole("row").filter({ hasText: "环境名不同" });
  await expect(otherRow.getByText("平台昵称", { exact: true })).toBeVisible();
  expect(await otherRow.getByText("环境名不同", { exact: true }).count()).toBe(1);
});

/**
 * (C) The initialization column renders the persisted per-account state, and it
 * only reads (`kuaishou_init_steps`) for rows that actually have a Kuaishou ID.
 */
test("shop list renders the initialization column and only reads rows with a Kuaishou ID", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [
      { id: "profile-done", name: "已完成小店" },
      { id: "profile-failed", name: "未完成小店" },
      { id: "profile-none", name: "无身份小店" },
    ],
    snapshots: [
      identity("profile-done", "11111111", "已完成小店"),
      identity("profile-failed", "22222222", "未完成小店"),
      // profile-none has no observed identity → no platformUserId.
    ],
    stepsByUser: {
      "11111111": [initStep("11111111", "subject", "done"), initStep("11111111", "slice", "done")],
      "22222222": [initStep("22222222", "subject", "failed", "validation-failed"), initStep("22222222", "slice", "done")],
    },
  });

  const table = page.getByRole("region", { name: "Shop" });
  await expect(table.getByRole("columnheader", { name: "Initialization" })).toBeVisible();

  // Both steps done → green "Done".
  const done = page.getByTestId("init-cell-profile-done");
  await expect(done).toHaveAttribute("data-state", "done");
  await expect(done).toHaveText("Done");

  // A failed step → amber "Not done", with the localized error in the tooltip.
  const failed = page.getByTestId("init-cell-profile-failed");
  await expect(failed).toHaveAttribute("data-state", "failed");
  await expect(failed).toHaveText("Not done");
  await expect(failed).toHaveAttribute("title", /Auto-validation failed/);

  // No Kuaishou ID → a dim placeholder, and no init read is issued for the row.
  const none = page.getByTestId("init-cell-profile-none");
  await expect(none).toHaveAttribute("data-state", "absent");
  await expect(none).toHaveText("—");

  const reads = await page.evaluate(() =>
    (window as any).__TEST_SHOP__.calls.filter((call: any) => call.command === "kuaishou_init_steps"),
  );
  // Only the two rows with a Kuaishou ID are read; the id-less row is never queried.
  expect(new Set(reads.map((call: any) => call.args.platformUserId))).toEqual(new Set(["11111111", "22222222"]));
});
