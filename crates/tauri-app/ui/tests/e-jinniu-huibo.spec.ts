import { expect, test, type Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";

/**
 * E-group (bind-creator / huibo-live / jinniu-promote) mock-render suite.
 *
 * 业务板块搬迁后：慧播开播在「直播 › 开播准备」，金牛推广与达人授权在
 * 「直播 › 投流」。
 *
 * All IPC is faked browser-locally: no profile launch, no platform request,
 * no real ad-account writes. Write-capable commands record their args and
 * return canned successes so the confirm-gated flows can be exercised.
 */

const JINNIU_ID = "jinniu-fixture";
const PROFILE = "fixture-profile";

interface Call {
  command: string;
  args: Record<string, any>;
}

async function installEBusinessMock(page: Page): Promise<void> {
  // NOTE: installBusinessMock is deliberately not reused — it ends with a
  // click back to the profiles section, which would leave the section
  // under test. The E-group pages never touch business_accounts_*,
  // so the base Tauri mock suffices.
  // Pre-seed before the first navigation: init scripts only fill absent keys,
  // and any later goto would wipe the in-page invoke wrapper installed below.
  await page.addInitScript(() => {
    localStorage.setItem("multizen.ui.section", JSON.stringify("live"));
    localStorage.setItem("multizen.ui.onboarded", "true");
  });
  await installTauriMock(page, { ...defaultSettings, language: "zh-CN" });
  await page.goto("/");
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const calls: Call[] = [];
    const videos = [
      {
        id: "replay-1", name: "回放样本一", uploadTime: "2026.10.01 20:00",
        segments: 3, size: "5.8GB", duration: "1小时41分", status: "success",
        usageCount: 0, expiryTime: "2026-11-01 20:00:00", goodsCount: 4,
      },
      {
        id: "replay-2", name: "回放样本二", uploadTime: "2026.10.02 20:00",
        segments: 1, size: "1.2GB", duration: "32分钟", status: "processing",
        usageCount: 1, expiryTime: "2026-11-02 20:00:00", goodsCount: 0,
      },
    ];
    const authorized = [
      { userId: "u-1001", userName: "达人样本", status: "已授权", authorizeTime: "2026-09-30 12:00:00" },
    ];
    const liveUsers = [
      { uid: "l-1", displayName: "推广用户甲", fullText: "推广用户甲 l-1", isSelected: true },
      { uid: "l-2", displayName: "推广用户乙", fullText: "推广用户乙 l-2", isSelected: false },
    ];
    Object.assign(window, { __TEST_EBUSINESS__: { calls, failNext: null as string | null } });
    internals.invoke = async (command: string, args: Record<string, any> = {}) => {
      if (
        command.startsWith("bind_creator_") ||
        command.startsWith("jinniu_promote_") ||
        command.startsWith("comment_listener_") ||
        command.startsWith("comment_events_") ||
        command.startsWith("sub_account") ||
        command === "save_sub_account" ||
        command === "list_sub_accounts" ||
        command === "unbind_sub_account" ||
        command === "batch_login_sub_accounts" ||
        ["get_huibo_video_list", "start_huibo_live", "get_shop_live_state", "cancel_huibo_task"].includes(command)
      ) {
        calls.push({ command, args: structuredClone(args) });
        const fail = (window as any).__TEST_EBUSINESS__.failNext;
        if (fail) {
          (window as any).__TEST_EBUSINESS__.failNext = null;
          throw fail; // Tauri rejects Result<_, String> with a string.
        }
        switch (command) {
          case "bind_creator_get_authorize_list": return { ok: true, data: structuredClone(authorized) };
          case "bind_creator_sync_authorize_list": return { ok: true, data: structuredClone(authorized) };
          case "bind_creator_start_authorize": return { ok: true };
          case "get_huibo_video_list": return structuredClone(videos);
          case "start_huibo_live": return { profileId: args.profileId, status: "live", liveRoomUrl: "https://live.fixture/room", updatedAt: 1 };
          case "get_shop_live_state": return { profileId: args.profileId, status: "live", liveRoomUrl: "https://live.fixture/room", updatedAt: 1 };
          case "cancel_huibo_task": return true;
          case "jinniu_promote_open_store_create": return { accountId: "acc-9", url: "https://niu.fixture/storeCreate?__accountId__=acc-9&homeType=new", targetId: "t-1" };
          case "jinniu_promote_live_users": return { accountId: "acc-9", users: structuredClone(liveUsers) };
          case "jinniu_promote_select_live_user": {
            const found = liveUsers.find((u) => u.uid === args.uid) ?? liveUsers[0];
            return { ...found, isSelected: true };
          }
          case "jinniu_promote_apply_phase1": return null;
          case "jinniu_promote_apply_phase2": return ["文案甲", "文案乙"];
          case "jinniu_promote_submit": return null;
          // Co-mounted B-group tabs: benign empties keep the shared shell quiet.
          case "comment_listener_status_all": return [];
          case "comment_listener_status": return null;
          case "comment_events_recent": return [];
          case "comment_events_history": return { rows: [], nextCursor: null };
          case "comment_events_next": return null;
          case "list_sub_accounts": return [];
          default: throw new Error(`Unexpected E-group fixture command: ${command}`);
        }
      }
      return original(command, args);
    };
  });
}

async function eCalls(page: Page): Promise<Call[]> {
  return page.evaluate(() => (window as any).__TEST_EBUSINESS__.calls);
}

/** The mock already landed on the Live section; open the ads tab (金牛推广 / 达人授权). */
async function openAds(page: Page): Promise<void> {
  await page.getByRole("tab", { name: "投流", exact: true }).click();
}

test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
  await installEBusinessMock(page);
});

async function confirmModal(page: Page, label: string): Promise<void> {
  const dialog = page.getByRole("dialog").last();
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: label, exact: true }).click();
  await expect(dialog).toHaveCount(0);
}

test("bind page lists local records and confirm-gates sync + authorize", async ({ page }) => {
  await openAds(page);
  const section = page.getByRole("region", { name: "达人授权" });
  await section.getByLabel("金牛 ID").fill(JINNIU_ID);
  await section.getByRole("button", { name: "加载记录" }).click();
  await expect(page.getByTestId("bind-row-u-1001")).toContainText("达人样本");
  expect(await eCalls(page)).toEqual([
    { command: "bind_creator_get_authorize_list", args: { jinniuId: JINNIU_ID } },
  ]);

  await section.getByRole("button", { name: "从后台同步" }).click();
  await confirmModal(page, "从后台同步");
  await expect(page.getByRole("status")).toContainText("已同步 1 条记录");

  await section.getByLabel("达人快手号").fill("ks-8899");
  await section.getByRole("button", { name: "发起授权" }).click();
  await confirmModal(page, "发起授权");
  await expect(page.getByRole("status")).toContainText("授权请求已提交");
  const calls = await eCalls(page);
  expect(calls[calls.length - 1]).toEqual({
    command: "bind_creator_start_authorize",
    args: { profileId: PROFILE, jinniuId: JINNIU_ID, kuaishouId: "ks-8899", skipConfirm: null, accountId: null },
  });
});

test("huibo page proves start success by re-reading live state", async ({ page }) => {
  const section = page.getByRole("region", { name: "跟播 / 回播" });
  await section.getByRole("button", { name: "刷新视频" }).click();
  await expect(page.getByTestId("huibo-video-replay-1")).toContainText("回放样本一");
  await section.getByRole("button", { name: "去开播" }).click();
  await confirmModal(page, "去开播");
  await expect(page.getByTestId("huibo-live-state")).toContainText("直播状态：live");
  const commands = (await eCalls(page)).map((c) => c.command);
  // start_huibo_live must be followed by a get_shop_live_state re-read.
  expect(commands).toEqual(["get_huibo_video_list", "start_huibo_live", "get_shop_live_state"]);

  await section.getByRole("button", { name: "取消任务" }).click();
  await confirmModal(page, "取消任务");
  await expect(page.getByRole("status")).toContainText("跟播任务已取消");
});

test("jinniu page runs the six-command promote flow with write confirms", async ({ page }) => {
  await openAds(page);
  const section = page.getByRole("region", { name: "金牛推广" });
  await section.getByRole("button", { name: "打开推广创编" }).click();
  await expect(page.getByTestId("jinniu-account")).toContainText("acc-9");
  await section.getByRole("button", { name: "加载可推广用户" }).click();
  await expect(page.getByTestId("jinniu-user-l-2")).toContainText("推广用户乙");

  await page.getByTestId("jinniu-user-l-2").getByRole("button", { name: "选择" }).click();
  await confirmModal(page, "选择");
  await expect(page.getByRole("status")).toContainText("已选择 推广用户乙");

  await section.getByLabel("日预算").fill("2000");
  await section.getByRole("button", { name: "应用第一阶段" }).click();
  await confirmModal(page, "应用第一阶段");
  await section.getByRole("button", { name: "应用第二阶段" }).click();
  await expect(page.getByTestId("jinniu-copies")).toContainText("文案甲");
  await section.getByRole("button", { name: "提交计划" }).click();
  await confirmModal(page, "提交计划");
  await expect(page.getByRole("status")).toContainText("推广计划已提交");

  const all = await eCalls(page);
  const commands = all.map((c) => c.command).filter((c) => c.startsWith("jinniu_promote_"));
  expect(commands).toEqual([
    "jinniu_promote_open_store_create",
    "jinniu_promote_live_users",
    "jinniu_promote_select_live_user",
    "jinniu_promote_apply_phase1",
    "jinniu_promote_apply_phase2",
    "jinniu_promote_submit",
  ]);
  const phase1 = all.find((c) => c.command === "jinniu_promote_apply_phase1")!;
  expect(phase1.args.config.dailyBudget).toBe("2000");
});
