import { expect, test, type Page } from "@playwright/test";
import {
  enterInteract,
  installListenSubMock,
  listenSubRequests,
  setListenSubFixture,
  subFixture,
} from "./listenSubMock";

// IPC is mocked: these tests never launch a profile or contact a platform.
//
// 业务板块搬迁后：弹幕监听 / 小号都在「直播 › 直播互动 › 互动账号脚本互动」，
// 随子区一起渲染（无独立页签）。面板在 interact 页签挂载时才加载数据，
// 因此每个用例先把 fixture 种好，再 `enterInteract` 让面板带数据挂载。
test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
  await installListenSubMock(page);
});

const commentsPage = (page: Page) => page.getByTestId("comments-page");
const subPage = (page: Page) => page.getByTestId("sub-page");
/**
 * 账号工作台改版后，监听账号不再手输：它来自「直播互动」左侧账号栏选中的
 * profileId（fixture 里唯一的浏览器环境）。
 */
const ACCOUNT = "fixture-profile";

test("listener start/stop drives status pill and streams snake_case events", async ({ page }) => {
  await setListenSubFixture(page, {
    events: [
      {
        id: "evt-1", type: "Comment", send_at: 1728000000.5, text: "好看",
        user_id: "u1", nickname: "nick", account_id: "p1", time: "2026-10-04T00:00:00Z",
      },
    ],
    historyRows: [
      {
        msg_id: "m-1", type: "Comment", user_id: "u1", nickname: "nick",
        content: "落库弹幕", time: "2026-10-04T00:00:00Z", account_id: "p1",
      },
    ],
  });
  await enterInteract(page);
  await expect(commentsPage(page)).toBeVisible();
  await expect(commentsPage(page).getByText("Stopped").first()).toBeVisible();
  // 账号来自左侧账号栏（无手输框）。
  await expect(commentsPage(page).getByPlaceholder("profile-id")).toHaveCount(0);
  await expect(commentsPage(page).getByText(`Current account: ${ACCOUNT}`)).toBeVisible();
  await commentsPage(page).getByRole("button", { name: "Start listening", exact: true }).click();
  const starts = (await listenSubRequests(page)).filter((r) => r.command === "comment_listener_start");
  expect(starts).toHaveLength(1);
  expect(starts[0].args).toEqual({ profileId: ACCOUNT, targetId: null });
  await expect(commentsPage(page).getByText("Running")).toBeVisible();
  await expect(commentsPage(page).getByTestId("comments-stream")).toContainText("nick: 好看");
  await commentsPage(page).getByRole("button", { name: "Load history", exact: true }).click();
  await expect(commentsPage(page).getByTestId("comments-history")).toContainText("落库弹幕");
  await commentsPage(page).getByRole("button", { name: "Stop", exact: true }).click();
  const stops = (await listenSubRequests(page)).filter((r) => r.command === "comment_listener_stop");
  expect(stops).toEqual([{ command: "comment_listener_stop", args: { profileId: ACCOUNT } }]);
  await expect(commentsPage(page).getByText("Stopped").first()).toBeVisible();
});

test("listener backend errors render the raw message under a Chinese template", async ({ page }) => {
  await enterInteract(page);
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    internals.invoke = async (command: string, args: any) => {
      if (command === "comment_listener_start") throw "环境未运行或会话未就绪，请先启动对应环境";
      return original(command, args);
    };
  });
  await commentsPage(page).getByRole("button", { name: "Start listening", exact: true }).click();
  await expect(commentsPage(page).getByRole("alert")).toContainText("环境未运行或会话未就绪");
});

test("sub-account list, batch login, and interaction history render from IPC", async ({ page }) => {
  await setListenSubFixture(page, {
    accounts: [subFixture()],
    interactions: [
      {
        id: 7, accountId: "sub-1", sceneId: null, action: "danmaku",
        message: "hello", liveRoomUrl: null, ok: true, error: null,
        durationMs: 120, createdAt: 1728000000000,
      },
    ],
  });
  await enterInteract(page);
  await expect(subPage(page)).toBeVisible();
  await expect(subPage(page).getByTestId("sub-list")).toContainText("小号甲");
  await subPage(page).getByRole("checkbox", { name: "Select 小号甲" }).check();
  await subPage(page).getByRole("button", { name: "Batch login (1)", exact: true }).click();
  await expect(subPage(page).getByTestId("sub-login-results")).toContainText("sub-1");
  const batch = (await listenSubRequests(page)).filter((r) => r.command === "batch_login_sub_accounts");
  expect(batch).toEqual([{ command: "batch_login_sub_accounts", args: { accountIds: ["sub-1"] } }]);
  await subPage(page).getByRole("button", { name: "History", exact: true }).click();
  await expect(subPage(page).getByTestId("sub-interactions")).toContainText("hello");
});

test("unbind and danmaku send both require confirmation then invoke", async ({ page }) => {
  await setListenSubFixture(page, { accounts: [subFixture()] });
  await enterInteract(page);
  await subPage(page).getByRole("button", { name: "Unbind", exact: true }).click();
  await expect(page.getByText("Unbind this sub-account?")).toBeVisible();
  await expect.poll(async () => (await listenSubRequests(page)).filter((r) => r.command === "unbind_sub_account")).toEqual([]);
  await page.getByRole("button", { name: "Confirm unbind" }).click();
  await expect.poll(async () => (await listenSubRequests(page)).filter((r) => r.command === "unbind_sub_account")).toEqual([
    { command: "unbind_sub_account", args: { id: "sub-1" } },
  ]);
  await expect(subPage(page).getByRole("status")).toContainText("Unbound");
});
