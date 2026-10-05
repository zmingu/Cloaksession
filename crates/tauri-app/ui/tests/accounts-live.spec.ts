import { expect, test, type Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";

/**
 * Live launch (正式开播) + 互动工作台契约套件。
 *
 * 业务板块归位后：
 *  - 慧播开播（HuiboLivePage，跟播 / 回播）+ 伴侣开播（LiveLaunchPage，本地视频循环推流）在「直播 › 正式开播」；
 *  - 开播准备只留脚本处理（LiveScriptPage，多视频多脚本占位）；
 *  - 伴侣账号管理（MateLoginPage）在「快手账号 › 直播伴侣」；
 *  - 互动工作台在「直播 › 直播互动」（左账号栏 + 右模块三组）。
 */
export const ACCOUNTS_LIVE_COMMANDS = [
  "mate_accounts_list",
  "mate_account_add",
  "mate_account_remove",
  "mate_login_start",
  "mate_login_cancel",
  "mate_login_state",
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

/** 切到「直播互动」账号工作台。 */
async function openInteract(page: Page): Promise<void> {
  await page.getByRole("tab", { name: "Live interact" }).click();
}

test("live pages (archived under Other) expose the four tabs; prepare hosts script, live hosts both launch pages", async ({ page }) => {
  await expect(page.getByRole("tab", { name: "Live prep" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Go live" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Live interact" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Ad delivery" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Live prep" })).toHaveAttribute("aria-selected", "true");
  // 开播准备只留脚本处理占位入口。
  await expect(page.getByTestId("live-prepare-script")).toBeVisible();
  await expect(page.getByTestId("live-prepare-huibo")).toHaveCount(0);
  await expect(page.getByTestId("live-prepare-mate")).toHaveCount(0);
  // 切到正式开播 = 慧播开播（跟播 / 回播）+ 伴侣开播（LiveLaunchPage 本地视频推流）。
  await page.getByRole("tab", { name: "Go live" }).click();
  const huibo = page.getByTestId("live-live-huibo");
  await expect(huibo.getByRole("heading", { name: "Huibo live", exact: true })).toBeVisible();
  await expect(huibo.getByRole("region", { name: "Huibo live (replay)" })).toBeVisible();
  const mate = page.getByTestId("live-live-mate");
  await expect(mate.getByRole("heading", { name: "Companion live", exact: true })).toBeVisible();
  await expect(mate.getByRole("region", { name: "Live launch" })).toBeVisible();
  // 伴侣账号管理页不属于正式开播。
  await expect(page.getByRole("region", { name: "Companion accounts" })).toHaveCount(0);
  // 正式开播已账号化：左账号栏（键 = profileId）+ 右开播控制。
  await expect(page.getByTestId("live-live-page")).toBeVisible();
  await expect(rail(page)).toBeVisible();
});

test("go-live is account-scoped: the rail drives the huibo account, mate stays independent", async ({
  page,
}) => {
  await page.getByRole("tab", { name: "Go live" }).click();
  const live = page.getByTestId("live-live-page");
  // 左账号栏列出每个浏览器环境；默认选中第一个。
  await expect(live.getByTestId("live-account-rail")).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE)).toBeVisible();
  await expect(accountRow(page, PROFILE_TWO)).toBeVisible();
  await expect(accountRow(page, PROFILE_ONE)).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("huibo-current-account")).toHaveText(PROFILE_ONE);

  // 切账号 → 右侧慧播开播消费新的 profileId。
  await accountRow(page, PROFILE_TWO).click();
  await expect(accountRow(page, PROFILE_TWO)).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("huibo-current-account")).toHaveText(PROFILE_TWO);

  // 伴侣开播是独立子区块：切账号后仍可达（mate 与 profile 是两套命名空间）。
  await expect(page.getByTestId("live-live-mate").getByRole("region", { name: "Live launch" })).toBeVisible();
  await expect(page.getByTestId("live-live-huibo").getByRole("region", { name: "Huibo live (replay)" })).toBeVisible();
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
  // The table keeps its shell; the dropped placeholder row is gone.
  await expect(mate.getByTestId("mate-list")).toBeVisible();
  await expect(mate.getByRole("row").filter({ hasText: MATE_PLACEHOLDER_LABEL })).toHaveCount(0);
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
