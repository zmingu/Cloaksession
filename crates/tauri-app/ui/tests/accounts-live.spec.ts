import { expect, test, type Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/** The 16 frontend-A commands (kuaishou_auth / mate_login / live_launch / live_room_monitor). */
export const ACCOUNTS_LIVE_COMMANDS = [
  "kuaishou_connect",
  "kuaishou_login",
  "ensure_kuaishou_auth",
  "mate_login_start",
  "mate_login_cancel",
  "mate_login_state",
  "live_launch_status",
  "live_launch_prerequisites",
  "live_launch_credentials",
  "live_launch_heartbeat_start",
  "live_launch_heartbeat_stop",
  "live_launch_stream_start",
  "live_launch_stream_stop",
  "start_live_room_monitor",
  "stop_live_room_monitor",
  "get_live_room_monitor_state",
] as const;

interface RecordedCall {
  command: string;
  args: Record<string, unknown>;
}

/**
 * Extends the shared Tauri mock with the A-group commands and boots straight
 * into the Business section. Installed via addInitScript, whose callback body
 * is serialized into the page — so it must not reference module scope.
 */
async function installAccountsLiveMock(page: Page): Promise<void> {
  await installTauriMock(page);
  await page.addInitScript((allowed: string[]) => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("business"));
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
    const streaming = (profileId: unknown) => ({
      profileId: String(profileId), status: "idle", mode: null, target: null, pid: null,
      stderrTail: [] as string[], exitCode: null, error: null,
      placeholderCredentials: false, startedAt: null,
    });
    const internals = (window as unknown as {
      __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> };
    }).__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    const recorded: RecordedCall[] = [];

    internals.invoke = async (command: string, args: Record<string, unknown> = {}) => {
      if (allowed.includes(command)) {
        recorded.push({ command, args: JSON.parse(JSON.stringify(args)) });
      }
      switch (command) {
        case "kuaishou_connect": return true;
        case "kuaishou_login": return;
        case "ensure_kuaishou_auth": return { ok: true, scanned: false };
        case "mate_login_start":
        case "mate_login_cancel":
        case "mate_login_state":
          return {
            accountId: String(args["accountId"]), stage: "awaiting-scan",
            qrImageDataUrl: "data:image/png;base64,fixture",
            qrLoginToken: null, qrLoginSignature: null, expireAt: null,
            errorMessage: null, user: null, startedAt: null, finishedAt: null,
          };
        case "live_launch_status":
          return streaming(args["profileId"]);
        case "live_launch_prerequisites":
          return { available: true, ffmpegPath: "/usr/bin/ffmpeg", searched: ["/usr/bin"], error: null };
        case "live_launch_credentials":
          return {
            rtmpServer: "rtmp://fixture.live/room", streamKey: "fixture-key",
            liveStreamId: "live-1", placeholder: false,
          };
        case "live_launch_heartbeat_start":
        case "live_launch_heartbeat_stop":
        case "live_launch_stream_start":
        case "live_launch_stream_stop":
          return streaming(args["profileId"]);
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
  }, ACCOUNTS_LIVE_COMMANDS as unknown as string[]);
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

test("business section exposes the four A-group tabs", async ({ page }) => {
  await expect(page.getByRole("tab", { name: "Shop auth" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Mate login" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Live launch" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Room monitor" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Shop auth" })).toHaveAttribute("aria-selected", "true");
});

test("shop auth: ensure-auth renders the reuse outcome", async ({ page }) => {
  await page.getByPlaceholder("profile-id").fill("fixture-profile");
  await page.getByPlaceholder("https://login.kwaixiaodian.com/").fill("https://login.kwaixiaodian.com/");
  await page.getByRole("button", { name: "Ensure auth" }).click();
  await expect(page.getByText("Authenticated via cookie reuse — no scan was needed.")).toBeVisible();
  expect(await calls(page, "ensure_kuaishou_auth")).toEqual([
    { command: "ensure_kuaishou_auth", args: { profileId: "fixture-profile", targetId: "https://login.kwaixiaodian.com/" } },
  ]);
});

test("shop auth: connect maps to camelCase args and the already-authed branch", async ({ page }) => {
  await page.getByPlaceholder("profile-id").fill("fixture-profile");
  await page.getByPlaceholder("https://login.kwaixiaodian.com/").fill("https://login.kwaixiaodian.com/");
  await page.getByRole("button", { name: "Check connection" }).click();
  await expect(page.getByText("Already authenticated — no scan needed.")).toBeVisible();
  expect(await calls(page, "kuaishou_connect")).toEqual([
    { command: "kuaishou_connect", args: { profileId: "fixture-profile", targetId: "https://login.kwaixiaodian.com/" } },
  ]);
});

test("mate login: start renders the QR and sends accountId", async ({ page }) => {
  await page.getByRole("tab", { name: "Mate login" }).click();
  await page.getByPlaceholder("account-id").fill("account-1");
  await page.getByRole("button", { name: "Start login" }).click();
  await expect(page.getByAltText("Login QR code")).toBeVisible();
  await expect(page.getByText("awaiting-scan")).toBeVisible();
  expect(await calls(page, "mate_login_start")).toEqual([
    { command: "mate_login_start", args: { accountId: "account-1" } },
  ]);
});

test("mate login: cancel issues mate_login_cancel", async ({ page }) => {
  await page.getByRole("tab", { name: "Mate login" }).click();
  await page.getByPlaceholder("account-id").fill("account-1");
  await page.getByRole("button", { name: "Cancel" }).click();
  expect(await commands(page)).toContain("mate_login_cancel");
});

test("live launch: prerequisites take no args and status takes profileId", async ({ page }) => {
  await page.getByRole("tab", { name: "Live launch" }).click();
  await page.getByRole("button", { name: "Check ffmpeg" }).click();
  await expect(page.getByText("ffmpeg ready: /usr/bin/ffmpeg")).toBeVisible();
  expect(await calls(page, "live_launch_prerequisites")).toEqual([
    { command: "live_launch_prerequisites", args: {} },
  ]);

  await page.getByPlaceholder("profile-id").fill("fixture-profile");
  await page.getByRole("button", { name: "Refresh status" }).click();
  await expect(page.getByText("idle", { exact: true })).toBeVisible();
  expect(await calls(page, "live_launch_status")).toEqual([
    { command: "live_launch_status", args: { profileId: "fixture-profile" } },
  ]);
});

test("live launch: heartbeat start/stop and stream start/stop arg shapes", async ({ page }) => {
  await page.getByRole("tab", { name: "Live launch" }).click();
  await page.getByPlaceholder("profile-id").fill("fixture-profile");
  await page.getByRole("button", { name: "Heartbeat start" }).click();
  await page.getByRole("button", { name: "Heartbeat stop" }).click();
  await expect(page.getByRole("button", { name: "Heartbeat stop" })).toBeEnabled();

  await page.getByPlaceholder("D:\\videos\\loop.mp4").fill("D:\\videos\\loop.mp4");
  await page.getByRole("button", { name: "Stream start" }).click();
  await page.getByRole("button", { name: "Confirm" }).click();
  await page.getByRole("button", { name: "Stream stop" }).click();
  await page.getByRole("button", { name: "Confirm" }).click();

  expect(await calls(page, "live_launch_heartbeat_start")).toEqual([
    { command: "live_launch_heartbeat_start", args: { profileId: "fixture-profile", controlUrl: null } },
  ]);
  expect(await calls(page, "live_launch_heartbeat_stop")).toEqual([
    { command: "live_launch_heartbeat_stop", args: { profileId: "fixture-profile" } },
  ]);
  expect(await calls(page, "live_launch_stream_start")).toEqual([
    { command: "live_launch_stream_start", args: { profileId: "fixture-profile", videoPath: "D:\\videos\\loop.mp4", controlUrl: null } },
  ]);
  expect(await calls(page, "live_launch_stream_stop")).toEqual([
    { command: "live_launch_stream_stop", args: { profileId: "fixture-profile" } },
  ]);
});

test("live launch: stream start is gated behind a confirm dialog", async ({ page }) => {
  await page.getByRole("tab", { name: "Live launch" }).click();
  await page.getByPlaceholder("profile-id").fill("fixture-profile");
  await page.getByPlaceholder("D:\\videos\\loop.mp4").fill("D:\\videos\\loop.mp4");
  await page.getByRole("button", { name: "Stream start" }).click();
  await expect(page.getByText("Start pushing the local video to the live room?")).toBeVisible();
  expect(await commands(page)).not.toContain("live_launch_stream_start");
});

test("room monitor: start sends profileId + config and stop/state take no args", async ({ page }) => {
  await page.getByRole("tab", { name: "Room monitor" }).click();
  await page.getByPlaceholder("profile-id").fill("fixture-profile");
  await page.getByPlaceholder("https://live.kuaishou.com/…").fill("https://live.kuaishou.com/u/fixture");
  await page.getByPlaceholder("optional").first().fill("42");
  await page.getByRole("button", { name: "Start monitor" }).click();
  await expect(page.getByText("checking")).toBeVisible();

  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(page.getByText("disabled")).toBeVisible();
  await page.getByRole("button", { name: "Refresh" }).click();

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
