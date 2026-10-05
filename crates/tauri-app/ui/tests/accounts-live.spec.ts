import { expect, test, type Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/**
 * Live launch (伴侣开播) + live-room-monitor (直播间监控, 多槽) 契约套件。
 *
 * 业务板块归位后：
 *  - 伴侣开播（LiveLaunchPage，本地视频循环推流）在「直播 › 开播准备」；
 *  - 伴侣账号管理（MateLoginPage）在「快手账号 › 直播伴侣」；
 *  - 直播间监控在「直播 › 直播互动」账号工作台（左账号栏 + 右模块）。
 *
 * 多槽改版（Rust driver/live_room_monitor.rs）后：
 *  - `start_live_room_monitor(profileId, config)` 按 profile 分槽，可多账号并存；
 *  - `stop_live_room_monitor(profileId)` / `get_live_room_monitor_state(profileId)`
 *    **必须带 profileId**（旧版无参）；
 *  - `list_live_room_monitor_states()` 无参，一次拉全所有槽。
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
  "list_live_room_monitor_states",
] as const;

/** Alias the backend stamps on a row added without a label (mirrors Rust). */
export const MATE_PLACEHOLDER_LABEL = "未命名伴侣";

/** 多账号 fixture：账号工作台的键 = profileId。 */
export const PROFILE_ONE = "profile-1";
export const PROFILE_TWO = "profile-2";

interface RecordedCall {
  command: string;
  args: Record<string, unknown>;
}

/** 两个浏览器环境（= 两个互动账号），供账号栏/多槽监控使用。 */
function profileFixtures(): Array<Record<string, unknown>> {
  return [
    { id: PROFILE_ONE, name: "Alpha env", tags: [], isRunning: false, timezone: "Asia/Shanghai" },
    { id: PROFILE_TWO, name: "Beta env", tags: [], isRunning: false, timezone: "Asia/Shanghai" },
  ];
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
    ({ allowed, placeholder, profiles }: {
      allowed: string[];
      placeholder: string;
      profiles: Array<Record<string, unknown>>;
    }) => {
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
      // 多槽监控：按 profileId 存快照，模拟 Rust 的 slots map。
      const monitorSlots: Record<string, Record<string, unknown>> = {};

      internals.invoke = async (command: string, args: Record<string, unknown> = {}) => {
        if (allowed.includes(command)) {
          recorded.push({ command, args: JSON.parse(JSON.stringify(args)) });
        }
        switch (command) {
          case "profiles_list":
            return JSON.parse(JSON.stringify(profiles));
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
          case "start_live_room_monitor": {
            const profileId = String(args["profileId"]);
            const slot = {
              ...idleMonitor(), enabled: true,
              profileId,
              liveRoomUrl: (args["config"] as { liveRoomUrl: string }).liveRoomUrl,
              status: "checking",
            };
            monitorSlots[profileId] = slot;
            return JSON.parse(JSON.stringify(slot));
          }
          case "stop_live_room_monitor": {
            const profileId = String(args["profileId"]);
            const slot = monitorSlots[profileId];
            delete monitorSlots[profileId];
            // 幂等：未知 profile → 默认快照（与 Rust 一致）。
            return JSON.parse(JSON.stringify(slot ?? idleMonitor()));
          }
          case "get_live_room_monitor_state": {
            const profileId = String(args["profileId"]);
            return JSON.parse(JSON.stringify(monitorSlots[profileId] ?? idleMonitor()));
          }
          case "list_live_room_monitor_states":
            return JSON.parse(JSON.stringify(
              Object.values(monitorSlots).sort((a, b) =>
                String(a["profileId"]).localeCompare(String(b["profileId"])),
              ),
            ));
          default:
            return originalInvoke(command, args);
        }
      };
      (window as unknown as { __TEST_A_CALLS__: RecordedCall[] }).__TEST_A_CALLS__ = recorded;
    },
    {
      allowed: ACCOUNTS_LIVE_COMMANDS as unknown as string[],
      placeholder: MATE_PLACEHOLDER_LABEL,
      profiles: profileFixtures(),
    },
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

/** 直接调用 mock 的 `list_live_room_monitor_states`，用于校验多槽并存。 */
async function listMonitorSlots(page: Page): Promise<Array<Record<string, unknown>>> {
  return page.evaluate(async () => {
    const internals = (window as unknown as {
      __TAURI_INTERNALS__: { invoke: (c: string, a?: Record<string, unknown>) => Promise<unknown> };
    }).__TAURI_INTERNALS__;
    return (await internals.invoke("list_live_room_monitor_states")) as Array<Record<string, unknown>>;
  });
}

test.beforeEach(async ({ page }) => {
  await installAccountsLiveMock(page);
});

/** The mate QR login page lives under 快手账号 › 直播伴侣 (Companion). */
async function openMateTab(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Kuaishou account", exact: true }).click();
  await page.getByRole("button", { name: "Companion", exact: true }).click();
}

const rail = (page: Page) => page.getByTestId("live-account-rail");
const accountRow = (page: Page, id: string) => page.getByTestId(`live-account-${id}`);
const monitor = (page: Page) =>
  page.getByTestId("live-interact-script").getByRole("region", { name: "Live-room monitor" });

/** 切到「直播互动」账号工作台。 */
async function openInteract(page: Page): Promise<void> {
  await page.getByRole("tab", { name: "Live interact" }).click();
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
  // 开播准备已账号化：左账号栏（键 = profileId）+ 右开播控制。
  await expect(page.getByTestId("live-prepare-page")).toBeVisible();
  await expect(rail(page)).toBeVisible();
});

test("live prep is account-scoped: the rail drives the huibo account, mate stays independent", async ({
  page,
}) => {
  const prepare = page.getByTestId("live-prepare-page");
  // 左账号栏列出每个浏览器环境；默认选中第一个。
  await expect(prepare.getByTestId("live-account-rail")).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE)).toBeVisible();
  await expect(accountRow(page, PROFILE_TWO)).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE)).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("huibo-current-account")).toHaveText(PROFILE_ONE);

  // 切账号 → 右侧慧播开播消费新的 profileId。
  await accountRow(page, PROFILE_TWO).click();
  await expect(accountRow(page, PROFILE_TWO)).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("huibo-current-account")).toHaveText(PROFILE_TWO);

  // 伴侣开播是独立子区块：切账号后仍可达（mate 与 profile 是两套命名空间）。
  await expect(page.getByTestId("live-prepare-mate").getByRole("region", { name: "Live launch" })).toBeVisible();
  await expect(page.getByTestId("live-prepare-huibo").getByRole("region", { name: "Huibo live (replay)" })).toBeVisible();
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

test("interact workspace: the rail lists every account and switching re-targets the modules", async ({
  page,
}) => {
  await openInteract(page);
  const page0 = page.getByTestId("live-interact-page");
  await expect(page0).toBeVisible();

  // 左账号栏渲染多个账号（键 = profileId），默认选中第一个。
  await expect(page0.getByTestId("live-account-rail")).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE)).toBeVisible();
  await expect(accountRow(page, PROFILE_TWO)).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE)).toHaveAttribute("aria-current", "true");

  // 右侧三组模块都在，且消费选中账号。
  const script = page.getByTestId("live-interact-script");
  const speak = page.getByTestId("live-interact-speak");
  await expect(script.getByRole("heading", { name: "Interact accounts · scripts", exact: true })).toBeVisible();
  await expect(speak.getByRole("heading", { name: "Auto speak", exact: true })).toBeVisible();
  // speak 组内多个面板都展示「当前账号」，取首个断言即可。
  await expect(speak.getByText("Current account: profile-1").first()).toBeVisible();

  // 切到第二个账号 → 右侧重挂载，模块改用新 profileId（IPC args 随之变化）。
  await accountRow(page, PROFILE_TWO).click();
  await expect(accountRow(page, PROFILE_TWO)).toHaveAttribute("aria-current", "true");
  await expect(speak.getByText("Current account: profile-2").first()).toBeVisible();
  await expect(speak.getByText("Current account: profile-1")).toHaveCount(0);

  // 监控快照按新账号拉取。
  await expect
    .poll(async () => (await calls(page, "get_live_room_monitor_state")).at(-1)?.args)
    .toEqual({ profileId: PROFILE_TWO });
});

test("interact workspace: a streaming profile gets the Live badge on its rail row", async ({
  page,
}) => {
  await openInteract(page);
  await expect(accountRow(page, PROFILE_TWO)).toBeVisible();
  // 尚无在播标记。
  await expect(accountRow(page, PROFILE_TWO).getByText("Live", { exact: true })).toHaveCount(0);

  // emit `live-launch-state-changed`（subjectKind:"profile", status:"streaming"）。
  await page.evaluate((profileId) => {
    (window as unknown as { __TEST_IPC__: { emit: (e: string, p: unknown) => void } }).__TEST_IPC__.emit(
      "live-launch-state-changed",
      {
        profileId, subjectKind: "profile", status: "streaming",
        mode: null, target: null, pid: 4242, exitCode: null, error: null,
        placeholderCredentials: false, liveStreamId: null,
      },
    );
  }, PROFILE_TWO);

  // 该账号行出现「Live」徽章，其它账号不受影响。
  await expect(accountRow(page, PROFILE_TWO).getByText("Live", { exact: true })).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE).getByText("Live", { exact: true })).toHaveCount(0);

  // 下播（stopped）→ 徽章消失。
  await page.evaluate((profileId) => {
    (window as unknown as { __TEST_IPC__: { emit: (e: string, p: unknown) => void } }).__TEST_IPC__.emit(
      "live-launch-state-changed",
      {
        profileId, subjectKind: "profile", status: "stopped",
        mode: null, target: null, pid: null, exitCode: 0, error: null,
        placeholderCredentials: false, liveStreamId: null,
      },
    );
  }, PROFILE_TWO);
  await expect(accountRow(page, PROFILE_TWO).getByText("Live", { exact: true })).toHaveCount(0);
});

test("room monitor: start sends the selected profileId + config; stop/state carry it too", async ({
  page,
}) => {
  await openInteract(page);
  const panel = monitor(page);
  // 账号来自左侧账号栏（默认第一个），不再有 profile-id 输入框。
  await expect(panel.getByPlaceholder("profile-id")).toHaveCount(0);
  await expect(panel.getByText("Current account: profile-1")).toBeVisible();

  await panel.getByPlaceholder("https://live.kuaishou.com/…").fill("https://live.kuaishou.com/u/fixture");
  await panel.getByPlaceholder("optional").first().fill("42");
  await panel.getByRole("button", { name: "Start monitor" }).click();
  await expect(panel.getByText("checking")).toBeVisible();

  await panel.getByRole("button", { name: "Stop", exact: true }).click();
  await panel.getByRole("button", { name: "Refresh" }).click();
  await expect(panel.getByText("disabled")).toBeVisible();

  expect(await calls(page, "start_live_room_monitor")).toEqual([
    {
      command: "start_live_room_monitor",
      args: {
        profileId: PROFILE_ONE,
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
  // 新契约：stop / state 都带 profileId。
  expect(await calls(page, "stop_live_room_monitor")).toEqual([
    { command: "stop_live_room_monitor", args: { profileId: PROFILE_ONE } },
  ]);
  const stateCalls = await calls(page, "get_live_room_monitor_state");
  expect(stateCalls.length).toBeGreaterThan(0);
  expect(stateCalls.every((c) => c.args["profileId"] === PROFILE_ONE)).toBe(true);
});

test("room monitor: two accounts monitor independently and stop only targets the selected one", async ({
  page,
}) => {
  await openInteract(page);

  // 账号 1：启动监控。
  await monitor(page).getByPlaceholder("https://live.kuaishou.com/…").fill("https://live.kuaishou.com/u/one");
  await monitor(page).getByRole("button", { name: "Start monitor" }).click();
  await expect(monitor(page).getByText("checking")).toBeVisible();

  // 切到账号 2：右侧重挂载，启动第二个监控槽。
  await accountRow(page, PROFILE_TWO).click();
  await expect(accountRow(page, PROFILE_TWO)).toHaveAttribute("aria-current", "true");
  await monitor(page).getByPlaceholder("https://live.kuaishou.com/…").fill("https://live.kuaishou.com/u/two");
  await monitor(page).getByRole("button", { name: "Start monitor" }).click();
  await expect(monitor(page).getByText("checking")).toBeVisible();

  // start 被调用两次，profileId 不同。
  const starts = await calls(page, "start_live_room_monitor");
  expect(starts.map((c) => c.args["profileId"]).sort()).toEqual([PROFILE_ONE, PROFILE_TWO]);
  expect(starts.map((c) => (c.args["config"] as { liveRoomUrl: string }).liveRoomUrl).sort()).toEqual([
    "https://live.kuaishou.com/u/one",
    "https://live.kuaishou.com/u/two",
  ]);

  // 多槽并存：list 无参返回两个槽。
  const slots = await listMonitorSlots(page);
  expect(slots.map((s) => s["profileId"]).sort()).toEqual([PROFILE_ONE, PROFILE_TWO]);

  // 切回账号 1 并停止 → 只停选中账号，账号 2 仍在跑。
  await accountRow(page, PROFILE_ONE).click();
  await expect(accountRow(page, PROFILE_ONE)).toHaveAttribute("aria-current", "true");
  await monitor(page).getByRole("button", { name: "Stop", exact: true }).click();
  expect(await calls(page, "stop_live_room_monitor")).toEqual([
    { command: "stop_live_room_monitor", args: { profileId: PROFILE_ONE } },
  ]);
  const remaining = await listMonitorSlots(page);
  expect(remaining.map((s) => s["profileId"])).toEqual([PROFILE_TWO]);
});
