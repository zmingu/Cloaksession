import { expect, test, type Page } from "@playwright/test";
import { installIdentityMock, PNG } from "./kuaishouIdentityMock";
import { FIRST_PROFILE } from "./businessAccountsMock";
import type { KuaishouSubjectArchive } from "../src/lib/kuaishouSubject";

const id = "00123456";
const key = `${"a".repeat(64)}.png`;
const archive: KuaishouSubjectArchive = {
  platformUserId: id, realName: "合成样本", idCard: "990001200002291234", source: "ocr",
  evidence: { realName: "合*本", idCard: "990001************" },
  attachments: [{ key, sha256: "a".repeat(64), mimeType: "image/png", byteLen: 100, width: 1, height: 1 }],
  validation: { name: "passed", structure: "passed", checksum: "passed", birthDate: "passed", visibleName: "passed", visibleIdCard: "passed" },
  reviewStatus: "pending-review", revision: 1, sourceProfileId: null,
  createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z", confirmedAt: null,
};

// Contract/UI fixture only: no native OCR, database validation, platform calls or real documents.
async function installSubjectMock(page: Page, invalid = false) {
  // This spec asserts localized (zh-CN) labels, so the shared mock runs in zh-CN.
  await installIdentityMock(page, undefined, undefined, "zh-CN");
  await page.evaluate(({ archive, invalid, png }) => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    const mock = {
      archive: structuredClone(archive),
      calls: [] as Array<{ command: string; args: any }>,
      conflict: false,
      noProfiles: false,
      badPhoto: false,
    };
    if (invalid) mock.archive.validation.checksum = "failed";
    Object.assign(window, { __TEST_SUBJECT__: mock });
    internals.invoke = async (command: string, args: any = {}) => {
      if (command === "profiles_list" && mock.noProfiles) return [];
      if (!command.startsWith("kuaishou_subject_") && !command.startsWith("kuaishou_init_") && command !== "kuaishou_ocr_availability") return original(command, args);
      mock.calls.push({ command, args: structuredClone(args) });
      const a = mock.archive;
      switch (command) {
        case "kuaishou_subject_list": {
          const { search, offset, limit } = args.query;
          const matches = [a.realName, a.idCard, a.platformUserId].some(s => s.includes(search));
          const item = { platformUserId: a.platformUserId, realName: a.realName, maskedIdCard: "990001********1234", nickname: "离线档案", avatarKey: null, source: a.source, reviewStatus: a.reviewStatus, revision: a.revision, updatedAt: a.updatedAt, profiles: [] };
          return { items: matches && offset === 0 ? [item] : [], total: matches ? 1 : 0, offset, limit };
        }
        case "kuaishou_subject_detail": return { archive: structuredClone(a), nickname: "离线档案", avatarKey: null, profiles: [], steps: [] };
        case "kuaishou_init_steps": return [];
        case "kuaishou_ocr_availability": return { available: false, message: "合成测试：系统 OCR 不可用" };
        case "kuaishou_subject_attachment": if (mock.badPhoto) throw new Error("fixture unreadable"); return png;
        case "kuaishou_subject_correct":
        case "kuaishou_subject_confirm": {
          if (args.input.expectedRevision !== a.revision || mock.conflict) {
            mock.conflict = false; a.revision += 1; throw new Error("资料版本已变化");
          }
          if (command === "kuaishou_subject_correct") {
            a.realName = args.input.realName; a.idCard = args.input.idCard;
            a.validation.checksum = "passed"; a.reviewStatus = "pending-review";
          } else { a.reviewStatus = "confirmed"; a.confirmedAt = a.updatedAt; }
          a.revision += 1; return structuredClone(a);
        }
        default: throw new Error(`Unexpected write-capable fixture command: ${command}`);
      }
    };
  }, { archive, invalid, png: PNG });
}
async function calls(page: Page, command: string) {
  return page.evaluate(command => (window as any).__TEST_SUBJECT__.calls.filter((c: any) => c.command === command), command);
}
async function openArchive(page: Page) {
  await page.getByRole("textbox", { name: "搜索环境与账号档案" }).fill("合成");
  await page.getByTestId(`archive-${id}`).getByRole("button", { name: "查看账号档案" }).click();
  await expect(page.getByTestId("subject-panel").getByLabel("主体姓名", { exact: true })).toHaveValue("合成样本");
}

test.beforeEach(async ({ page }) => {
  await page.route(/https?:\/\/(?!127\.0\.0\.1(?::|\/))/, route => route.abort());
});

test("zero-profile orphan archives remain searchable and copyable without browser actions or photo prefetch", async ({ page }) => {
  await installSubjectMock(page);
  // Simulate a profile refresh after the last environment was removed. The archive is independent.
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.evaluate(() => { (window as any).__TEST_SUBJECT__.noProfiles = true; });
  await page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }).click();
  await expect(page.getByTestId(`kuaishou-summary-${FIRST_PROFILE}`)).toHaveCount(0);
  const search = page.getByRole("textbox", { name: "搜索环境与账号档案" });
  await search.fill(archive.idCard);
  const row = page.getByTestId(`archive-${id}`);
  await expect(row).toContainText("无关联环境 / 环境已删除");
  await expect(row).not.toContainText(archive.idCard);
  expect(await calls(page, "kuaishou_subject_detail")).toHaveLength(0);
  expect(await calls(page, "kuaishou_subject_attachment")).toHaveLength(0);
  await row.getByRole("button", { name: "查看账号档案" }).click();
  const panel = page.getByTestId("subject-panel");
  await expect(panel.getByRole("img", { name: "主体证件照片 1" })).toHaveAttribute("src", PNG);
  await expect(panel.getByRole("button", { name: "执行 / 补做初始化" })).toHaveCount(0);
  await panel.getByRole("button", { name: "复制主体资料", exact: true }).click();
  expect(await page.evaluate(() => (window as any).__COPIED_ID__)).toBe(`未核对\n快手ID：${id}\n姓名：合成样本\n身份证号：${archive.idCard}`);
  expect(await calls(page, "kuaishou_init_retry")).toHaveLength(0);
});

test("invalid and dirty candidates cannot confirm; revision conflict refreshes without forced confirmation", async ({ page }) => {
  await installSubjectMock(page, true);
  await openArchive(page);
  const panel = page.getByTestId("subject-panel");
  const confirm = panel.getByRole("button", { name: "确认已核对", exact: true });
  await expect(confirm).toBeDisabled();
  await expect(panel.getByRole("button", { name: "重新识别（本地 OCR）" })).toBeDisabled();
  await panel.getByLabel("主体姓名", { exact: true }).fill("修正样本");
  await expect(confirm).toBeDisabled();
  await panel.getByRole("button", { name: "保存修正并校验" }).click();
  await expect(confirm).toBeEnabled();
  expect((await calls(page, "kuaishou_subject_correct"))[0].args.input.expectedRevision).toBe(1);
  await page.evaluate(() => { (window as any).__TEST_SUBJECT__.conflict = true; });
  await confirm.click();
  await expect(panel.getByRole("alert")).toContainText("版本冲突");
  await expect(panel.getByTestId("subject-review")).toContainText("待核对");
  await expect(panel.getByTestId("subject-review")).toContainText("版本 3");
  await confirm.click();
  await expect(panel.getByTestId("subject-review")).toContainText("已核对");
  expect((await calls(page, "kuaishou_subject_confirm")).map((call: any) => call.args.input.expectedRevision)).toEqual([2, 3]);
  expect(await calls(page, "kuaishou_init_retry")).toHaveLength(0);
});

test("missing local photos render failure, never a remote image or OCR fallback", async ({ page }) => {
  await installSubjectMock(page);
  await page.evaluate(() => { (window as any).__TEST_SUBJECT__.badPhoto = true; });
  await openArchive(page);
  const panel = page.getByTestId("subject-panel");
  await expect(panel).toContainText("照片读取失败");
  await expect(panel.locator("img")).toHaveCount(0);
  expect(await calls(page, "kuaishou_subject_attachment")).toHaveLength(1); // StrictMode replay shares this failed request.
  await page.evaluate(() => { (window as any).__TEST_SUBJECT__.badPhoto = false; });
  await panel.getByRole("button", { name: "刷新档案", exact: true }).click();
  await expect(panel.getByRole("img", { name: "主体证件照片 1" })).toHaveAttribute("src", PNG);
  expect(await calls(page, "kuaishou_subject_attachment")).toHaveLength(2);
  await page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }).click();
  await expect(panel).toHaveCount(0);
  // A real reopen must revalidate the local file, not retain the successful image globally.
  await page.evaluate(() => { (window as any).__TEST_SUBJECT__.badPhoto = true; });
  await page.getByTestId(`archive-${id}`).getByRole("button", { name: "查看账号档案" }).click();
  await expect(panel).toContainText("照片读取失败");
  await expect(panel.locator("img")).toHaveCount(0);
  expect(await calls(page, "kuaishou_subject_attachment")).toHaveLength(3);
  expect(await calls(page, "kuaishou_subject_reocr")).toHaveLength(0);
  expect(await calls(page, "kuaishou_init_retry")).toHaveLength(0);
});

test("archive pagination resets with search and keeps document payloads out of list reads", async ({ page }) => {
  await installSubjectMock(page);
  await page.evaluate(() => {
    const internals = (window as any).__TAURI_INTERNALS__;
    const original = internals.invoke;
    internals.invoke = async (command: string, args: any = {}) => {
      if (command !== "kuaishou_subject_list") return original(command, args);
      (window as any).__TEST_SUBJECT__.calls.push({ command, args: structuredClone(args) });
      const { search, offset, limit } = args.query;
      const items = Array.from({ length: 11 }, (_, i) => ({
        platformUserId: String(50000 + i), realName: `分页样本${i}`, maskedIdCard: "990001********1234",
        nickname: null, avatarKey: null, source: "ocr", reviewStatus: "pending-review", revision: 1,
        updatedAt: "2026-10-01T00:00:00Z", profiles: [],
      })).filter(item => !search || item.platformUserId.includes(search) || item.realName.includes(search));
      return { items: items.slice(offset, offset + limit), total: items.length, offset, limit };
    };
  });
  const search = page.getByRole("textbox", { name: "搜索环境与账号档案" });
  await search.fill("分页");
  await expect(page.getByTestId("archive-50000")).toBeVisible();
  await expect(page.getByTestId("archive-50010")).toHaveCount(0);
  await page.getByRole("button", { name: "下一页档案" }).click();
  await expect(page.getByTestId("archive-50010")).toBeVisible();
  await expect(page.getByRole("button", { name: "下一页档案" })).toBeDisabled();
  await search.fill("50000");
  await expect(page.getByTestId("archive-50000")).toBeVisible();
  await expect(page.getByTestId("archive-50010")).toHaveCount(0);
  const requests = await calls(page, "kuaishou_subject_list");
  expect(requests.at(-1).args.query).toEqual({ search: "50000", offset: 0, limit: 10 });
  expect(await calls(page, "kuaishou_subject_detail")).toHaveLength(0);
  expect(await calls(page, "kuaishou_subject_attachment")).toHaveLength(0);
});
