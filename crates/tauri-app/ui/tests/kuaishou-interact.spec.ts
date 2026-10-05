import { expect, test, type Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";
import type { BusinessAccount } from "../src/lib/businessAccounts";

/**
 * 互动账号（快手 › 互动账号）的最小 UI 契约。
 *
 * 环境化列表：账号 = 环境。登记记录来自 `list_sub_accounts`（kind=kuaishou-sub），
 * 运行态/身份 join `profiles_list` 与后端身份检测。建号走独立向导
 * （`KuaishouInteractWizard`），列表不再内嵌创建表单。
 *
 * 桌面布局专用：room / actions 列在窄视口会被 DataTable 依次丢弃，
 * 断言只在 desktop-chrome 有意义（照抄 kuaishou-shop.spec.ts 风格）。
 *
 * 全部 IPC 走本地 mock：只读 list 与受控 fixture，不写真实平台弹幕。
 */

interface InteractCall {
  command: string;
  args: Record<string, any>;
}

function fixtureAccount(
  patch: Partial<BusinessAccount> & { id: string; displayName: string },
): BusinessAccount {
  return {
    kind: "kuaishou-sub",
    platformUserId: null,
    profileId: "fixture-profile",
    createdAt: "2026-10-01T00:00:00Z",
    updatedAt: "2026-10-01T00:00:00Z",
    ...patch,
  };
}

const ALPHA = (): BusinessAccount =>
  fixtureAccount({ id: "interact-1", displayName: "Alpha One", platformUserId: "ks-alpha" });
const BETA = (): BusinessAccount =>
  fixtureAccount({ id: "interact-2", displayName: "Beta Two", platformUserId: "ks-beta" });
/** A record whose environment is gone (deleted / unbound): profile_id is NULL. */
const ORPHAN = (): BusinessAccount =>
  fixtureAccount({ id: "interact-3", displayName: "Gamma Three", profileId: null, platformUserId: null });

/** Land directly on 快手 › 互动账号 with a local sub-account IPC fake. */
async function gotoInteract(
  page: Page,
  accounts: BusinessAccount[],
  profiles?: Array<{ id: string; name: string; isRunning: boolean }>,
): Promise<void> {
  await installTauriMock(page);
  await page.addInitScript(({ accounts, profiles }) => {
    localStorage.setItem("cloaksession.test.interactAccounts", JSON.stringify(accounts));
    // Same landing mechanism as kuaishou-shop.spec.ts (persisted tab state).
    localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
    localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("interact"));
    const internals = (window as any).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    const store = { requests: [] as Array<{ command: string; args: Record<string, any> }> };
    (window as any).__TEST_INTERACT__ = store;
    const read = (): any[] =>
      JSON.parse(localStorage.getItem("cloaksession.test.interactAccounts") ?? "[]");
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      // Optional environment fixture: pin the running state (Stop vs Launch) or
      // simulate a deleted environment (a record with no matching profile row).
      if (profiles && command === "profiles_list") {
        return profiles.map((profile) => ({ ...profile, tags: [], timezone: "Asia/Shanghai" }));
      }
      const handled =
        command === "list_sub_accounts" ||
        command === "save_sub_account" ||
        command === "unbind_sub_account" ||
        command === "sub_account_login" ||
        command === "batch_login_sub_accounts" ||
        command === "sub_account_enter_live_room" ||
        command === "sub_account_send_danmaku" ||
        command === "sub_account_interactions" ||
        // Environment lifecycle (delete = delete the browser profile).
        command === "profiles_close" ||
        command === "profiles_delete";
      if (!handled) return originalInvoke(command, args);
      store.requests.push({ command, args: JSON.parse(JSON.stringify(args)) });
      if (command === "list_sub_accounts") return read();
      if (command === "save_sub_account") {
        const list = read();
        const next = {
          ...args.input,
          id: args.input.id ?? `interact-${list.length + 1}`,
          createdAt: "2026-10-04T00:00:00Z",
          updatedAt: "2026-10-04T00:00:00Z",
        };
        localStorage.setItem(
          "cloaksession.test.interactAccounts",
          JSON.stringify([...list.filter((a: any) => a.id !== next.id), next]),
        );
        return next;
      }
      if (command === "unbind_sub_account") {
        // The record is retained with profile_id=NULL (business_accounts has
        // ON DELETE SET NULL) — mirror that instead of removing the row.
        localStorage.setItem(
          "cloaksession.test.interactAccounts",
          JSON.stringify(
            read().map((a: any) => (a.id === args.id ? { ...a, profileId: null } : a)),
          ),
        );
        return null;
      }
      if (command === "profiles_close") return null;
      if (command === "profiles_delete") return null;
      if (command === "sub_account_login") {
        return { accountId: args.accountId, ok: true, error: null };
      }
      if (command === "batch_login_sub_accounts") {
        return (args.accountIds as string[]).map((id) => ({ accountId: id, ok: true, error: null }));
      }
      if (command === "sub_account_enter_live_room") return null;
      if (command === "sub_account_send_danmaku") {
        return {
          id: 1, accountId: args.accountId, sceneId: null, action: "danmaku",
          message: args.content, liveRoomUrl: null, ok: true, error: null,
          durationMs: 5, createdAt: 1728000000000,
        };
      }
      return read().filter((a: any) => a.accountId === args.accountId);
    };
  }, { accounts, profiles });
  await page.goto("/");
}

async function interactRequests(page: Page): Promise<InteractCall[]> {
  return page.evaluate(() => (window as any).__TEST_INTERACT__.requests);
}

const table = (page: Page) => page.getByRole("region", { name: "Interact" });
const scope = (page: Page) => page.getByTestId("interact-page");

function desktopOnly(testInfo: { project: { name: string } }): void {
  test.skip(testInfo.project.name !== "desktop-chrome", "desktop layout only");
}

test("interact tab renders environment-joined rows with a new-account action", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA(), BETA()]);

  const t = table(page);
  await expect(t).toBeVisible();
  await expect(t.getByRole("columnheader", { name: "Kuaishou ID" })).toBeVisible();
  await expect(t.getByRole("columnheader", { name: "Detection" })).toBeVisible();
  await expect(t.getByRole("columnheader", { name: "Actions" })).toBeVisible();

  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toBeVisible();
  await expect(t.getByRole("row").filter({ hasText: "Beta Two" })).toBeVisible();

  // The row action set: launch/stop + the retained live-room operations.
  const alpha = t.getByRole("row").filter({ hasText: "Alpha One" });
  await expect(alpha.getByRole("button", { name: "Launch", exact: true })).toBeVisible();
  await expect(alpha.getByRole("button", { name: "Enter room", exact: true })).toBeVisible();
  await expect(alpha.getByRole("button", { name: "Send", exact: true })).toBeVisible();
  await expect(alpha.getByRole("button", { name: "Sign in again", exact: true })).toBeVisible();
  await expect(alpha.getByRole("button", { name: "Delete", exact: true })).toBeVisible();

  await expect(
    scope(page).getByRole("button", { name: "New account", exact: true }),
  ).toBeVisible();

  // Sidebar uses pure labels (no shortcut suffix) — the only allowed selectors.
  await expect(page.getByRole("button", { name: "Kuaishou", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Interact", exact: true })).toBeVisible();
});

test("search filters by name, platform id, record id and profile id; no match shows the filtered empty state", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA(), BETA()]);

  const t = table(page);
  const search = scope(page).getByLabel("Search name, Kuaishou ID or profile ID", { exact: true });
  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toBeVisible();

  await search.fill("beta");
  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toHaveCount(0);
  await expect(t.getByRole("row").filter({ hasText: "Beta Two" })).toBeVisible();

  await search.fill("interact-1");
  await expect(t.getByRole("row").filter({ hasText: "Beta Two" })).toHaveCount(0);
  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toBeVisible();

  // The joined environment is searchable too.
  await search.fill("fixture-profile");
  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toBeVisible();

  await search.fill("zzz-no-match");
  await expect(t).toContainText("No account matches.");
});

test("empty store shows the create-ahead empty state", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, []);

  await expect(table(page)).toBeVisible();
  await expect(table(page)).toContainText("No interact accounts yet. Use New account to create one.");
  await expect(
    page.getByRole("button", { name: "New account", exact: true }),
  ).toBeVisible();
});

test("new account opens the wizard without firing any IPC write", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, []);

  await page.getByRole("button", { name: "New account", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Add interact account")).toBeVisible();
  // Same partitioned form as the shop wizard: home / proxy / extensions / fingerprint.
  await expect(dialog.getByRole("button", { name: "Home", exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Proxy", exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Extensions", exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Fingerprint", exact: true })).toBeVisible();
  // The home page is prefilled with the Kuaishou main site, not the shop console.
  await expect(dialog.getByRole("textbox")).toHaveValue("https://www.kuaishou.com/");

  // Opening the wizard must not create anything yet — the page's own mount read
  // of the record list is expected, a write is not.
  expect(
    (await interactRequests(page)).filter((call) => call.command !== "list_sub_accounts"),
  ).toEqual([]);
});

test("batch login is disabled without selection; enter/send need toolbar input; nothing is written", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA()]);

  const s = scope(page);
  const t = table(page);
  const row = t.getByRole("row").filter({ hasText: "Alpha One" });
  await expect(row).toBeVisible();

  await expect(s.getByRole("button", { name: "Batch login (0)", exact: true })).toBeDisabled();
  await expect(row.getByRole("button", { name: "Enter room", exact: true })).toBeDisabled();
  await expect(row.getByRole("button", { name: "Send", exact: true })).toBeDisabled();
  // Buttons that need no toolbar input stay available.
  await expect(row.getByRole("button", { name: "Launch", exact: true })).toBeEnabled();

  // Typing toolbar input enables the gated buttons without firing any IPC write.
  await s.getByLabel("Live room URL", { exact: true }).fill("https://live.kuaishou.com/u/fixture");
  await s.getByLabel("Danmaku content", { exact: true }).fill("fixture hello (mocked, never sent)");
  await expect(row.getByRole("button", { name: "Enter room", exact: true })).toBeEnabled();
  await expect(row.getByRole("button", { name: "Send", exact: true })).toBeEnabled();

  const writes = (await interactRequests(page)).filter((call) => call.command !== "list_sub_accounts");
  expect(writes).toEqual([]);
});

test("selecting a row enables batch login and renders mock login results", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA(), BETA()]);

  const s = scope(page);
  const t = table(page);
  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toBeVisible();

  await t.getByRole("checkbox", { name: "Select Alpha One" }).check();
  const batch = s.getByRole("button", { name: "Batch login (1)", exact: true });
  await expect(batch).toBeEnabled();
  await batch.click();

  await expect(s.getByTestId("interact-results")).toContainText("interact-1");
  await expect(s.getByRole("status")).toContainText("1 / 1 logged in.");

  const calls = await interactRequests(page);
  expect(calls.filter((call) => call.command === "batch_login_sub_accounts")).toEqual([
    { command: "batch_login_sub_accounts", args: { accountIds: ["interact-1"] } },
  ]);
  // Mock login only: no danmaku write may fire on this path.
  expect(calls.some((call) => call.command === "sub_account_send_danmaku")).toBe(false);
});

test("a running environment offers Stop instead of Launch, driven only by the profile fixture", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA()], [
    { id: "fixture-profile", name: "Alpha env", isRunning: true },
  ]);

  const t = table(page);
  const row = t.getByRole("row").filter({ hasText: "Alpha One" });
  await expect(row).toBeVisible();

  // Running → the row swaps Launch for Stop (they are mutually exclusive).
  await expect(row.getByRole("button", { name: "Stop", exact: true })).toBeEnabled();
  await expect(row.getByRole("button", { name: "Launch", exact: true })).toHaveCount(0);

  // The toggle is driven by the profile fixture alone; this page never writes to
  // the platform (only the read-only list calls are allowed).
  const writes = (await interactRequests(page)).filter((call) => call.command !== "list_sub_accounts");
  expect(writes).toEqual([]);
});

test("a record with no environment shows the missing pill and disables launch/stop/relogin", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ORPHAN()]);

  const t = table(page);
  const row = t.getByRole("row").filter({ hasText: "Gamma Three" });
  await expect(row).toBeVisible();

  // Environment-bound actions are disabled...
  await expect(row.getByText("Environment missing")).toBeVisible();
  await expect(row.getByRole("button", { name: "Launch", exact: true })).toBeDisabled();
  await expect(row.getByRole("button", { name: "Sign in again", exact: true })).toBeDisabled();
  // ...but the record can still be cleaned up (unbind / delete stay available).
  await expect(row.getByRole("button", { name: "Unbind", exact: true })).toBeEnabled();
  await expect(row.getByRole("button", { name: "Delete", exact: true })).toBeEnabled();
});

test("deleting a row confirms first, then removes the environment and unbinds the record", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA()]);

  const t = table(page);
  const row = t.getByRole("row").filter({ hasText: "Alpha One" });
  await row.getByRole("button", { name: "Delete", exact: true }).click();

  // Destructive action is gated behind a confirm dialog.
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Delete this account?")).toBeVisible();
  const before = await interactRequests(page);
  expect(before.some((call) => call.command === "profiles_delete")).toBe(false);

  await dialog.getByRole("button", { name: "Yes, delete", exact: true }).click();

  await expect
    .poll(async () => (await interactRequests(page)).filter((c) => c.command === "profiles_delete").length)
    .toBe(1);
  const calls = await interactRequests(page);
  expect(calls.filter((call) => call.command === "profiles_close")).toEqual([
    { command: "profiles_close", args: { id: "fixture-profile" } },
  ]);
  expect(calls.filter((call) => call.command === "profiles_delete")).toEqual([
    { command: "profiles_delete", args: { id: "fixture-profile" } },
  ]);
  // The registration is retained (profile_id -> NULL), never deleted.
  expect(calls.filter((call) => call.command === "unbind_sub_account")).toEqual([
    { command: "unbind_sub_account", args: { id: "interact-1" } },
  ]);
});

const PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6lqQAAAAASUVORK5CYII=";

/**
 * The full wizard path: create the environment → launch hidden on the Kuaishou
 * main site → poll the QR + identity → register the interact account. Every
 * command is a browser-local fake; no browser is launched and nothing is written
 * to the platform.
 */
test("interact wizard: create, hidden main-site launch, QR + detect, register", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await installInteractWizardMock(page, "zh-CN");

  await page.getByRole("button", { name: "新建账号", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("添加互动账号")).toBeVisible();
  // Prefilled with the Kuaishou main site (not the shop console).
  await expect(dialog.getByRole("textbox")).toHaveValue("https://www.kuaishou.com/");

  await dialog.getByRole("button", { name: "创建并登录", exact: true }).click();

  // The QR is captured from the hidden window and shown inside the wizard.
  await expect(dialog.getByTestId("interact-wizard-qr").locator("img")).toBeVisible();
  // Hidden launch through the new viewer entry, never the shop entry.
  expect(await wizardCalls(page, "profiles_launch")).toEqual([
    { id: "fixture-profile", entry: "kuaishou-sub", hidden: true },
  ]);

  const createCalls = await wizardCalls(page, "profiles_create");
  expect(createCalls).toHaveLength(1);
  const input = createCalls[0].input;
  expect(input.name).toBe("未命名");
  expect(input.startUrl).toBe("https://www.kuaishou.com/");
  // A random fingerprint seed is carried into the profile (Chromix owns identity).
  expect((input.chromixOptions?.args ?? []) as string[]).toEqual(
    expect.arrayContaining([expect.stringMatching(/^--fingerprint=\d+$/)]),
  );
  expect(input).not.toHaveProperty("fingerprint");

  // Once the identity is read the wizard names the environment from the account.
  await expect(dialog.getByText("00123456")).toBeVisible();
  await expect
    .poll(async () => (await wizardCalls(page, "profiles_update")).length)
    .toBeGreaterThan(0);
  expect(await wizardCalls(page, "profiles_update")).toEqual([
    { id: "fixture-profile", patch: { name: "本地小号" } },
  ]);

  // Registration writes the interact record with the detected identity.
  await dialog.getByRole("button", { name: "登记", exact: true }).click();
  await expect(dialog.getByText("已登记为互动账号。")).toBeVisible();
  expect(await wizardCalls(page, "save_sub_account")).toEqual([
    {
      input: {
        profileId: "fixture-profile",
        kind: "kuaishou-sub",
        displayName: "本地小号",
        platformUserId: "00123456",
      },
    },
  ]);

  // Done stops the hidden browser and closes the wizard; the registered
  // environment is never deleted.
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect((await wizardCalls(page, "profiles_close")).length).toBe(1);
  expect(await wizardCalls(page, "profiles_delete")).toEqual([]);
});

/**
 * Closing the wizard mid-wait must tear the QR/identity polling timer down: the
 * component stays mounted (the Modal only renders null), so the `open` gate in
 * the polling effect is what prevents a leaked screenshot loop.
 */
test("interact wizard: closing during the wait stops the QR/identity polling", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await installInteractWizardMock(page, "en");
  await page.evaluate(() => { (window as any).__TEST_INTERACT_WIZARD__.neverDetect = true; });

  await page.getByRole("button", { name: "New account", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Create & sign in", exact: true }).click();
  // Wait until the polling loop is actually running.
  await expect(dialog.getByTestId("interact-wizard-qr").locator("img")).toBeVisible();

  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);

  // Cancelling an onboarding that was never registered discards the environment
  // it just created, so no stray "unnamed" profile is left behind.
  expect(await wizardCalls(page, "profiles_close")).toEqual([{ id: "fixture-profile" }]);
  expect(await wizardCalls(page, "profiles_delete")).toEqual([{ id: "fixture-profile" }]);

  const qrAfterClose = (await wizardCalls(page, "kuaishou_login_qr")).length;
  const detectAfterClose = (await wizardCalls(page, "kuaishou_identity_detect")).length;
  // Longer than two poll intervals (QR_POLL_MS = 2000): a leaked timer would fire.
  await page.waitForTimeout(4500);
  expect((await wizardCalls(page, "kuaishou_login_qr")).length).toBe(qrAfterClose);
  expect((await wizardCalls(page, "kuaishou_identity_detect")).length).toBe(detectAfterClose);
});

/** Browser-local fake: no profile launch, no platform request. */
async function installInteractWizardMock(page: Page, language: "en" | "zh-CN" = "en"): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language });
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("kuaishou"));
    localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify("interact"));
    localStorage.setItem("cloaksession.test.interactAccounts", JSON.stringify([]));
  });
  await page.goto("/");
  await page.evaluate((png) => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const mock = {
      calls: [] as Array<{ command: string; args: Record<string, any> }>,
      detects: 0,
      neverDetect: false,
    };
    Object.assign(window, { __TEST_INTERACT_WIZARD__: mock });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (command === "list_sub_accounts") return [];
      if (command === "save_sub_account") {
        mock.calls.push({ command, args: structuredClone(args) });
        return {
          id: "interact-1",
          kind: "kuaishou-sub",
          displayName: args.input.displayName,
          platformUserId: args.input.platformUserId,
          profileId: args.input.profileId,
          createdAt: "2026-10-05T00:00:00Z",
          updatedAt: "2026-10-05T00:00:00Z",
        };
      }
      if (command === "profiles_create") {
        mock.calls.push({ command, args: structuredClone(args) });
        return {
          id: "fixture-profile",
          name: args.input.name,
          tags: [],
          dataDir: "/fixture/profile",
          createdAt: "2026-10-05T00:00:00Z",
          updatedAt: "2026-10-05T00:00:00Z",
          group: null,
        };
      }
      if (command === "profiles_launch" || command === "profiles_close" || command === "profiles_delete") {
        mock.calls.push({ command, args: structuredClone(args) });
        if (command === "profiles_launch") {
          return { id: args.id, cdpEndpoint: "http://127.0.0.1:9", pid: 1, startedAt: "2026-01-01T00:00:00Z" };
        }
        return undefined;
      }
      if (command === "profiles_update") {
        mock.calls.push({ command, args: structuredClone(args) });
        return {
          id: args.id,
          name: args.patch?.name ?? "未命名",
          tags: [],
          dataDir: "/fixture/profile",
          createdAt: "2026-10-05T00:00:00Z",
          updatedAt: "2026-10-05T00:00:00Z",
          group: null,
        };
      }
      if (command === "kuaishou_login_qr") {
        // base64 payload only (no data: prefix) — matches the Rust contract.
        return png.slice("data:image/png;base64,".length);
      }
      if (command === "kuaishou_identity_detect") {
        mock.calls.push({ command, args: structuredClone(args) });
        // Stay "not detected" on the first poll so the waiting + QR state is
        // observable, then report the signed-in viewer identity. `neverDetect`
        // keeps it waiting for the cancellation test.
        mock.detects += 1;
        if (mock.neverDetect || mock.detects < 2) {
          return {
            profileId: args.profileId, status: "not-detected", platformUserId: null, nickname: null,
            avatarKey: null, checkedAt: "2026-10-05T00:00:00Z", lastSeenAt: null, message: null,
          };
        }
        return {
          profileId: args.profileId, status: "detected", platformUserId: "00123456",
          nickname: "本地小号", avatarKey: null, checkedAt: "2026-10-05T00:00:00Z",
          lastSeenAt: "2026-10-05T00:00:00Z", message: null,
        };
      }
      return original(command, args);
    };
  }, PNG);
}

async function wizardCalls(page: Page, command: string) {
  return page.evaluate(
    (name) =>
      (window as any).__TEST_INTERACT_WIZARD__.calls
        .filter((call: any) => call.command === name)
        .map((call: any) => call.args),
    command,
  );
}

test("business page scrolls inside its table region and the drawer never covers row actions", async ({ page }, testInfo) => {
  desktopOnly(testInfo);
  await gotoInteract(page, [ALPHA(), BETA()]);

  const t = table(page);
  await expect(t.getByRole("row").filter({ hasText: "Alpha One" })).toBeVisible();

  // The table owns scrolling — the region must be overflow-auto, not the page body.
  const overflowY = await t.evaluate((element) => getComputedStyle(element).overflowY);
  expect(["auto", "scroll"]).toContain(overflowY);

  // No horizontal overflow on the business page at desktop width.
  const bounds = await scope(page).evaluate((element) => ({
    client: element.clientWidth,
    scroll: element.scrollWidth,
  }));
  expect(bounds.scroll).toBeLessThanOrEqual(bounds.client + 1);

  // The collapsed activity drawer (36px) sits below the content and never covers row actions.
  const send = t
    .getByRole("row")
    .filter({ hasText: "Alpha One" })
    .getByRole("button", { name: "Send", exact: true });
  await expect(send).toBeVisible();
  const sendBox = await send.boundingBox();
  const drawerBox = await page.getByText("MCP activity").boundingBox();
  expect(sendBox).not.toBeNull();
  expect(drawerBox).not.toBeNull();
  expect(sendBox!.y + sendBox!.height).toBeLessThanOrEqual(drawerBox!.y + 1);
});
