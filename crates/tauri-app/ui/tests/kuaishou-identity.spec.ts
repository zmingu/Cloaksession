import { expect, test, type Page } from "@playwright/test";
import { accountFixture, businessRequests, FIRST_PROFILE, savedAccounts, SECOND_PROFILE, setBusinessFixture } from "./businessAccountsMock";
import { profilePatches } from "./tauriMock";
import { holdIdentity, identityCalls, identityFixture, installIdentityMock, PNG, refreshIdentity, releaseIdentity, waitIdentity } from "./kuaishouIdentityMock";

const LIST = "kuaishou_identity_list";
const DETECT = "kuaishou_identity_detect";
const AVATAR = "kuaishou_identity_avatar";
const summary = (page: Page, id = FIRST_PROFILE) => page.getByTestId(`kuaishou-summary-${id}`);
const detail = (page: Page) => page.getByTestId("kuaishou-detail");
const detectButton = (page: Page, id = FIRST_PROFILE) => summary(page, id).getByRole("button", { name: "重新检测", exact: true });
async function openDetails(page: Page, id = FIRST_PROFILE) {
  await summary(page, id).getByRole("button", { name: "快手详情", exact: true }).click();
  await expect(detail(page)).toBeVisible();
}
async function closeDetails(page: Page) {
  await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
}

test.beforeEach(async ({ page }) => {
  // Any accidentally introduced remote avatar URL is blocked, not sent to a real platform.
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, (route) => route.abort());
});

test("detected cards and rows preserve the profile emoji; details copy only ID and remain usable on narrow screens", async ({ page }, testInfo) => {
  await installIdentityMock(page);
  await expect(summary(page)).toHaveAttribute("data-status", "detected");
  await expect(summary(page)).toContainText("本轮已识别");
  await expect(summary(page)).toContainText("本地小店");
  await expect(summary(page).getByRole("img", { name: "快手平台头像" })).toHaveAttribute("src", PNG);
  await expect(page.getByRole("button", { name: /Regression profile/ })).toContainText("🧭");
  await page.screenshot({ path: testInfo.outputPath("identity-grid.png") });
  await page.getByTitle("List view", { exact: true }).click();
  await expect(page.getByRole("row").filter({ has: summary(page) })).toContainText("🧭");
  await expect(summary(page)).toContainText("00123456");
  await openDetails(page);
  await page.screenshot({ path: testInfo.outputPath("identity-detail.png"), animations: "disabled" });
  await expect(page.getByRole("dialog")).toHaveCount(1);
  // Slim dialog: the Kuaishou ID stays; detection timestamps, provenance and
  // the photo/validation/retry panels are gone.
  await expect(detail(page)).toContainText("00123456");
  await detail(page).getByRole("button", { name: "复制信息", exact: true }).click();
  await expect(detail(page).getByRole("status")).toHaveText("已复制快手ID、姓名、身份证号");
  expect(await page.evaluate(() => (window as any).__COPIED_ID__)).toBe("快手ID：00123456\n姓名：\n身份证号：");
  const widths = await detail(page).evaluate((element) => ({ client: element.clientWidth, scroll: element.scrollWidth }));
  expect(widths.scroll).toBeLessThanOrEqual(widths.client + 1);
  await page.keyboard.press("Escape");
  await expect(detail(page)).toHaveCount(0);
  await openDetails(page);
  await closeDetails(page);
  expect(await identityCalls(page, DETECT)).toHaveLength(0);
  expect(await identityCalls(page, AVATAR)).toHaveLength(1);
  expect(await profilePatches(page)).toEqual([]);
  expect(await page.locator("button button").count()).toBe(0);
});

test("offline snapshots show last identity, never a current green result, and cannot trigger detection", async ({ page }) => {
  await installIdentityMock(page, [identityFixture({ status: "closed" })], []);
  await expect(summary(page)).toHaveAttribute("data-status", "closed");
  await expect(summary(page)).toContainText("已关闭 · 上次识别");
  await expect(summary(page)).toContainText("00123456");
  await expect(summary(page)).not.toContainText("本轮已识别");
  await expect(detectButton(page)).toBeDisabled();
  await openDetails(page);
  await expect(detail(page)).toContainText("00123456");
  expect(await identityCalls(page, DETECT)).toEqual([]);
});

test("no identity is not fake data; not-detected, skipped and error preserve historical semantics", async ({ page }) => {
  await installIdentityMock(page, []);
  await expect(summary(page)).toContainText("未检测");
  await expect(summary(page).locator("img")).toHaveCount(0);
  await expect(summary(page)).not.toContainText("快手ID：");
  await openDetails(page);
  await expect(detail(page).getByRole("button", { name: "复制信息", exact: true })).toBeDisabled();
  await closeDetails(page);
  for (const status of ["not-detected", "skipped", "error"] as const) {
    await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.snapshots = [snapshot]; }, identityFixture({ status }));
    await refreshIdentity(page);
    await expect(summary(page)).toHaveAttribute("data-status", status);
    await expect(summary(page)).toContainText("上次识别");
    await expect(summary(page)).not.toContainText("本轮已识别");
  }
});

test("registration conflicts do not overwrite manual records, autosave or unbind", async ({ page }) => {
  await installIdentityMock(page, [identityFixture({ status: "conflict", message: "后台检测到登记冲突" })], []);
  const account = accountFixture({ platformUserId: "99999999" });
  await setBusinessFixture(page, [account], { [FIRST_PROFILE]: "kuaishou" });
  await expect(summary(page)).toContainText("登记冲突");
  await openDetails(page);
  await expect(detail(page)).toContainText("00123456");
  await closeDetails(page);
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: "General", exact: true }).click();
  const region = page.getByRole("region", { name: "业务账号登记", exact: true });
  await expect(region).toContainText("手填平台ID与识别结果不同");
  await expect(region.getByLabel("平台ID（可选，手填不代表已登录）", { exact: true })).toHaveValue("99999999");
  await expect(region).toContainText("不会覆盖人工ID或解除绑定");
  expect(await savedAccounts(page)).toEqual([account]);
  expect((await businessRequests(page)).filter((call) => /save|unbind/.test(call.command))).toEqual([]);
  expect(await profilePatches(page)).toEqual([]);
});

test("manual detection is shared and ignores duplicate clicks, including another mounted view", async ({ page }) => {
  await installIdentityMock(page);
  const key = `${DETECT}:${FIRST_PROFILE}`;
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.detections[snapshot.profileId] = snapshot; }, identityFixture({ platformUserId: "22222222", nickname: "新身份", checkedAt: "2001-01-01T00:00:00Z" }));
  await holdIdentity(page, key);
  await detectButton(page).evaluate((button: HTMLButtonElement) => { button.click(); button.click(); });
  await waitIdentity(page, key);
  await openDetails(page);
  expect(await identityCalls(page, DETECT)).toEqual([{ command: DETECT, args: { profileId: FIRST_PROFILE } }]);
  await releaseIdentity(page, key);
  await expect(detail(page)).toContainText("22222222");
  await expect(summary(page)).toContainText("22222222");
});

test("failed manual detection preserves the last ID as unknown and can be retried", async ({ page }) => {
  await installIdentityMock(page);
  await page.evaluate((key) => { (window as any).__TEST_IDENTITY__.failures[key] = "fixture CDP failure"; }, `${DETECT}:${FIRST_PROFILE}`);
  await detectButton(page).click();
  await expect(summary(page)).toHaveAttribute("data-status", "unknown");
  await expect(summary(page)).toContainText("00123456");
  await openDetails(page);
  await expect(detail(page)).toContainText("00123456");
  await closeDetails(page);
  // Retry from the summary: the slim detail dialog no longer owns a detect control.
  await detectButton(page).click();
  await expect(summary(page)).toHaveAttribute("data-status", "detected");
  expect(await identityCalls(page, DETECT)).toHaveLength(2);
});

test("one global list request is deduplicated and an older list never overwrites a newer manual result", async ({ page }) => {
  await installIdentityMock(page);
  const count = (await identityCalls(page, LIST)).length;
  await holdIdentity(page, LIST);
  const refresh = page.getByRole("button", { name: "刷新检测状态", exact: true });
  await refresh.evaluate((button: HTMLButtonElement) => { button.click(); button.click(); });
  await waitIdentity(page, LIST);
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.detections[snapshot.profileId] = snapshot; }, identityFixture({ platformUserId: "33333333", checkedAt: "1999-01-01T00:00:00Z" }));
  await detectButton(page).click();
  await expect(summary(page)).toContainText("33333333");
  await releaseIdentity(page, LIST);
  await expect(refresh).toBeEnabled();
  await expect(summary(page)).toContainText("33333333");
  expect(await identityCalls(page, LIST)).toHaveLength(count + 1);
});

test("global list errors retain history without stale green and a successful read recovers", async ({ page }) => {
  await installIdentityMock(page);
  await page.evaluate((key) => { (window as any).__TEST_IDENTITY__.failures[key] = "fixture list unavailable"; }, LIST);
  await refreshIdentity(page);
  await expect(summary(page)).toHaveAttribute("data-status", "unknown");
  await expect(summary(page)).toContainText("上次识别");
  await expect(summary(page)).toContainText("00123456");
  await expect(page.getByRole("alert")).toContainText("fixture list unavailable");
  await refreshIdentity(page);
  await expect(summary(page)).toHaveAttribute("data-status", "detected");
  await expect(page.getByRole("alert")).toHaveCount(0);
  expect(await identityCalls(page, DETECT)).toEqual([]);
});

test("multi-profile late manual responses cannot alter the selected profile's details", async ({ page }) => {
  await installIdentityMock(page, [identityFixture(), identityFixture({ profileId: SECOND_PROFILE, platformUserId: "77777777", nickname: "第二小店", avatarKey: null })]);
  const key = `${DETECT}:${FIRST_PROFILE}`;
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.detections[snapshot.profileId] = snapshot; }, identityFixture({ platformUserId: "88888888", nickname: "迟到第一小店" }));
  await holdIdentity(page, key);
  await detectButton(page).click();
  await waitIdentity(page, key);
  await openDetails(page);
  await closeDetails(page);
  await openDetails(page, SECOND_PROFILE);
  await releaseIdentity(page, key);
  await expect(summary(page)).toContainText("88888888");
  await expect(detail(page)).toContainText("77777777");
  await expect(detail(page)).not.toContainText("88888888");
  await expect(detail(page)).not.toContainText("迟到第一小店");
  await detail(page).getByRole("button", { name: "复制信息", exact: true }).click();
  expect(await page.evaluate(() => (window as any).__COPIED_ID__)).toBe("快手ID：77777777\n姓名：\n身份证号：");
});

test("avatars accept local raster data only; invalid payloads and failures fall back without network", async ({ page }) => {
  await installIdentityMock(page);
  const unsafe = ["https://example.invalid/avatar.png", "data:image/svg+xml;base64,PHN2Zy8+", "data:text/html;base64,PGgxPng8L2gxPg==", "data:image/png;base64,PHN2Zy8+", null];
  for (const [index, value] of unsafe.entries()) {
    const key = `unsafe-${index}`;
    await page.evaluate(({ value, key, snapshot }) => {
      const mock = (window as any).__TEST_IDENTITY__;
      mock.avatars[key] = value;
      mock.snapshots = [snapshot];
    }, { value, key, snapshot: identityFixture({ avatarKey: key }) });
    await refreshIdentity(page);
    await expect(summary(page).locator("img")).toHaveCount(0);
    await expect(summary(page).getByLabel("平台头像不可用")).toBeVisible();
  }
  await page.evaluate((snapshot) => {
    const mock = (window as any).__TEST_IDENTITY__;
    mock.snapshots = [snapshot];
    mock.failures[`kuaishou_identity_avatar:${snapshot.avatarKey}`] = "fixture avatar missing";
  }, identityFixture({ avatarKey: "fails" }));
  await refreshIdentity(page);
  await expect(summary(page).locator("img")).toHaveCount(0);
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.snapshots = [snapshot]; }, identityFixture({ avatarKey: "x".repeat(257) }));
  await refreshIdentity(page);
  expect((await identityCalls(page, AVATAR)).every((call: any) => call.args.avatarKey.length <= 256)).toBe(true);
});

test("late avatar responses cannot leak an old photo after the identity and avatar key change", async ({ page }) => {
  await installIdentityMock(page);
  const key = `${AVATAR}:old-held`;
  await holdIdentity(page, key);
  await page.evaluate(({ snapshot, png }) => {
    const mock = (window as any).__TEST_IDENTITY__;
    mock.snapshots = [snapshot];
    mock.avatars["old-held"] = png;
  }, { snapshot: identityFixture({ avatarKey: "old-held" }), png: PNG });
  await refreshIdentity(page);
  await waitIdentity(page, key);
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.snapshots = [snapshot]; }, identityFixture({ platformUserId: "99999999", nickname: "无头像的新ID", avatarKey: "new-missing" }));
  await refreshIdentity(page);
  await expect(summary(page)).toContainText("99999999");
  await releaseIdentity(page, key);
  await expect(summary(page).locator("img")).toHaveCount(0);
  await openDetails(page);
  await expect(detail(page).locator("img")).toHaveCount(0);
  await expect(detail(page)).toContainText("无头像的新ID");
});

test("periodic refresh only lists once per interval and never invokes detect", async ({ page }) => {
  await page.clock.install();
  await installIdentityMock(page);
  const count = (await identityCalls(page, LIST)).length;
  await page.clock.fastForward(15_100);
  await expect.poll(async () => (await identityCalls(page, LIST)).length).toBe(count + 1);
  expect(await identityCalls(page, DETECT)).toEqual([]);
});

test("a list started during manual detection cannot replace it even when it returns last", async ({ page }) => {
  await installIdentityMock(page);
  const key = `${DETECT}:${FIRST_PROFILE}`;
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.detections[snapshot.profileId] = snapshot; }, identityFixture({ platformUserId: "44444444" }));
  await holdIdentity(page, key);
  await detectButton(page).click();
  await waitIdentity(page, key);
  await holdIdentity(page, LIST);
  await page.getByRole("button", { name: "刷新检测状态", exact: true }).click();
  await waitIdentity(page, LIST);
  await releaseIdentity(page, key);
  await expect(summary(page)).toContainText("44444444");
  await releaseIdentity(page, LIST);
  await expect(page.getByRole("button", { name: "刷新检测状态", exact: true })).toBeEnabled();
  await expect(summary(page)).toContainText("44444444");
});

test("malformed list and mismatched manual profile IDs are visible errors, never successful identity updates", async ({ page }) => {
  await installIdentityMock(page);
  await page.evaluate(() => { (window as any).__TEST_IDENTITY__.snapshots = [{ profileId: "fixture-profile", status: "detected" }]; });
  await refreshIdentity(page);
  await expect(summary(page)).toHaveAttribute("data-status", "unknown");
  await expect(summary(page)).toContainText("00123456");
  await expect(page.getByRole("alert")).toContainText("身份检测结果字段无效");
  await page.evaluate(({ id, snapshot }) => { (window as any).__TEST_IDENTITY__.detections[id] = snapshot; }, { id: FIRST_PROFILE, snapshot: identityFixture({ profileId: SECOND_PROFILE, platformUserId: "55555555" }) });
  await detectButton(page).click();
  await openDetails(page);
  await expect(detail(page)).not.toContainText("55555555");
});

test("details opened from registration close with Escape without disturbing the profile or registration draft", async ({ page }) => {
  await installIdentityMock(page, [identityFixture()], []);
  await setBusinessFixture(page, [accountFixture({ platformUserId: "99999999" })], { [FIRST_PROFILE]: "kuaishou" });
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: "General", exact: true }).click();
  const region = page.getByRole("region", { name: "业务账号登记", exact: true });
  await region.getByLabel("账号别名（必填）", { exact: true }).fill("未保存登记草稿");
  await region.getByRole("button", { name: "快手详情", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(2);
  await detail(page).getByRole("button", { name: "复制信息", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(detail(page)).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(1);
  await expect(region.getByLabel("账号别名（必填）", { exact: true })).toHaveValue("未保存登记草稿");
  expect((await businessRequests(page)).filter((call) => /save|unbind/.test(call.command))).toEqual([]);
  expect(await profilePatches(page)).toEqual([]);
});
