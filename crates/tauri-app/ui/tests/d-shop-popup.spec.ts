import { expect, test, type Page } from "@playwright/test";
import { dshopRequests, installDShopMock, openProfileEditor } from "./dShopPopupMock";

// Every navigation is loopback and every D-group command is a local Tauri fake.
// The base fixture language is English, so assertions use the en dictionary.
test.beforeEach(async ({ page }) => {
  await installDShopMock(page);
});

async function gotoSection(page: Page, name: string): Promise<void> {
  await page.getByRole("dialog").getByRole("button", { name, exact: true }).click();
}

test("product scripts render, create, and delete with two-step confirm", async ({ page }) => {
  await openProfileEditor(page);
  await gotoSection(page, "Product scripts");
  await expect(page.getByText("Product script library").first()).toBeVisible();
  await expect(page.getByText("晚场 A")).toBeVisible();
  // React StrictMode double-mounts in dev, so the mount load may fire twice.
  expect((await dshopRequests(page, "shop_product_scripts_list")).length).toBeGreaterThanOrEqual(1);

  // Create.
  await page.getByPlaceholder("e.g. Evening session A").fill("深夜场 B");
  await page.getByRole("button", { name: "Create script", exact: true }).click();
  await expect(page.getByText("深夜场 B")).toBeVisible();
  const creates = await dshopRequests(page, "shop_product_script_create");
  expect(creates).toHaveLength(1);
  expect(creates[0].args.input).toEqual({ name: "深夜场 B", description: null });

  // Delete requires a second confirm click; the first click sends nothing.
  // Scope to the created row (new scripts prepend, so DOM order is unstable).
  const createdRow = page.getByTestId("pscript-list").locator(":scope > div", { hasText: "深夜场 B" });
  await createdRow.getByRole("button", { name: "Delete", exact: true }).click();
  expect(await dshopRequests(page, "shop_product_script_delete")).toEqual([]);
  await page.getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(page.getByText("深夜场 B")).toHaveCount(0);
  expect(await dshopRequests(page, "shop_product_script_delete")).toHaveLength(1);
});

test("product script detail edits lines and reorders", async ({ page }) => {
  await openProfileEditor(page);
  await gotoSection(page, "Product scripts");
  await page.getByRole("button", { name: "Open", exact: true }).first().click();
  await expect(page.getByText("Script detail").first()).toBeVisible();
  await expect(page.getByText("好吃的苹果")).toBeVisible();
  expect(await dshopRequests(page, "shop_product_script_get")).toHaveLength(1);

  // Add a line.
  await page.getByText("Goods ID (1–128 chars)").first().locator("..").locator("input").fill("2002");
  await page.getByRole("button", { name: "Add line", exact: true }).click();
  expect(await dshopRequests(page, "shop_product_script_add_line")).toHaveLength(1);

  // Reorder via the move buttons.
  await page.getByRole("button", { name: "↓", exact: true }).first().click();
  const reorders = await dshopRequests(page, "shop_product_script_reorder_lines");
  expect(reorders).toHaveLength(1);
  expect(reorders[0].args).toEqual({ scriptId: "script-1", orderedIds: ["line-2", "line-1"] });
});

test("shop helper reads goods, boards with confirm, and shows the event panel", async ({ page }) => {
  await openProfileEditor(page);
  await gotoSection(page, "Shop helper");
  await expect(page.getByText("Shop helper boarding").first()).toBeVisible();

  await page.getByPlaceholder("Target id of the helper page").fill("target-1");
  await page.getByRole("button", { name: "Read goods", exact: true }).click();
  await expect(page.getByText("红苹果 · 1001")).toBeVisible();
  expect(await dshopRequests(page, "shop_helper_read_goods")).toHaveLength(1);

  // Boarding is a write: first click only arms the confirm group.
  await page.getByRole("button", { name: "Board", exact: true }).click();
  await expect(page.getByText("is a real listing write", { exact: false }).first()).toBeVisible();
  expect(await dshopRequests(page, "shop_helper_add_to_cart")).toEqual([]);
  await page.getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(page.getByText("Done:", { exact: false }).first()).toBeVisible();
  const adds = await dshopRequests(page, "shop_helper_add_to_cart");
  expect(adds).toHaveLength(1);
  expect(adds[0].args).toEqual({ profileId: "fixture-profile", targetId: "target-1", goodsId: "1001" });

  // Event panel renders (no backend event in the mock, so it stays empty).
  await expect(page.getByTestId("helper-event")).toContainText("—");
});

test("auto popup starts, explains once with confirm, and stops with confirm", async ({ page }) => {
  await openProfileEditor(page);
  await gotoSection(page, "Auto popup");
  await expect(page.getByText("Auto popup (rotating explain)").first()).toBeVisible();

  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(page.getByText("Auto popup started.").first()).toBeVisible();
  const starts = await dshopRequests(page, "auto_popup_start");
  expect(starts).toHaveLength(1);
  expect(starts[0].args.profileId).toBe("fixture-profile");
  expect(starts[0].args.config.interval).toEqual([30, 60]);

  // Single explain requires confirm.
  await page.getByText("Goods serial", { exact: true }).locator("..").locator("input").fill("001");
  await page.getByRole("button", { name: "Explain once", exact: true }).click();
  await expect(page.getByText("clicks the explain button", { exact: false }).first()).toBeVisible();
  expect(await dshopRequests(page, "auto_popup_explain_once")).toEqual([]);
  await page.getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(page.getByText("Explained goods 001.").first()).toBeVisible();
  expect(await dshopRequests(page, "auto_popup_explain_once")).toHaveLength(1);

  // Stop requires confirm.
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  expect(await dshopRequests(page, "auto_popup_stop")).toEqual([]);
  await page.getByRole("button", { name: "Confirm", exact: true }).click();
  await expect(page.getByText("Auto popup stopped.").first()).toBeVisible();
  expect(await dshopRequests(page, "auto_popup_stop")).toHaveLength(1);
});

test("auto popup goods, scan, and shortcuts round-trip", async ({ page }) => {
  await openProfileEditor(page);
  await gotoSection(page, "Auto popup");

  await page.getByTestId("popup-status").getByRole("button", { name: "Refresh status", exact: true }).click();
  expect(await dshopRequests(page, "auto_popup_status")).toHaveLength(1);

  await page.getByTestId("popup-goods").getByRole("button", { name: "Refresh status", exact: true }).click();
  await expect(page.getByText("001 · 苹果 · 9.9")).toBeVisible();
  await page.getByTestId("popup-goods").getByRole("button", { name: "Scan knowledge", exact: true }).click();
  await expect(page.getByText("No diffs. 1 candidates scanned.").first()).toBeVisible();

  await page.getByRole("button", { name: "Register shortcuts", exact: true }).click();
  const regs = await dshopRequests(page, "auto_popup_register_shortcuts");
  expect(regs).toHaveLength(1);
  expect(regs[0].args.bindings).toEqual({ "CommandOrControl+1": "001" });
});
