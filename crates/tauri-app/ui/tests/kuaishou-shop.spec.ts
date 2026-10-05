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
type ShopSub = { id: string; kind: string; profileId: string | null; displayName: string; platformUserId: string | null };

/** A 1×1 PNG as a bare base64 payload (no `data:` prefix), matching the Rust contract. */
const QR_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lqQAAAAASUVORK5CYII=";

/**
 * Local shop fixture: profiles + identity snapshots + init steps + interact
 * sub-account registrations, injected before the app boots so the first (and
 * only) provider read already sees them. No profile is launched and no platform
 * request is made.
 *
 * `running` seeds which environments report `isRunning: true`; the mutable
 * `__TEST_SHOP__.running` array lets a test flip it before delivering a
 * synthetic `profiles:running-changed` event.
 *
 * `subs` seeds `list_sub_accounts` and `business_accounts_list` — the shop
 * list exclusion reads the shared business-account table (kuaishou-sub +
 * jinniu kinds), and the conversion dialog writes through save_sub_account,
 * so both commands serve the same store. `detect` decides what
 * `kuaishou_identity_detect` reports while the conversion dialog polls:
 * "never" keeps it waiting, "detected" reports a signed-in main-site identity.
 */
async function installShopMock(
  page: Page,
  data: {
    profiles: ShopRow[];
    snapshots: unknown[];
    stepsByUser?: Record<string, unknown[]>;
    running?: string[];
    subs?: ShopSub[];
    detect?: "never" | "detected";
  },
): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language: "en" });
  await page.addInitScript((payload) => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
    localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("shop"));
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const mock = {
      calls: [] as Array<{ command: string; args: Record<string, any> }>,
      running: payload.running,
      subs: payload.subs,
    };
    Object.assign(window, { __TEST_SHOP__: mock });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "profiles_list") {
        mock.calls.push({ command, args: structuredClone(args) });
        return payload.profiles.map((profile) => ({
          id: profile.id, name: profile.name, tags: [], group: null, icon: null,
          isRunning: mock.running.includes(profile.id), timezone: "Asia/Shanghai",
        }));
      }
      if (command === "kuaishou_identity_list") return structuredClone(payload.snapshots);
      if (command === "kuaishou_identity_avatar") return null;
      if (command === "kuaishou_init_steps") {
        mock.calls.push({ command, args: structuredClone(args) });
        return structuredClone(payload.stepsByUser[args.platformUserId] ?? []);
      }
      // --- interact sub-account registration (conversion target) ---------
      if (command === "business_accounts_list") {
        mock.calls.push({ command, args: structuredClone(args) });
        return structuredClone(mock.subs);
      }
      if (command === "list_sub_accounts") {
        mock.calls.push({ command, args: structuredClone(args) });
        return structuredClone(mock.subs);
      }
      if (command === "save_sub_account") {
        mock.calls.push({ command, args: structuredClone(args) });
        const input = args.input ?? {};
        const id: string = input.id ?? `sub-${mock.subs.length + 1}`;
        const record = {
          id, kind: input.kind, displayName: input.displayName,
          platformUserId: input.platformUserId ?? null, profileId: input.profileId,
          createdAt: "2026-10-05T00:00:00Z", updatedAt: "2026-10-05T00:00:00Z",
        };
        mock.subs = [...mock.subs.filter((s: any) => s.id !== id), record];
        return structuredClone(record);
      }
      if (command === "unbind_sub_account") {
        mock.calls.push({ command, args: structuredClone(args) });
        mock.subs = mock.subs.map((s: any) => (s.id === args.id ? { ...s, profileId: null } : s));
        return undefined;
      }
      // --- environment lifecycle + main-site login -----------------------
      if (command === "profiles_launch" || command === "profiles_close" || command === "profiles_delete") {
        mock.calls.push({ command, args: structuredClone(args) });
        if (command === "profiles_launch") {
          return { id: args.id, cdpEndpoint: "http://127.0.0.1:9", pid: 1, startedAt: "2026-01-01T00:00:00Z" };
        }
        return undefined;
      }
      if (command === "kuaishou_login_qr") {
        mock.calls.push({ command, args: structuredClone(args) });
        return payload.qr;
      }
      if (command === "kuaishou_identity_detect") {
        mock.calls.push({ command, args: structuredClone(args) });
        if (payload.detect === "detected") {
          return {
            profileId: args.profileId, status: "detected", platformUserId: "00123456",
            nickname: "已转互动", avatarKey: null, checkedAt: "2026-10-05T00:00:00Z",
            lastSeenAt: "2026-10-05T00:00:00Z", message: null,
          };
        }
        return {
          profileId: args.profileId, status: "not-detected", platformUserId: null, nickname: null,
          avatarKey: null, checkedAt: "2026-10-05T00:00:00Z", lastSeenAt: null, message: null,
        };
      }
      return original(command, args);
    };
  }, {
    profiles: data.profiles,
    snapshots: data.snapshots,
    stepsByUser: data.stepsByUser ?? {},
    running: data.running ?? [],
    subs: data.subs ?? [],
    detect: data.detect ?? "never",
    qr: QR_PNG,
  });
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Shop" })).toBeVisible();
}

/** Args of every recorded `command` call made through the shop fixture. */
async function shopCalls(page: Page, command: string): Promise<Record<string, any>[]> {
  return page.evaluate(
    (name) =>
      (window as any).__TEST_SHOP__.calls
        .filter((call: any) => call.command === name)
        .map((call: any) => call.args),
    command,
  );
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

  // Column order: the initialization column sits right after the Kuaishou ID and
  // before the detection status.
  const headers = await table.getByRole("columnheader").allTextContents();
  const idIdx = headers.indexOf("Kuaishou ID");
  const initIdx = headers.indexOf("Initialization");
  const statusIdx = headers.indexOf("Detection");
  expect(idIdx).toBeGreaterThanOrEqual(0);
  expect(initIdx).toBe(idIdx + 1);
  expect(statusIdx).toBe(initIdx + 1);

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

/**
 * (B) The avatar + name block is a single accessible button that opens the shared
 * Kuaishou identity dialog (mounted once at the app root — no local popup here).
 */
test("clicking the account avatar or name opens the shared Kuaishou detail dialog", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [{ id: "detail-profile", name: "本地小店" }],
    snapshots: [identity("detail-profile", "00123456", "平台昵称")],
  });

  const table = page.getByRole("region", { name: "Shop" });
  const row = table.getByRole("row").filter({ hasText: "本地小店" });
  await expect(row).toBeVisible();

  // The whole avatar+name block is one button, labelled with the account name.
  await row.getByRole("button", { name: "View account info: 本地小店", exact: true }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("快手详情", { exact: true })).toBeVisible();
  const detail = page.getByTestId("kuaishou-detail");
  await expect(detail).toBeVisible();
  await expect(detail).toContainText("00123456");
  await expect(detail).toContainText("平台昵称");
});

/**
 * (A) The list subscribes to `profiles:running-changed` and refetches, so an
 * environment that dies on its own (window closed directly) converges without
 * the user touching the page.
 */
test("a profiles:running-changed event refreshes the shop list automatically", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [{ id: "running-profile", name: "运行中小店" }],
    snapshots: [identity("running-profile", "00123456", "运行中小店")],
    running: ["running-profile"],
  });

  const table = page.getByRole("region", { name: "Shop" });
  const row = table.getByRole("row").filter({ hasText: "运行中小店" });
  await expect(row.getByRole("button", { name: "Stop" })).toBeVisible();
  const listCalls = () =>
    page.evaluate(() => (window as any).__TEST_SHOP__.calls.filter((call: any) => call.command === "profiles_list").length);
  const before = await listCalls();

  // Flip the fixture (the process is gone) and deliver the backend event the way
  // Tauri would; no local button is pressed.
  await page.evaluate(() => {
    const mock = (window as any).__TEST_SHOP__;
    mock.running = [];
    (window as any).__TEST_IPC__.emit("profiles:running-changed", {
      kind: "closed", profileId: "running-profile", reason: "external-exit",
    });
  });

  await expect(row.getByRole("button", { name: "Launch" })).toBeVisible();
  expect(await listCalls()).toBeGreaterThan(before);
});

/**
 * (D) The shop list offers「转为互动账号」on every row, and clicking it drives the
 * conversion: bind `save_sub_account(kind=kuaishou-sub)` first, then launch the
 * environment hidden on the Kuaishou main site (`profiles_launch(entry=kuaishou-sub)`).
 */
test("shop row offers Convert to interact account and binds + launches the main site", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [{ id: "convert-profile", name: "待转小店" }],
    snapshots: [identity("convert-profile", "00123456", "待转小店")],
    detect: "never",
  });

  const table = page.getByRole("region", { name: "Shop" });
  const row = table.getByRole("row").filter({ hasText: "待转小店" });
  await expect(row).toBeVisible();

  const convertButton = row.getByRole("button", { name: "Convert to interact account" });
  await expect(convertButton).toBeVisible();
  await convertButton.click();

  // The conversion dialog opens on the environment name.
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Convert to interact account", { exact: true })).toBeVisible();

  // Bind first (identity scope narrows to the main site), then the hidden launch.
  // Exactly once — StrictMode's effect replay must not double-bind or relaunch.
  await expect
    .poll(async () => (await shopCalls(page, "save_sub_account")).length)
    .toBe(1);
  const saves = await shopCalls(page, "save_sub_account");
  expect(saves[0].input).toMatchObject({
    profileId: "convert-profile",
    kind: "kuaishou-sub",
    platformUserId: null,
  });

  await expect
    .poll(async () => (await shopCalls(page, "profiles_launch")).length)
    .toBe(1);
  expect((await shopCalls(page, "profiles_launch"))[0]).toEqual({
    id: "convert-profile",
    entry: "kuaishou-sub",
    hidden: true,
  });
  // The profile is stopped before binding (the business guard requires it).
  expect(await shopCalls(page, "profiles_close")).toEqual([{ id: "convert-profile" }]);
});

/**
 * (E) An environment already bound as an interact account (`kind=kuaishou-sub`)
 * is filtered out of the shop list — it belongs to the interact list.
 */
test("environments bound as kuaishou-sub are hidden from the shop list", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [
      { id: "shop-profile", name: "小店环境" },
      { id: "bound-profile", name: "已绑定互动" },
    ],
    snapshots: [],
    subs: [
      {
        id: "sub-1", kind: "kuaishou-sub", profileId: "bound-profile",
        displayName: "已绑定互动", platformUserId: "00123456",
      },
    ],
  });

  const table = page.getByRole("region", { name: "Shop" });
  await expect(table.getByRole("row").filter({ hasText: "小店环境" })).toBeVisible();
  await expect(table.getByRole("row").filter({ hasText: "已绑定互动" })).toHaveCount(0);
});

/**
 * (F) The full conversion path: once the main-site identity is read, the record
 * is finalized (platformUserId + nickname) and the environment leaves the shop
 * list. Nothing deletes the environment.
 */
test("converting finalizes the record and removes the environment from the shop list", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [{ id: "convert-profile", name: "待转小店" }],
    snapshots: [],
    detect: "detected",
  });

  const table = page.getByRole("region", { name: "Shop" });
  const row = table.getByRole("row").filter({ hasText: "待转小店" });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Convert to interact account" }).click();

  // The finalized save carries the detected identity.
  await expect
    .poll(async () => (await shopCalls(page, "save_sub_account")).length)
    .toBe(2);
  const saves = await shopCalls(page, "save_sub_account");
  expect(saves[1].input).toMatchObject({
    profileId: "convert-profile",
    kind: "kuaishou-sub",
    platformUserId: "00123456",
  });
  expect(typeof saves[1].input.id).toBe("string");

  // Auto-close after the success state, then the row is filtered out.
  await expect(table.getByRole("row").filter({ hasText: "待转小店" })).toHaveCount(0);
  // The environment itself is never deleted.
  expect(await shopCalls(page, "profiles_delete")).toHaveLength(0);
});

/**
 * (G) Cancelling a conversion closes the browser and rolls back this run's bind,
 * but never deletes the pre-existing environment.
 */
test("cancelling a conversion closes the browser, unbinds and never deletes the environment", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
  await installShopMock(page, {
    profiles: [{ id: "convert-profile", name: "待转小店" }],
    snapshots: [],
    detect: "never",
  });

  const table = page.getByRole("region", { name: "Shop" });
  const row = table.getByRole("row").filter({ hasText: "待转小店" });
  await row.getByRole("button", { name: "Convert to interact account" }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect
    .poll(async () => (await shopCalls(page, "save_sub_account")).length)
    .toBe(1);

  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);

  expect(await shopCalls(page, "profiles_close")).toHaveLength(2);
  expect((await shopCalls(page, "unbind_sub_account"))[0]).toEqual({ id: "sub-1" });
  expect(await shopCalls(page, "profiles_delete")).toHaveLength(0);
  // The environment is still in the shop list after the rollback.
  await expect(table.getByRole("row").filter({ hasText: "待转小店" })).toBeVisible();
});
