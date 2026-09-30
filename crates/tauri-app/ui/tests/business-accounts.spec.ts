import { expect, test, type Page } from "@playwright/test";
import {
  accountFixture, businessRequests, FIRST_PROFILE, installBusinessMock,
  savedAccounts, SECOND_PROFILE, setBusinessFixture,
} from "./businessAccountsMock";
import { profilePatches } from "./tauriMock";

const region = (page: Page) => page.getByRole("region", { name: "业务账号登记", exact: true });
const alias = (page: Page) => region(page).getByLabel("账号别名（必填）", { exact: true });
const platformId = (page: Page) => region(page).getByLabel("平台ID（可选，手填不代表已登录）", { exact: true });
const kind = (page: Page) => region(page).getByRole("combobox", { name: /^账号类型/ });
const create = (page: Page) => region(page).getByRole("button", { name: "创建并绑定登记", exact: true });
const mode = (page: Page) => region(page).getByRole("combobox", { name: "登记方式", exact: true });
const existing = (page: Page) => region(page).getByRole("combobox", { name: /^已有未关联记录/ });
const saves = async (page: Page) => (await businessRequests(page)).filter((call) => call.command === "business_accounts_save");
const unbinds = async (page: Page) => (await businessRequests(page)).filter((call) => call.command === "business_accounts_unbind");

async function enterAccounts(page: Page): Promise<void> {
  await page.getByRole("dialog").getByRole("button", { name: "General", exact: true }).click();
  await expect(region(page)).toBeVisible();
}
async function openAccounts(page: Page, profileName = "Regression profile"): Promise<void> {
  await page.getByRole("button", { name: new RegExp(profileName) }).click();
  await enterAccounts(page);
}
async function closeEdit(page: Page): Promise<void> {
  await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
}
async function holdNext(page: Page, command: string): Promise<void> {
  await page.evaluate((command) => { (window as any).__TEST_BUSINESS__.holdNext = command; }, command);
}
async function waitHeld(page: Page): Promise<void> {
  await expect.poll(() => page.evaluate(() => (window as any).__TEST_BUSINESS__.pending.length)).toBe(1);
}
async function release(page: Page): Promise<void> {
  await page.evaluate(() => (window as any).__TEST_BUSINESS__.release());
}
async function setRunning(page: Page, ids: string[]): Promise<void> {
  await page.evaluate((ids) => { (window as any).__TEST_BUSINESS__.running = ids; }, ids);
}

// Every navigation is loopback and every business command is a local Tauri fake.
test.beforeEach(async ({ page }) => {
  await installBusinessMock(page);
});

test("creation is edit-only, explicit, and independent of unsaved profile fields; edits keep kind immutable", async ({ page }, testInfo) => {
  await page.getByRole("button", { name: /^New profile/ }).click();
  await expect(page.getByRole("dialog").getByRole("button", { name: "业务账号", exact: true })).toHaveCount(0);
  await closeEdit(page);
  expect(await businessRequests(page)).toEqual([]);

  await page.getByRole("button", { name: /Regression profile/ }).click();
  const dialog = page.getByRole("dialog");
  // An invalid profile draft must remain local even when account registration succeeds.
  await dialog.locator("input[data-autofocus]").fill("");
  await dialog.locator("textarea").fill("未保存的环境备注");
  await enterAccounts(page);
  await expect(create(page)).toBeDisabled();
  await expect(region(page)).toContainText("不代表已登录或免登录，不共享 Cookie");
  await alias(page).fill("   ");
  await expect(create(page)).toBeDisabled();
  await alias(page).fill("  工作小店  ");
  await kind(page).selectOption("kuaishou-shop");
  await page.waitForTimeout(750); // Past the profile autosave debounce: still no business write.
  expect(await saves(page)).toEqual([]);
  expect(await profilePatches(page)).toEqual([]);
  await create(page).click();
  await expect(region(page).getByRole("status")).toHaveText("账号登记已保存；这不代表已登录平台。");
  expect((await saves(page))[0].args).toEqual({ input: {
    profileId: FIRST_PROFILE, kind: "kuaishou-shop", displayName: "工作小店", platformUserId: null,
  } });
  await expect(kind(page)).toBeDisabled();
  await expect(alias(page)).toHaveValue("工作小店");
  await dialog.getByRole("button", { name: "General", exact: true }).click();
  await expect(dialog.locator("input[data-autofocus]")).toHaveValue("");
  await expect(dialog.locator("textarea")).toHaveValue("未保存的环境备注");
  expect(await profilePatches(page)).toEqual([]);
  await enterAccounts(page);
  await alias(page).fill("工作小店改名");
  await platformId(page).fill("  user-789  ");
  await region(page).getByRole("button", { name: "保存登记修改", exact: true }).click();
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  expect((await saves(page))[1].args.input).toEqual({
    id: "account-1", profileId: FIRST_PROFILE, kind: "kuaishou-shop", displayName: "工作小店改名", platformUserId: "user-789",
  });
  expect(await profilePatches(page)).toEqual([]);
  const bounds = await region(page).evaluate((element) => ({ client: element.clientWidth, scroll: element.scrollWidth }));
  expect(bounds.scroll).toBeLessThanOrEqual(bounds.client + 1);
  await alias(page).scrollIntoViewIfNeeded();
  await page.screenshot({ path: testInfo.outputPath("business-account-edit.png") });
});

test("unbind requires confirmation, retains the record and Jinniu scope, and allows rebinding the same record", async ({ page }) => {
  const original = accountFixture({ kind: "jinniu", displayName: "独立金牛" });
  await setBusinessFixture(page, [original], { [FIRST_PROFILE]: "jinniu" });
  await openAccounts(page);
  await expect(kind(page)).toHaveValue("jinniu");
  await region(page).getByRole("button", { name: "解绑登记", exact: true }).click();
  const confirmation = region(page).getByRole("group", { name: "确认解绑登记", exact: true });
  await expect(confirmation).toContainText("保留登记记录，不清除 Cookie、不退出平台登录");
  await expect(confirmation).toContainText("金牛标记");
  expect(await unbinds(page)).toEqual([]);
  await confirmation.getByRole("button", { name: "取消", exact: true }).click();
  await expect(confirmation).toHaveCount(0);
  expect(await unbinds(page)).toEqual([]);
  await region(page).getByRole("button", { name: "解绑登记", exact: true }).click();
  await holdNext(page, "business_accounts_unbind");
  await confirmation.getByRole("button", { name: "确认解绑", exact: true }).click();
  await waitHeld(page);
  await expect(confirmation.getByRole("button", { name: "确认解绑", exact: true })).toBeDisabled();
  await release(page);
  await expect(region(page).getByRole("status")).toContainText("登记记录已保留，未清除 Cookie");
  expect((await unbinds(page))[0].args).toEqual({ id: original.id });
  expect(await savedAccounts(page)).toEqual([{ ...original, profileId: null }]);
  await expect(region(page)).toContainText("解绑后仍保留金牛专用标记");
  await expect(kind(page).locator("option")).toHaveCount(1);
  await mode(page).selectOption("existing");
  await existing(page).selectOption(original.id);
  await region(page).getByRole("button", { name: "绑定已有登记", exact: true }).click();
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  expect(await savedAccounts(page)).toHaveLength(1);
  expect((await savedAccounts(page))[0].profileId).toBe(FIRST_PROFILE);
  expect((await saves(page))[0].args.input.id).toBe(original.id);
});

test("orphan rebinding never offers other profiles' accounts and disables Jinniu in a Kuaishou scope", async ({ page }) => {
  const orphan = accountFixture({ id: "orphan", profileId: null, kind: "kuaishou-live", displayName: "删环境后保留" });
  const elsewhere = accountFixture({ id: "elsewhere", profileId: SECOND_PROFILE, displayName: "其他环境已占用" });
  const gold = accountFixture({ id: "gold", kind: "jinniu", profileId: null, displayName: "金牛旧记录" });
  await setBusinessFixture(page, [orphan, elsewhere, gold], { [FIRST_PROFILE]: "kuaishou" });
  await openAccounts(page);
  await expect(kind(page).locator('option[value="jinniu"]')).toHaveJSProperty("disabled", true);
  await expect(region(page)).toContainText("不能登记磁力金牛");
  await mode(page).selectOption("existing");
  await expect(existing(page).locator('option[value="elsewhere"]')).toHaveCount(0);
  await expect(existing(page).locator('option[value="gold"]')).toHaveJSProperty("disabled", true);
  await existing(page).selectOption("orphan");
  await expect(kind(page)).toBeDisabled();
  await expect(kind(page)).toHaveValue("kuaishou-live");
  await region(page).getByRole("button", { name: "绑定已有登记", exact: true }).click();
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  expect((await saves(page))[0].args.input).toEqual({
    id: "orphan", profileId: FIRST_PROFILE, kind: "kuaishou-live", displayName: orphan.displayName, platformUserId: orphan.platformUserId,
  });
  expect(await savedAccounts(page)).toHaveLength(3);
  expect((await savedAccounts(page)).find((account) => account.id === "elsewhere")).toEqual(elsewhere);
});

test("retained Jinniu scope offers only Jinniu types and compatible orphan records", async ({ page }) => {
  await setBusinessFixture(page, [accountFixture({ profileId: null }), accountFixture({ id: "gold", profileId: null, kind: "jinniu" })], { [FIRST_PROFILE]: "jinniu" });
  await openAccounts(page);
  await expect(kind(page).locator("option")).toHaveCount(1);
  await expect(kind(page)).toHaveValue("jinniu");
  await mode(page).selectOption("existing");
  await expect(existing(page).locator('option[value="account-fixture"]')).toHaveCount(0);
  await expect(existing(page).locator('option[value="gold"]')).toHaveCount(1);
});

test("load errors and busy states are real, retryable, and never fabricate an unbound state", async ({ page }) => {
  await openAccounts(page);
  await expect(create(page)).toBeVisible();
  await page.evaluate(() => { (window as any).__TEST_BUSINESS__.failNext.business_accounts_profile_state = "fixture state database unavailable"; });
  await holdNext(page, "business_accounts_profile_state");
  await region(page).getByRole("button", { name: "重新加载状态", exact: true }).click();
  await waitHeld(page);
  await expect(region(page)).toHaveAttribute("aria-busy", "true");
  await expect(region(page).getByRole("button", { name: "重新加载状态", exact: true })).toBeDisabled();
  await expect(create(page)).toBeDisabled();
  await release(page);
  await expect(region(page).getByRole("alert")).toContainText("fixture state database unavailable");
  await expect(region(page).getByText("当前未关联登记记录", { exact: true })).toHaveCount(0);
  await expect(create(page)).toHaveCount(0);
  await region(page).getByRole("button", { name: "重新加载 / 重试", exact: true }).click();
  await expect(create(page)).toBeVisible();
  expect(await saves(page)).toEqual([]);
});

test("failed explicit save preserves draft; pending save stays busy across section switches", async ({ page }) => {
  await openAccounts(page);
  await alias(page).fill("失败后保留草稿");
  await platformId(page).fill("draft-id");
  await page.evaluate(() => { (window as any).__TEST_BUSINESS__.failNext.business_accounts_save = "fixture account uniqueness conflict"; });
  await holdNext(page, "business_accounts_save");
  await create(page).click();
  await waitHeld(page);
  await expect(create(page)).toBeDisabled();
  await expect(alias(page)).toBeDisabled();
  await page.getByRole("dialog").getByRole("button", { name: "Browser", exact: true }).click();
  await enterAccounts(page);
  await expect(create(page)).toBeDisabled();
  await expect(region(page)).toHaveAttribute("aria-busy", "true");
  await release(page);
  await expect(region(page).getByRole("alert")).toContainText("fixture account uniqueness conflict");
  await expect(alias(page)).toHaveValue("失败后保留草稿");
  await expect(platformId(page)).toHaveValue("draft-id");
  expect(await savedAccounts(page)).toEqual([]);
  await expect(create(page)).toBeEnabled();
  await create(page).click();
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  expect(await saves(page)).toHaveLength(2);
  expect(await savedAccounts(page)).toHaveLength(1);
});

test("running and directory conflicts are explained by backend rather than blocked by UI hints", async ({ page }) => {
  await setRunning(page, [FIRST_PROFILE]);
  await openAccounts(page);
  await expect(alias(page)).toBeEnabled();
  await alias(page).fill("后台仍需校验");
  await create(page).click();
  await expect(region(page).getByRole("alert")).toContainText("Profile must be stopped (backend)");
  expect(await saves(page)).toHaveLength(1);
  await setRunning(page, []);
  await page.evaluate(() => { (window as any).__TEST_BUSINESS__.failNext.business_accounts_save = new Error("实际目录冲突，请使用独立环境"); });
  await create(page).click();
  await expect(region(page).getByRole("alert")).toContainText("实际目录冲突，请使用独立环境");
  await expect(alias(page)).toHaveValue("后台仍需校验");
  expect(await savedAccounts(page)).toEqual([]);
});

test("late profile-state response cannot overwrite another profile after close and switch", async ({ page }) => {
  await setBusinessFixture(page, [accountFixture(), accountFixture({ id: "second-account", profileId: SECOND_PROFILE, kind: "jinniu", displayName: "第二环境金牛" })], { [FIRST_PROFILE]: "kuaishou", [SECOND_PROFILE]: "jinniu" });
  await holdNext(page, "business_accounts_profile_state");
  await openAccounts(page);
  await waitHeld(page);
  await closeEdit(page);
  await openAccounts(page, "Second profile");
  await expect(alias(page)).toHaveValue("第二环境金牛");
  await release(page);
  await expect(alias(page)).toHaveValue("第二环境金牛");
  await expect(kind(page)).toHaveValue("jinniu");
  await alias(page).fill("第二环境修改");
  await region(page).getByRole("button", { name: "保存登记修改", exact: true }).click();
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  expect((await saves(page))[0].args.input.profileId).toBe(SECOND_PROFILE);
  expect((await savedAccounts(page)).find((item) => item.profileId === FIRST_PROFILE)?.displayName).toBe("原登记");
});

test("a completed write with failed refresh requires reload rather than a duplicate create", async ({ page }) => {
  await openAccounts(page);
  await alias(page).fill("保存已成功");
  await page.evaluate(() => { (window as any).__TEST_BUSINESS__.failNext.business_accounts_list = "fixture refresh failed"; });
  await create(page).click();
  await expect(region(page).getByRole("alert")).toContainText("操作已完成，但最新状态读取失败");
  await expect(region(page).getByRole("alert")).toContainText("fixture refresh failed");
  await expect(create(page)).toHaveCount(0);
  expect(await savedAccounts(page)).toHaveLength(1);
  await region(page).getByRole("button", { name: "重新加载 / 重试", exact: true }).click();
  await expect(alias(page)).toHaveValue("保存已成功");
  await expect(region(page).getByRole("button", { name: "保存登记修改", exact: true })).toBeEnabled();
  expect(await saves(page)).toHaveLength(1);
});

test("late save completion cannot inject a previous profile's account into a new profile", async ({ page }) => {
  await openAccounts(page);
  await alias(page).fill("第一环境延迟保存");
  await holdNext(page, "business_accounts_save");
  await create(page).click();
  await waitHeld(page);
  await closeEdit(page);
  await openAccounts(page, "Second profile");
  await alias(page).fill("第二环境本地草稿");
  await release(page);
  await expect.poll(async () => (await savedAccounts(page)).length).toBe(1);
  await expect(alias(page)).toHaveValue("第二环境本地草稿");
  await expect(create(page)).toBeEnabled();
  await expect(region(page).getByText("当前未关联登记记录", { exact: true })).toBeVisible();
  await create(page).click();
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  expect((await saves(page)).map((call) => call.args.input.profileId)).toEqual([FIRST_PROFILE, SECOND_PROFILE]);
});


test("General embeds registration without a new navigation entry and duplicate submits save once", async ({ page }) => {
  await openAccounts(page);
  await expect(page.getByRole("dialog").getByRole("button", { name: "业务账号", exact: true })).toHaveCount(0);
  await alias(page).fill("防重复提交");
  await holdNext(page, "business_accounts_save");
  await create(page).evaluate((button) => {
    const form = (button as HTMLButtonElement).form!;
    form.requestSubmit();
    form.requestSubmit();
  });
  await waitHeld(page);
  expect(await saves(page)).toHaveLength(1);
  await expect(alias(page)).toBeDisabled();
  await release(page);
  await expect(region(page).getByRole("status")).toContainText("账号登记已保存");
  await expect(mode(page)).toHaveCount(0);
  await expect(create(page)).toHaveCount(0);
  expect(await savedAccounts(page)).toHaveLength(1);
});
