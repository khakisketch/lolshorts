import { test, expect, BASE_URL } from "./fixtures/tauri-fixture";

test.describe("Clip vault montage UX", () => {
  test("fixed selection bar leaves the final card reachable at target viewport sizes", async ({
    page,
  }) => {
    for (const viewport of [
      { width: 1280, height: 800 },
      { width: 3440, height: 1440 },
    ]) {
      await page.setViewportSize(viewport);
      await page.goto(`${BASE_URL}/results?tab=clips`, {
        waitUntil: "domcontentloaded",
      });
      const firstGame = page
        .locator('[data-testid^="clip-vault-disclosure-"]')
        .first();
      await expect(firstGame).toHaveAttribute("aria-expanded", "false");
      await firstGame.click();
      await expect(firstGame).toHaveAttribute("aria-expanded", "true");
      const checkboxes = page.getByRole("checkbox", { name: /Select clip/i });
      await expect(checkboxes.first()).toBeVisible({ timeout: 30000 });
      await checkboxes.first().check();
      await checkboxes.last().check();

      const actionBar = page.getByTestId("clip-vault-action-bar");
      await expect(actionBar).toBeVisible();
      await page.evaluate(() =>
        window.scrollTo(0, document.documentElement.scrollHeight),
      );
      const [barBox, lastCardBox] = await Promise.all([
        actionBar.boundingBox(),
        page.locator("article").last().boundingBox(),
      ]);
      expect(barBox).not.toBeNull();
      expect(lastCardBox).not.toBeNull();
      expect(
        (lastCardBox?.y ?? 0) + (lastCardBox?.height ?? 0),
      ).toBeLessThanOrEqual(barBox?.y ?? 0);
    }
  });

  test("keeps game groups collapsed until the user expands them", async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/results?tab=clips`, {
      waitUntil: "domcontentloaded",
    });

    const disclosures = page.locator('[data-testid^="clip-vault-disclosure-"]');
    await expect(disclosures).toHaveCount(6);
    await expect(disclosures.first()).toHaveAttribute("aria-expanded", "false");
    await expect(page.locator('[data-testid^="clip-vault-card-"]')).toHaveCount(
      0,
    );

    await disclosures.first().focus();
    await page.keyboard.press("Enter");
    await expect(disclosures.first()).toHaveAttribute("aria-expanded", "true");
    await expect(page.locator('[data-testid^="clip-vault-card-"]')).toHaveCount(
      3,
    );

    await page.keyboard.press("Space");
    await expect(disclosures.first()).toHaveAttribute("aria-expanded", "false");
    await expect(page.locator('[data-testid^="clip-vault-card-"]')).toHaveCount(
      0,
    );
  });

  test("tab URL survives reload and browser history", async ({ page }) => {
    await page.goto(`${BASE_URL}/results?tab=clips`, {
      waitUntil: "domcontentloaded",
    });
    const highlights = page.getByTestId("results-tab-highlights");
    await highlights.click();
    await expect(page).toHaveURL(/\/results\?tab=highlights$/);
    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(highlights).toHaveAttribute("data-state", "active");

    const clips = page.getByTestId("results-tab-clips");
    await clips.click();
    await expect(page).toHaveURL(/\/results\?tab=clips$/);
    await page.goBack({ waitUntil: "domcontentloaded" });
    await expect(page).toHaveURL(/\/results\?tab=highlights$/);
    await expect(page.getByTestId("results-tab-highlights")).toHaveAttribute(
      "data-state",
      "active",
    );
  });
});
