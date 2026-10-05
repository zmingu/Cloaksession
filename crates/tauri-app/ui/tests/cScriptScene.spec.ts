import { expect, test, type Page } from "@playwright/test";
import { cgroupRequests, installCGroupMock } from "./cScriptSceneMock";

// The mock already landed on the Live section; open the interact tab, whose
// 自动发言 sub-section hosts the auto-message / auto-reply / scene panels.
async function openScripts(page: Page): Promise<void> {
  await page.getByRole("tab", { name: /直播互动|Live interact/ }).click();
}

// 搬迁后三个面板同处「自动发言」子区（堆叠），不再有页签切换，
// 因此所有断言都按各自 region 作用域限定，避免同名控件（添加台词 / 播放 / 停止 / 确认）串扰。
const msgPanel = (page: Page) => page.getByRole("region", { name: /主播互动|Auto messages/ });
const replyPanel = (page: Page) => page.getByRole("region", { name: /自动回复|Auto replies/ });
const scenePanel = (page: Page) => page.getByRole("region", { name: /场景剧本|Scene scripts/ });

// Every navigation is loopback and every C-group command is a local Tauri fake.
test.beforeEach(async ({ page }) => {
  await installCGroupMock(page);
});

test("auto-message, auto-reply and scene panels render stacked in the same sub-section", async ({ page }) => {
  await openScripts(page);
  await expect(msgPanel(page)).toBeVisible();
  await expect(replyPanel(page)).toBeVisible();
  await expect(scenePanel(page)).toBeVisible();
  // 账号工作台改版后，主播互动面板不再手输账号 ID，改为展示账号栏选中的 profileId。
  await expect(msgPanel(page).getByText("Current account: fixture-profile")).toBeVisible();
  await expect(replyPanel(page).getByLabel(/要预览的评论|Comment to preview/)).toBeVisible();
  await expect(scenePanel(page).getByLabel(/场景名称|Scene name/)).toBeVisible();
});

test("auto-message start is confirmed and never sends random-space injection", async ({ page }) => {
  await openScripts(page);
  // 账号来自左侧账号栏（fixture 的唯一 profile）；台词归属该账号。
  await msgPanel(page).getByLabel(/时间偏移|Offset \(sec\)/).fill("5");
  await msgPanel(page).getByLabel(/弹幕模板|Message template/).fill("欢迎 {主播名称}");
  await msgPanel(page).getByRole("button", { name: /添加台词|Add line/ }).click();
  // Start requires a second confirmation step.
  await msgPanel(page).getByRole("button", { name: /^开始|Start$/ }).click();
  await expect(msgPanel(page).getByRole("group", { name: /确认开始|Confirm start/ })).toBeVisible();
  expect(await cgroupRequests(page)).toEqual([]);
  await msgPanel(page).getByRole("group", { name: /确认开始|Confirm start/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(msgPanel(page).getByRole("status")).toContainText(/已开始发送|Run started/);
  const starts = (await cgroupRequests(page)).filter((call) => call.command === "auto_message_start");
  expect(starts).toHaveLength(1);
  expect(starts[0].args.lines).toEqual([
    { offset_sec: 5, message: "欢迎 {主播名称}", account_id: "fixture-profile" },
  ]);
  // No random-space injection anywhere in the IPC args.
  const raw = JSON.stringify(starts[0].args).toLowerCase();
  expect(raw).not.toContain("randomspace");
  expect(raw).not.toContain("random_space");
  // Stop is confirmed too.
  await msgPanel(page).getByRole("button", { name: /^停止|Stop$/ }).click();
  await expect(msgPanel(page).getByRole("group", { name: /确认停止|Confirm stop/ })).toBeVisible();
  await msgPanel(page).getByRole("group", { name: /确认停止|Confirm stop/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(msgPanel(page).getByRole("status")).toContainText(/已停止发送|Run stopped/);
});

test("auto-reply preview and confirmed record write", async ({ page }) => {
  await openScripts(page);
  await replyPanel(page).getByLabel(/要预览的评论|Comment to preview/).fill("这件多少钱");
  await replyPanel(page).getByLabel(/商品标题|Goods title/).fill("测试商品");
  await replyPanel(page).getByLabel(/匹配词|Match tokens/).fill("多少钱,价格");
  await replyPanel(page).getByRole("button", { name: /预览|Preview/ }).click();
  await expect(replyPanel(page).getByLabel(/预览|Preview/).getByText(/命中/)).toBeVisible();
  // Record requires a second confirmation step.
  await replyPanel(page).getByRole("button", { name: /保存为记录|Save as record/ }).click();
  await expect(replyPanel(page).getByRole("group", { name: /确认保存|Confirm save/ })).toBeVisible();
  expect((await cgroupRequests(page)).filter((call) => call.command === "auto_reply_record")).toEqual([]);
  await replyPanel(page).getByRole("group", { name: /确认保存|Confirm save/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(replyPanel(page).getByRole("status")).toContainText(/已保存|saved/);
});

test("scene create, line add, play and stop are all confirmed", async ({ page }) => {
  await openScripts(page);
  await scenePanel(page).getByLabel(/场景名称|Scene name/).fill("开场剧本");
  await scenePanel(page).getByRole("button", { name: /创建|Create/ }).click();
  await expect(scenePanel(page).getByRole("group", { name: /确认创建|Confirm create/ })).toBeVisible();
  expect((await cgroupRequests(page)).filter((call) => call.command === "scene_create")).toEqual([]);
  await scenePanel(page).getByRole("group", { name: /确认创建|Confirm create/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(scenePanel(page).getByRole("button", { name: /开场剧本/ })).toBeVisible();
  await scenePanel(page).getByRole("button", { name: /开场剧本/ }).click();
  await scenePanel(page).getByLabel(/台词内容|Message/).fill("欢迎来到直播间");
  await scenePanel(page).getByLabel(/动作|Action/).selectOption("danmaku");
  await scenePanel(page).getByRole("button", { name: /添加台词|Add line/ }).click();
  await expect(scenePanel(page).getByText(/欢迎来到直播间/)).toBeVisible();
  // Play requires a second confirmation step.
  await scenePanel(page).getByRole("button", { name: /播放|Play/ }).click();
  await expect(scenePanel(page).getByRole("group", { name: /确认播放|Confirm play/ })).toBeVisible();
  expect((await cgroupRequests(page)).filter((call) => call.command === "scene_play")).toEqual([]);
  await scenePanel(page).getByRole("group", { name: /确认播放|Confirm play/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(scenePanel(page).getByRole("status")).toContainText(/播放中|playing/);
  // Stop requires a second confirmation step.
  await scenePanel(page).getByRole("button", { name: /停止|Stop/ }).click();
  await scenePanel(page).getByRole("group", { name: /确认停止|Confirm stop/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(scenePanel(page).getByRole("status")).toContainText(/已停止|stopped/);
  // Delete line requires a second confirmation step.
  await scenePanel(page).getByRole("button", { name: /删除第 1 条|Delete line 1/ }).click();
  await expect(scenePanel(page).getByRole("group", { name: /确认删除台词|Confirm delete line/ })).toBeVisible();
  await scenePanel(page).getByRole("group", { name: /确认删除台词|Confirm delete line/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(scenePanel(page).getByText(/欢迎来到直播间/)).toHaveCount(0);
});

test("scene_update tri-state: keep omits, clear nulls, set writes the group", async ({ page }) => {
  await openScripts(page);
  await scenePanel(page).getByLabel(/场景名称|Scene name/).fill("分组剧本");
  await scenePanel(page).getByRole("button", { name: /创建|Create/ }).click();
  await scenePanel(page).getByRole("group", { name: /确认创建|Confirm create/ }).getByRole("button", { name: /确认|Confirm/ }).click();
  await expect(scenePanel(page).getByRole("button", { name: /分组剧本/ })).toBeVisible();
  const updates = async () =>
    (await cgroupRequests(page)).filter((call) => call.command === "scene_update");
  // Create always issues an update carrying the trigger mode; default group mode is keep.
  const first = (await updates())[0];
  expect(first.args.triggerMode).toBe("relative-time");
  expect("groupId" in first.args).toBe(false);
});
