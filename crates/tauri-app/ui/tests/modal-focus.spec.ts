import { expect, test } from "@playwright/test";
import { installBusinessMock } from "./businessAccountsMock";
import { profilePatches } from "./tauriMock";

test("initial modal focus must not steal a textarea selected before the delayed autofocus", async ({ page }) => {
  await page.clock.install({ time: new Date("2026-09-30T00:00:00Z") });
  await page.clock.pauseAt(new Date("2026-09-30T00:00:01Z"));
  await installBusinessMock(page);
  await page.getByRole("button", { name: /Regression profile/ }).evaluate((button: HTMLButtonElement) => button.click());
  const dialog = page.getByRole("dialog");
  const name = dialog.locator("input[data-autofocus]");
  const notes = dialog.locator("textarea");
  await name.fill("");
  await notes.fill("");
  // Deterministically cross the Modal's 60 ms initial-focus timer after the user chose Notes.
  await page.clock.runFor(80);
  await expect(notes).toBeFocused();
  await page.keyboard.insertText("未保存的环境备注");
  await page.clock.runFor(750);
  await expect(name).toHaveValue("");
  await expect(notes).toHaveValue("未保存的环境备注");
  expect(await profilePatches(page)).toEqual([]);
});
