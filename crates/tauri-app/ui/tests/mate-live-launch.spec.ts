import { expect, test, type Page } from "@playwright/test";
import {
  emitLiveLaunchState,
  installMateLiveMock,
  mateAccountFixture,
  mateLiveRequests,
} from "./mateLiveLaunchMock";

/**
 * 伴侣开播（LiveLaunchPage）契约套件 —— 对齐 jieger `pages/live-launch`。
 *
 * 覆盖：选伴侣账号下拉渲染、未登录禁用、取流、推流二次确认、状态推进。
 * 全部 IPC 浏览器本地伪造：不启浏览器、不取真实推流码、不推真实流。
 */

const LOGGED_IN = mateAccountFixture({ id: "mate-1", label: "伴侣甲", loginAt: 1750000000000 });
const LOGGED_OUT = mateAccountFixture({ id: "mate-2", label: "伴侣乙", loginAt: null });

async function mateBlock(page: Page) {
  return page.getByTestId("live-prepare-mate").getByRole("region", { name: "Live launch" });
}

test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
});

test("account dropdown lists companion accounts with login state", async ({ page }) => {
  await installMateLiveMock(page, [LOGGED_IN, LOGGED_OUT]);
  const block = await mateBlock(page);

  const select = block.getByTestId("live-mate-account");
  await expect(select).toBeVisible();
  await expect(select.locator("option")).toHaveCount(2);
  await expect(select).toHaveValue("mate-1");
  // The first (logged-in) account drives the initial state.
  await expect(block.getByTestId("live-mate-login-state")).toContainText("Logged in");
  // No manual profile-id input remains in the companion block.
  await expect(block.getByPlaceholder("profile-id")).toHaveCount(0);

  await select.selectOption("mate-2");
  await expect(block.getByTestId("live-mate-login-state")).toContainText("Not logged in");
  await expect(block.getByText("This account is not logged in")).toBeVisible();
});

test("an empty account list prompts to log in and disables fetching", async ({ page }) => {
  await installMateLiveMock(page, []);
  const block = await mateBlock(page);

  await expect(block.getByRole("alert")).toContainText("No Companion account yet");
  await expect(block.getByRole("button", { name: "Fetch stream key" })).toBeDisabled();
  await expect(block.getByRole("button", { name: "Stream start" })).toBeDisabled();
});

test("logged-out account disables fetch and stream start", async ({ page }) => {
  await installMateLiveMock(page, [LOGGED_OUT]);
  const block = await mateBlock(page);

  await expect(block.getByTestId("live-mate-login-state")).toContainText("Not logged in");
  await expect(block.getByRole("button", { name: "Fetch stream key" })).toBeDisabled();
  await expect(block.getByRole("button", { name: "Stream start" })).toBeDisabled();
  await expect(block.getByRole("button", { name: "Stream stop" })).toBeDisabled();
});

test("fetch credentials then start asks for confirmation and streams", async ({ page }) => {
  await installMateLiveMock(page, [LOGGED_IN]);
  const block = await mateBlock(page);

  // Fetch stream key for the selected account.
  await block.getByRole("button", { name: "Fetch stream key" }).click();
  await expect(block.getByRole("status")).toContainText("Credentials ready");
  expect(await mateLiveRequests(page, "live_launch_mate_credentials")).toEqual([
    { command: "live_launch_mate_credentials", args: { mateAccountId: "mate-1" } },
  ]);

  // Start is confirm-gated: cancelling must not issue the command.
  await block.getByPlaceholder("D:\\videos\\loop.mp4").fill("D:\\videos\\loop.mp4");
  await block.getByRole("button", { name: "Stream start" }).click();
  const confirmDialog = page.getByRole("dialog");
  await expect(confirmDialog).toContainText("Start pushing the local video");
  await confirmDialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await mateLiveRequests(page, "live_launch_mate_stream_start")).toEqual([]);

  // Confirming issues the command with (accountId, videoPath).
  await block.getByRole("button", { name: "Stream start" }).click();
  await expect(page.getByRole("dialog")).toContainText("Start pushing the local video");
  await page.getByRole("dialog").getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await mateLiveRequests(page, "live_launch_mate_stream_start")).toEqual([
    {
      command: "live_launch_mate_stream_start",
      args: { mateAccountId: "mate-1", videoPath: "D:\\videos\\loop.mp4" },
    },
  ]);
  await expect(block.getByText("streaming")).toBeVisible();
});

test("stop asks for confirmation and advances the status to stopped", async ({ page }) => {
  await installMateLiveMock(page, [LOGGED_IN]);
  const block = await mateBlock(page);
  // Wait for the account to be selected so the push is attributed to it.
  await expect(block.getByTestId("live-mate-account")).toHaveValue("mate-1");

  // Drive the block into a streaming state via the backend push channel.
  await emitLiveLaunchState(page, {
    profileId: "mate-1",
    subjectKind: "mate",
    status: "streaming",
    mode: null,
    target: "rtmp://push.fixture/live/***",
    pid: 4242,
    exitCode: null,
    error: null,
    placeholderCredentials: false,
    liveStreamId: "LS-1",
  });
  await expect(block.getByText("streaming")).toBeVisible();

  // A push for the other bucket must be ignored.
  await emitLiveLaunchState(page, {
    profileId: "mate-1",
    subjectKind: "profile",
    status: "stopped",
    mode: null,
    target: null,
    pid: null,
    exitCode: null,
    error: null,
    placeholderCredentials: false,
    liveStreamId: null,
  });
  await expect(block.getByText("streaming")).toBeVisible();

  await block.getByRole("button", { name: "Stream stop" }).click();
  await expect(page.getByRole("dialog")).toContainText("Stop the active push");
  await page.getByRole("dialog").getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(block.getByText("stopped")).toBeVisible();
  expect(await mateLiveRequests(page, "live_launch_mate_stream_stop")).toEqual([
    { command: "live_launch_mate_stream_stop", args: { mateAccountId: "mate-1" } },
  ]);
});

test("the FFmpeg prerequisite check reports readiness", async ({ page }) => {
  await installMateLiveMock(page, [LOGGED_IN]);
  const block = await mateBlock(page);

  await block.getByRole("button", { name: "Check ffmpeg" }).click();
  await expect(block.getByRole("status")).toContainText("ffmpeg ready");
});
