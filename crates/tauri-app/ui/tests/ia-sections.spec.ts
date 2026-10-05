import { expect, test, type Page } from "@playwright/test";
import { defaultSettings, installTauriMock } from "./tauriMock";

/**
 * IA 契约（直播板块归档后）：
 * 侧边栏顶级板块：快手账号 / 磁力金牛账号 / 浏览器配置 / MCP / 其它 / 设置。
 * 「直播」入口已移除；原直播板块的三个页面归档到「其它」，
 * 由页面内顶部页签切换（不再依赖侧边栏二级菜单）：
 *   开播准备 → 脚本处理（LiveScriptPage，多视频多脚本占位）
 *   正式开播 → 慧播开播（跟播 / 回播）+ 伴侣开播（本地视频推流 LiveLaunchPage）
 *   直播互动 → 三个堆叠子区（互动账号脚本互动 / 自动上车 / 自动发言），各含真实组件
 *   投流     → 金牛推广（达人授权已移至「磁力金牛账号」的二级菜单）
 * 磁力金牛账号下含 账号 / 达人授权 两个缩进二级菜单：
 *   账号     → 磁力金牛多大户管理
 *   达人授权 → BindAuthorizePage
 * 快手账号内含 小店 / 直播伴侣 / 互动账号 三个二级菜单：
 *   直播伴侣 → 直播伴侣账号管理（MateLoginPage）
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

test("sidebar exposes the Jinniu accounts and Other sections; Live entry is archived", async ({ page }) => {
  await gotoSection(page, "profiles");

  await expect(page.getByRole("button", { name: "磁力金牛账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "直播", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "其它", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "业务", exact: true })).toHaveCount(0);
});

test("Other section hosts the archived live pages with four tabs; prepare hosts script, live hosts both launch pages", async ({ page }) => {
  await gotoSection(page, "business");

  for (const name of ["开播准备", "正式开播", "直播互动", "投流"]) {
    await expect(page.getByRole("tab", { name, exact: true })).toBeVisible();
  }

  // 开播准备（默认页签）：只留脚本处理占位入口。
  const script = page.getByTestId("live-prepare-script");
  await expect(script.getByRole("heading", { name: "脚本处理", exact: true })).toBeVisible();
  // 慧播/伴侣开播已搬到「正式开播」，准备页不再渲染。
  await expect(page.getByTestId("live-prepare-huibo")).toHaveCount(0);
  await expect(page.getByTestId("live-prepare-mate")).toHaveCount(0);

  // 正式开播：慧播开播 + 伴侣开播，两个页面都真实渲染。
  await page.getByRole("tab", { name: "正式开播", exact: true }).click();
  const huibo = page.getByTestId("live-live-huibo");
  await expect(huibo.getByRole("heading", { name: "慧播开播", exact: true })).toBeVisible();
  await expect(huibo.getByRole("heading", { name: "跟播 / 回播", exact: true })).toBeVisible();
  const mate = page.getByTestId("live-live-mate");
  await expect(mate.getByRole("heading", { name: "伴侣开播", exact: true })).toBeVisible();
  await expect(mate.getByRole("region", { name: "开播", exact: true })).toBeVisible();
  // 伴侣账号管理页不属于正式开播（它落在「快手账号 › 直播伴侣」）。
  await expect(page.getByRole("heading", { name: "直播伴侣账号", exact: true })).toHaveCount(0);
});

test("Kuaishou › 直播伴侣 tab hosts the mate account manager", async ({ page }) => {
  await gotoSection(page, "kuaishou", "mate");

  await expect(page.getByRole("button", { name: "快手账号", exact: true })).toBeVisible();
  const login = page.getByRole("heading", { name: "直播伴侣账号", exact: true });
  await expect(login).toBeVisible();
  await expect(page.getByRole("region", { name: "直播伴侣账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "添加账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "刷新", exact: true })).toBeVisible();
  await expect(page.getByPlaceholder("account-id")).toHaveCount(0);
});

test("Live ads tab hosts Jinniu promote (creator authorize moved to Jinniu)", async ({ page }) => {
  await gotoSection(page, "business");
  await page.getByRole("tab", { name: "投流", exact: true }).click();

  await expect(page.getByRole("heading", { name: "金牛推广", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "达人授权", exact: true })).toHaveCount(0);
});

test("Jinniu section hosts accounts and creator authorize sub-menu entries", async ({ page }) => {
  await gotoSection(page, "profiles");
  await page.getByRole("button", { name: "磁力金牛账号", exact: true }).click();

  await expect(page.getByRole("button", { name: "账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "达人授权", exact: true })).toBeVisible();

  // 账号（默认）：磁力金牛多大户管理页。
  await expect(page.getByRole("heading", { name: "磁力金牛账号", exact: true })).toBeVisible();

  // 达人授权：BindAuthorizePage（原「直播 › 投流」内联页）。
  await page.getByRole("button", { name: "达人授权", exact: true }).click();
  await expect(page.getByRole("heading", { name: "达人授权", exact: true })).toBeVisible();
});

test("Live interact tab renders three stacked sub-sections with their real entries", async ({ page }) => {
  await gotoSection(page, "business");
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

test("Jinniu accounts section renders the account manager shell", async ({ page }) => {
  await gotoSection(page, "jinniu");

  // 真实多大户管理页：标题 + 添加账户入口。副标题与空态文案已按产品要求删除。
  // 添加流程（add(label:null) → login）的契约由 jinniu-accounts.spec.ts 覆盖。
  await expect(page.getByRole("heading", { name: "磁力金牛账号", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "添加账户", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "刷新", exact: true })).toBeVisible();
  await expect(page.getByText("管理多个磁力金牛大户")).toHaveCount(0);
  await expect(page.getByText("还没有大户")).toHaveCount(0);
  await expect(page.getByText("建设中", { exact: true })).toHaveCount(0);
});
