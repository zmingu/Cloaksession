import { expect, test, type Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";

/**
 * IA 契约（业务板块搬迁后）：
 * 侧边栏顶级板块：快手账号 / 磁力金牛账号 / 直播 / 浏览器配置 / MCP / 其它 / 设置。
 * 直播内含 开播准备 / 直播互动 / 投流 三个页签：
 *   开播准备 → 慧播开播（跟播 / 回播）+ 伴侣开播（本地视频推流 LiveLaunchPage）
 *   直播互动 → 三个堆叠子区（互动账号脚本互动 / 自动上车 / 自动发言），各含真实组件
 *   投流     → 金牛推广 + 达人授权
 * 快手账号内含 小店 / 直播伴侣 / 互动账号 三个二级菜单：
 *   直播伴侣 → 伴侣扫码登录（MateLoginPage）
 * 「其它」板块内容已清空，只剩空态占位。
 *
 * 只验证结构（标题/子区/空态），不触碰任何业务 IPC。用 zh-CN 断言中文标签。
 */

/** 直接落在指定 section 上（持久化在 localStorage，与既有 spec 一致）。 */
async function gotoSection(page: Page, section: string, kuaishouTab?: string): Promise<void> {
  await installTauriMock(page, { ...defaultSettings, language: "zh-CN" });
  await page.addInitScript(({ s, tab }) => {
    localStorage.setItem("multizen.ui.section", JSON.stringify(s));
    if (tab) localStorage.setItem("multizen.ui.kuaishouTab", JSON.stringify(tab));
  }, { s: section, tab: kuaishouTab ?? null });
  await page.goto("/");
}

test("sidebar exposes the Jinniu accounts, Live and Other sections", async ({ page }) => {
  await gotoSection(page, "profiles");

  await expect(page.getByRole("button", { name: "磁力金牛账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "直播", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "其它", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "业务", exact: true })).toHaveCount(0);
});

test("Live section renders three tabs and the prepare tab hosts both launch pages", async ({ page }) => {
  await gotoSection(page, "profiles");
  await page.getByRole("button", { name: "直播", exact: true }).click();

  for (const name of ["开播准备", "直播互动", "投流"]) {
    await expect(page.getByRole("tab", { name, exact: true })).toBeVisible();
  }

  // 开播准备：慧播开播 + 伴侣开播，两个页面都真实渲染。
  await page.getByRole("tab", { name: "开播准备", exact: true }).click();
  const huibo = page.getByTestId("live-prepare-huibo");
  await expect(huibo.getByRole("heading", { name: "慧播开播", exact: true })).toBeVisible();
  await expect(huibo.getByRole("heading", { name: "跟播 / 回播", exact: true })).toBeVisible();
  const mate = page.getByTestId("live-prepare-mate");
  await expect(mate.getByRole("heading", { name: "伴侣开播", exact: true })).toBeVisible();
  await expect(mate.getByRole("region", { name: "开播", exact: true })).toBeVisible();
  // 伴侣扫码登录页不再属于开播准备（它落在「快手账号 › 直播伴侣」）。
  await expect(page.getByRole("heading", { name: "伴侣扫码登录", exact: true })).toHaveCount(0);
});

test("Kuaishou › 直播伴侣 tab hosts the mate QR login page", async ({ page }) => {
  await gotoSection(page, "kuaishou", "mate");

  await expect(page.getByRole("button", { name: "快手账号", exact: true })).toBeVisible();
  const login = page.getByRole("heading", { name: "伴侣扫码登录", exact: true });
  await expect(login).toBeVisible();
  await expect(page.getByRole("region", { name: "伴侣扫码登录", exact: true })).toBeVisible();
  await expect(page.getByPlaceholder("account-id")).toBeVisible();
  await expect(page.getByRole("button", { name: "开始登录", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "取消", exact: true })).toBeVisible();
});

test("Live ads tab hosts Jinniu promote and creator authorize", async ({ page }) => {
  await gotoSection(page, "profiles");
  await page.getByRole("button", { name: "直播", exact: true }).click();
  await page.getByRole("tab", { name: "投流", exact: true }).click();

  await expect(page.getByRole("heading", { name: "金牛推广", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "达人授权", exact: true })).toBeVisible();
});

test("Live interact tab renders three stacked sub-sections with their real entries", async ({ page }) => {
  await gotoSection(page, "profiles");
  await page.getByRole("button", { name: "直播", exact: true }).click();
  await page.getByRole("tab", { name: "直播互动", exact: true }).click();

  const panel = page.getByTestId("live-interact-page");
  await expect(panel).toBeVisible();

  // 三个子区按固定顺序堆叠：互动账号脚本互动 → 自动上车 → 自动发言。
  const sections = [
    { testId: "live-interact-script", title: "互动账号脚本互动" },
    { testId: "live-interact-shelf", title: "自动上车" },
    { testId: "live-interact-speak", title: "自动发言" },
  ];
  for (const { testId, title } of sections) {
    await expect(page.getByTestId(testId).getByRole("heading", { name: title, exact: true })).toBeVisible();
  }

  // 各子区分别含预期组件入口（按组件的 region 锚定；弹幕监听/小号在无后端
  // fixture 时会降级为 note，故其内容由 listen-sub.spec.ts 覆盖）。
  const script = page.getByTestId("live-interact-script");
  await expect(script.getByRole("region", { name: "直播间监控", exact: true })).toBeVisible();
  await expect(script.getByRole("region", { name: "场景剧本", exact: true })).toBeVisible();

  const shelf = page.getByTestId("live-interact-shelf");
  await expect(shelf.getByRole("region", { name: "跟播助手上车", exact: true })).toBeVisible();
  await expect(shelf.getByRole("region", { name: "商品话术库", exact: true })).toBeVisible();

  const speak = page.getByTestId("live-interact-speak");
  await expect(speak.getByRole("region", { name: "主播互动", exact: true })).toBeVisible();
  await expect(speak.getByRole("region", { name: "自动回复", exact: true })).toBeVisible();
  await expect(speak.getByRole("region", { name: "自动弹窗（轮换讲解）", exact: true })).toBeVisible();
  await expect(speak.getByTestId("comments-page")).toBeVisible();
});

test("Other section is an empty placeholder with no tabs", async ({ page }) => {
  await gotoSection(page, "business");

  await expect(page.getByRole("heading", { name: "其它", exact: true })).toBeVisible();
  await expect(page.getByText("建设中", { exact: true })).toBeVisible();
  await expect(page.getByRole("tab")).toHaveCount(0);
});

test("Jinniu accounts section renders the real multi-master shell", async ({ page }) => {
  await gotoSection(page, "jinniu");

  // 已从占位壳升级为真实多大户管理页：标题 + 副标题 + 添加大户入口，不再有「建设中」。
  await expect(page.getByRole("heading", { name: "磁力金牛账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "添加大户", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "刷新", exact: true })).toBeVisible();
  await expect(page.getByText("建设中", { exact: true })).toHaveCount(0);
});
