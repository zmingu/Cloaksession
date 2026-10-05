import { expect, test, type Page } from "@playwright/test";
import { dshopRequests, installDShopMock } from "./dShopPopupMock";

// Every navigation is loopback and every D-group command is a local Tauri fake.
// The base fixture language is English, so assertions use the en dictionary.
//
// 业务板块搬迁后：商品话术库 / 跟播助手在「直播 › 直播互动 › 自动上车」，
// 自动弹窗在「直播 › 直播互动 › 自动发言」。同一页签内多个组件同时挂载，
// 因此所有断言都按组件 region 作用域限定，避免同名按钮（Start / Stop / Add line）串扰。
test.beforeEach(async ({ page }) => {
  await installDShopMock(page);
});

// The mock already landed on the Live section; open the interact tab.
async function gotoInteract(page: Page): Promise<void> {
  await page.getByRole("tab", { name: /直播互动|Live interact/ }).click();
}

const pscript = (page: Page) => page.getByRole("region", { name: "Product script library" });
const popup = (page: Page) => page.getByRole("region", { name: "Auto popup (rotating explain)" });

test("product scripts render, create, and delete with two-step confirm", async ({ page }) => {
  await gotoInteract(page);
  await expect(pscript(page).getByText("Product script library").first()).toBeVisible();
  await expect(pscript(page).getByText("晚场 A")).toBeVisible();
  // React StrictMode double-mounts in dev, so the mount load may fire twice.
  expect((await dshopRequests(page, "shop_product_scripts_list")).length).toBeGreaterThanOrEqual(1);

  // Create.
  await pscript(page).getByPlaceholder("e.g. Evening session A").fill("深夜场 B");
  await pscript(page).getByRole("button", { name: "Create script", exact: true }).click();
  await expect(pscript(page).getByText("深夜场 B")).toBeVisible();
  const creates = await dshopRequests(page, "shop_product_script_create");
  expect(creates).toHaveLength(1);
  expect(creates[0].args.input).toEqual({ name: "深夜场 B", description: null });

  // Delete requires a second confirm click; the first click sends nothing.
  // Scope to the created row (new scripts prepend, so DOM order is unstable).
  const createdRow = page.getByTestId("pscript-list").locator(":scope > div", { hasText: "深夜场 B" });
  await createdRow.getByRole("button", { name: "Delete", exact: true }).click();
  expect(await dshopRequests(page, "shop_product_script_delete")).toEqual([]);
  await pscript(page).getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(pscript(page).getByText("深夜场 B")).toHaveCount(0);
  expect(await dshopRequests(page, "shop_product_script_delete")).toHaveLength(1);
});

test("product script play requires confirm and renders the state event", async ({ page }) => {
  await gotoInteract(page);
  const row = page.getByTestId("pscript-list").locator(":scope > div", { hasText: "晚场 A" });

  // Playback is a write: the first click only arms the confirm group.
  await row.getByRole("button", { name: "Play", exact: true }).click();
  await expect(pscript(page).getByText("real writes", { exact: false }).first()).toBeVisible();
  expect(await dshopRequests(page, "shop_product_script_play")).toEqual([]);
  await pscript(page).getByRole("button", { name: "Confirm", exact: true }).click();
  const plays = await dshopRequests(page, "shop_product_script_play");
  expect(plays).toHaveLength(1);
  expect(plays[0].args).toEqual({
    profileId: "fixture-profile",
    scriptId: "script-1",
    goodsIds: null,
    startedAtMs: null,
  });
  await expect(pscript(page).getByText("Playback started", { exact: false }).first()).toBeVisible();

  // The `product-script-state-changed` listener renders the broadcast snapshot.
  await page.evaluate(() => {
    (window as any).__TEST_IPC__.emit("product-script-state-changed", {
      scriptId: "script-1",
      status: "playing",
      currentLineId: "line-1",
      currentIndex: 1,
      total: 2,
      startedAtMs: 0,
    });
  });
  await expect(page.getByTestId("pscript-play-state")).toContainText("playing");
  await expect(page.getByTestId("pscript-play-state")).toContainText("1/2");
  await expect(page.getByTestId("pscript-play-state")).toContainText("line-1");
});

test("product script detail edits lines and reorders", async ({ page }) => {
  await gotoInteract(page);
  await pscript(page).getByRole("button", { name: "Open", exact: true }).first().click();
  await expect(pscript(page).getByText("Script detail").first()).toBeVisible();
  await expect(pscript(page).getByText("好吃的苹果")).toBeVisible();
  expect(await dshopRequests(page, "shop_product_script_get")).toHaveLength(1);

  // Add a line.
  await pscript(page).getByText("Goods ID (1–128 chars)").first().locator("..").locator("input").fill("2002");
  await pscript(page).getByRole("button", { name: "Add line", exact: true }).click();
  expect(await dshopRequests(page, "shop_product_script_add_line")).toHaveLength(1);

  // Reorder via the move buttons.
  await pscript(page).getByRole("button", { name: "↓", exact: true }).first().click();
  const reorders = await dshopRequests(page, "shop_product_script_reorder_lines");
  expect(reorders).toHaveLength(1);
  expect(reorders[0].args).toEqual({ scriptId: "script-1", orderedIds: ["line-2", "line-1"] });
});

test("shop helper reads goods, boards with confirm, and shows the event panel", async ({ page }) => {
  await gotoInteract(page);
  const helper = page.getByRole("region", { name: "Shop helper boarding" });
  await expect(helper).toBeVisible();

  await helper.getByPlaceholder("Target id of the helper page").fill("target-1");
  await helper.getByRole("button", { name: "Read goods", exact: true }).click();
  await expect(helper.getByText("红苹果 · 1001")).toBeVisible();
  expect(await dshopRequests(page, "shop_helper_read_goods")).toHaveLength(1);

  // Boarding is a write: first click only arms the confirm group.
  await helper.getByRole("button", { name: "Board", exact: true }).click();
  await expect(helper.getByText("is a real listing write", { exact: false }).first()).toBeVisible();
  expect(await dshopRequests(page, "shop_helper_add_to_cart")).toEqual([]);
  await helper.getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(helper.getByText("Done:", { exact: false }).first()).toBeVisible();
  const adds = await dshopRequests(page, "shop_helper_add_to_cart");
  expect(adds).toHaveLength(1);
  expect(adds[0].args).toEqual({ profileId: "fixture-profile", targetId: "target-1", goodsId: "1001" });

  // Event panel renders (no backend event in the mock, so it stays empty).
  await expect(page.getByTestId("helper-event")).toContainText("—");
});

test("auto popup starts, explains once with confirm, and stops with confirm", async ({ page }) => {
  await gotoInteract(page);
  await expect(popup(page)).toBeVisible();

  await popup(page).getByRole("button", { name: "Start", exact: true }).click();
  await expect(popup(page).getByText("Auto popup started.").first()).toBeVisible();
  const starts = await dshopRequests(page, "auto_popup_start");
  expect(starts).toHaveLength(1);
  expect(starts[0].args.profileId).toBe("fixture-profile");
  expect(starts[0].args.config.interval).toEqual([30, 60]);

  // Single explain requires confirm.
  await popup(page).getByText("Goods serial", { exact: true }).locator("..").locator("input").fill("001");
  await popup(page).getByRole("button", { name: "Explain once", exact: true }).click();
  await expect(popup(page).getByText("clicks the explain button", { exact: false }).first()).toBeVisible();
  expect(await dshopRequests(page, "auto_popup_explain_once")).toEqual([]);
  await popup(page).getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(popup(page).getByText("Explained goods 001.").first()).toBeVisible();
  expect(await dshopRequests(page, "auto_popup_explain_once")).toHaveLength(1);

  // Stop requires confirm.
  await popup(page).getByRole("button", { name: "Stop", exact: true }).click();
  expect(await dshopRequests(page, "auto_popup_stop")).toEqual([]);
  await popup(page).getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(popup(page).getByText("Auto popup stopped.").first()).toBeVisible();
  expect(await dshopRequests(page, "auto_popup_stop")).toHaveLength(1);
});

test("auto popup goods, scan, and shortcuts round-trip", async ({ page }) => {
  await gotoInteract(page);

  await page.getByTestId("popup-status").getByRole("button", { name: "Refresh status", exact: true }).click();
  expect(await dshopRequests(page, "auto_popup_status")).toHaveLength(1);

  await page.getByTestId("popup-goods").getByRole("button", { name: "Refresh status", exact: true }).click();
  await expect(popup(page).getByText("001 · 苹果 · 9.9")).toBeVisible();
  await page.getByTestId("popup-goods").getByRole("button", { name: "Scan knowledge", exact: true }).click();
  await expect(popup(page).getByText("No diffs. 1 candidates scanned.").first()).toBeVisible();

  await popup(page).getByRole("button", { name: "Register shortcuts", exact: true }).click();
  const regs = await dshopRequests(page, "auto_popup_register_shortcuts");
  expect(regs).toHaveLength(1);
  expect(regs[0].args.bindings).toEqual({ "CommandOrControl+1": "001" });
});
