import { expect, test, type Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/**
 * Live launch (伴侣开播) and live-room-monitor (直播间监控) contract suite.
 *
 * 业务板块归位后：伴侣开播（LiveLaunchPage，本地视频循环推流）在
 * 「直播 › 开播准备」，与慧播开播并列；伴侣账号管理（MateLoginPage）在
 * 「快手账号 › 直播伴侣」；直播间监控在「直播 › 直播互动 › 互动账号脚本互动」。
 * 小店鉴权页已删除，故本文件保留这些命令的契约。
 */
export const ACCOUNTS_LIVE_COMMANDS = [
  "mate_accounts_list",
  "mate_account_add",
  "mate_account_remove",
  "mate_login_start",
  "mate_login_cancel",
  "mate_login_state",
  "start_live_room_monitor",
  "stop_live_room_monitor",
  "get_live_room_monitor_state",
] as const;

/** Alias the backend stamps on a row added without a label (mirrors Rust). */
export const MATE_PLACEHOLDER_LABEL = "未命名伴侣";

interface RecordedCall {
  command: string;
  args: Record<string, unknown>;
}

/**
 * Extends the shared Tauri mock with the mate-login / monitor commands and
 * boots straight into the Live section. Installed via addInitScript, whose
 * callback body is serialized into the page — so it must not reference module
 * scope.
 */
async function installAccountsLiveMock(page: Page): Promise<void> {
  await installTauriMock(page);
  await page.addInitScript(
    ({ allowed, placeholder }: { allowed: string[]; placeholder: string }) => {
      localStorage.setItem("multizen.ui.section", JSON.stringify("live"));
      const idleMonitor = () => ({
        enabled: false, profileId: null, liveRoomUrl: null,
        sceneId: null, groupId: null, productScriptId: null,
        productScriptAccountId: null, autoExitSubAccounts: false,
        status: "idle", liveStatus: "unknown",
        triggeredForCurrentLive: false, enteringRooms: false, exitingRooms: false,
        lastCheckedAt: null, nextCheckAt: null, lastTriggeredAt: null,
        lastEnterAllResult: null, lastExitAllResult: null,
        lastProductScriptResult: null, error: null,
      });
      const internals = (window as unknown as {
        __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> };
      }).__TAURI_INTERNALS__;
      const originalInvoke = internals.invoke;
      const recorded: RecordedCall[] = [];
      // The mate tab is an account manager: it lists accounts first and only
      // scans for the account the user picks. Start empty and grow on add.
      const mateAccounts: Array<Record<string, unknown>> = [];

      internals.invoke = async (command: string, args: Record<string, unknown> = {}) => {
        if (allowed.includes(command)) {
          recorded.push({ command, args: JSON.parse(JSON.stringify(args)) });
        }
        switch (command) {
          case "mate_accounts_list":
            return JSON.parse(JSON.stringify(mateAccounts));
          case "mate_account_add": {
            const row = {
              id: `mate-${mateAccounts.length + 1}`,
              // No label → the backend falls back to the placeholder alias.
              label: args["label"] === undefined ? placeholder : String(args["label"]),
              platformUserId: null, userName: null, avatarUrl: null,
              matePassToken: null, mateToken: null, mateSt: null, mateH5St: null,
              mateLmtoken: null, loginAt: null,
              createdAt: "2026-01-02T00:00:00.000Z", updatedAt: "2026-01-02T00:00:00.000Z",
            };
            mateAccounts.push(row);
            return JSON.parse(JSON.stringify(row));
          }
          case "mate_account_remove": {
            const id = String(args["id"]);
            const index = mateAccounts.findIndex((entry) => entry["id"] === id);
            if (index >= 0) mateAccounts.splice(index, 1);
            return;
          }
          case "mate_login_start":
          case "mate_login_cancel":
          case "mate_login_state":
            return {
              accountId: String(args["accountId"]), stage: "awaiting-scan",
              qrImageDataUrl: "data:image/png;base64,fixture",
              qrLoginToken: null, qrLoginSignature: null, expireAt: null,
              errorMessage: null, user: null, startedAt: null, finishedAt: null,
            };
          case "start_live_room_monitor":
            return {
              ...idleMonitor(), enabled: true,
              profileId: String(args["profileId"]),
              liveRoomUrl: (args["config"] as { liveRoomUrl: string }).liveRoomUrl,
              status: "checking",
            };
          case "stop_live_room_monitor":
          case "get_live_room_monitor_state":
            return idleMonitor();
          default:
            return originalInvoke(command, args);
        }
      };
      (window as unknown as { __TEST_A_CALLS__: RecordedCall[] }).__TEST_A_CALLS__ = recorded;
    },
    { allowed: ACCOUNTS_LIVE_COMMANDS as unknown as string[], placeholder: MATE_PLACEHOLDER_LABEL },
  );
  await page.goto("/");
}

async function calls(page: Page, command?: string): Promise<RecordedCall[]> {
  return page.evaluate((cmd) => {
    const all = (window as unknown as { __TEST_A_CALLS__: RecordedCall[] }).__TEST_A_CALLS__;
    return cmd === null ? all : all.filter((c) => c.command === cmd);
  }, command ?? null);
}

async function commands(page: Page): Promise<string[]> {
  return (await calls(page)).map((c) => c.command);
}

test.beforeEach(async ({ page }) => {
  await installAccountsLiveMock(page);
});

/** The mate QR login page lives under 快手账号 › 直播伴侣 (Companion). */
async function openMateTab(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Kuaishou account", exact: true }).click();
  await page.getByRole("button", { name: "Companion", exact: true }).click();
}

test("live section exposes the three tabs and prepare hosts both launch pages", async ({ page }) => {
  await expect(page.getByRole("tab", { name: "Live prep" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Live interact" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Ad delivery" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Live prep" })).toHaveAttribute("aria-selected", "true");
  // 开播准备 = 慧播开播（跟播 / 回播）+ 伴侣开播（LiveLaunchPage 本地视频推流）。
  const huibo = page.getByTestId("live-prepare-huibo");
  await expect(huibo.getByRole("heading", { name: "Huibo live", exact: true })).toBeVisible();
  await expect(huibo.getByRole("region", { name: "Huibo live (replay)" })).toBeVisible();
  const mate = page.getByTestId("live-prepare-mate");
  await expect(mate.getByRole("heading", { name: "Companion live", exact: true })).toBeVisible();
  await expect(mate.getByRole("region", { name: "Live launch" })).toBeVisible();
  // 伴侣账号管理页不再属于开播准备。
  await expect(page.getByRole("region", { name: "Companion accounts" })).toHaveCount(0);
});

test("Kuaishou › Companion tab hosts the mate account manager", async ({ page }) => {
  await openMateTab(page);
  const login = page.getByRole("region", { name: "Companion accounts" });
  await expect(login).toBeVisible();
  // An account manager, not a bare account-id input: add + refresh entries.
  await expect(login.getByRole("button", { name: "Add account", exact: true })).toBeVisible();
  await expect(login.getByRole("button", { name: "Refresh", exact: true })).toBeVisible();
  await expect(login.getByPlaceholder("account-id")).toHaveCount(0);
});

test("mate login: adding an account opens the QR dialog and starts the flow with the new id", async ({
  page,
}) => {
  await openMateTab(page);
  const mate = page.getByRole("region", { name: "Companion accounts" });

  // Add account → no alias prompt, the QR dialog appears straight away.
  await mate.getByRole("button", { name: "Add account", exact: true }).click();
  const qrDialog = page.getByRole("dialog");
  await expect(qrDialog).toBeVisible();
  await expect(qrDialog.getByAltText("Login QR code")).toBeVisible();

  // The add carries no label; the freshly minted id starts the QR flow.
  expect(await calls(page, "mate_account_add")).toEqual([
    { command: "mate_account_add", args: {} },
  ]);
  expect(await calls(page, "mate_login_start")).toEqual([
    { command: "mate_login_start", args: { accountId: "mate-1" } },
  ]);
});

test("mate login: closing the QR dialog issues mate_login_cancel and drops the new row", async ({
  page,
}) => {
  await openMateTab(page);
  const mate = page.getByRole("region", { name: "Companion accounts" });
  await mate.getByRole("button", { name: "Add account", exact: true }).click();

  const qrDialog = page.getByRole("dialog");
  await expect(qrDialog.getByAltText("Login QR code")).toBeVisible();
  await qrDialog.getByRole("button", { name: "Close", exact: true }).last().click();
  await expect(qrDialog).toHaveCount(0);
  expect(await commands(page)).toContain("mate_login_cancel");
  // Closing abandons the add: the placeholder row is removed too.
  expect(await calls(page, "mate_account_remove")).toEqual([
    { command: "mate_account_remove", args: { id: "mate-1" } },
  ]);
  await expect(mate.getByTestId("mate-list")).toHaveCount(0);
});

test("room monitor: start sends profileId + config and stop/state take no args", async ({ page }) => {
  await page.getByRole("tab", { name: "Live interact" }).click();
  const script = page.getByTestId("live-interact-script");
  const monitor = script.getByRole("region", { name: "Live-room monitor" });
  await monitor.getByPlaceholder("profile-id").fill("fixture-profile");
  await monitor.getByPlaceholder("https://live.kuaishou.com/…").fill("https://live.kuaishou.com/u/fixture");
  await monitor.getByPlaceholder("optional").first().fill("42");
  await monitor.getByRole("button", { name: "Start monitor" }).click();
  await expect(monitor.getByText("checking")).toBeVisible();

  await monitor.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(monitor.getByText("disabled")).toBeVisible();
  await monitor.getByRole("button", { name: "Refresh" }).click();

  expect(await calls(page, "start_live_room_monitor")).toEqual([
    {
      command: "start_live_room_monitor",
      args: {
        profileId: "fixture-profile",
        config: {
          liveRoomUrl: "https://live.kuaishou.com/u/fixture",
          sceneId: 42,
          groupId: null,
          productScriptId: null,
          productScriptAccountId: null,
          autoExitSubAccounts: false,
        },
      },
    },
  ]);
  expect(await calls(page, "stop_live_room_monitor")).toEqual([
    { command: "stop_live_room_monitor", args: {} },
  ]);
  expect((await calls(page, "get_live_room_monitor_state")).length).toBeGreaterThan(0);
});
