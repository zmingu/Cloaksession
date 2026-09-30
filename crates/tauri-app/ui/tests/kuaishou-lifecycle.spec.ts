import { expect, test, type Page } from "@playwright/test";
import { FIRST_PROFILE } from "./businessAccountsMock";
import { holdIdentity, identityFixture, installIdentityMock, refreshIdentity, releaseIdentity, waitIdentity } from "./kuaishouIdentityMock";

const summary = (page: Page) => page.getByTestId(`kuaishou-summary-${FIRST_PROFILE}`);
async function changeRunning(page: Page, running: boolean) {
  await page.evaluate(({ id, running }) => { (window as any).__TEST_BUSINESS__.running = running ? [id] : []; }, { id: FIRST_PROFILE, running });
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
  await expect.poll(() => summary(page).getByRole("button", { name: "重新检测", exact: true }).isEnabled()).toBe(running);
}

test("reopening cannot promote a prior run's detected snapshot, including an unchanged list result", async ({ page }) => {
  await installIdentityMock(page);
  await expect(summary(page)).toHaveAttribute("data-status", "detected");
  await changeRunning(page, false);
  await expect(summary(page)).toHaveAttribute("data-status", "closed");
  await changeRunning(page, true);
  await expect(summary(page)).toHaveAttribute("data-status", "unknown");
  await expect(summary(page)).toContainText("上次识别");
  await refreshIdentity(page);
  await expect(summary(page)).toHaveAttribute("data-status", "unknown");
  // Compare backend observation markers for equality, never to the local clock.
  await page.evaluate((snapshot) => { (window as any).__TEST_IDENTITY__.snapshots = [snapshot]; }, identityFixture({ checkedAt: "2020-01-01T00:00:00Z", lastSeenAt: "2020-01-01T00:00:00Z" }));
  await refreshIdentity(page);
  await expect(summary(page)).toHaveAttribute("data-status", "detected");
});

test("a manual result started before a stop and reopen cannot restore a current green result", async ({ page }) => {
  await installIdentityMock(page);
  const key = `kuaishou_identity_detect:${FIRST_PROFILE}`;
  await holdIdentity(page, key);
  await summary(page).getByRole("button", { name: "重新检测", exact: true }).click();
  await waitIdentity(page, key);
  await page.evaluate(() => { (window as any).__TEST_BUSINESS__.running = []; });
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
  await page.evaluate((id) => { (window as any).__TEST_BUSINESS__.running = [id]; }, FIRST_PROFILE);
  await page.getByRole("button", { name: /Regression profile/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();
  await releaseIdentity(page, key);
  await expect(summary(page).getByRole("button", { name: "重新检测", exact: true })).toBeEnabled();
  await expect(summary(page)).toHaveAttribute("data-status", "unknown");
  await summary(page).getByRole("button", { name: "重新检测", exact: true }).click();
  await expect(summary(page)).toHaveAttribute("data-status", "detected");
});
