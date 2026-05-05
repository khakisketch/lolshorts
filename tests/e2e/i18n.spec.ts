import { test, expect, BASE_URL } from "./fixtures/tauri-fixture";
import { Page } from "@playwright/test";

/**
 * E2E Tests for Internationalization (i18n)
 *
 * Tests language switching using Radix Select component.
 * The LanguageSelector uses shadcn/Radix Select, so we interact via
 * role="combobox" trigger + role="option" items in the listbox.
 */

/**
 * Helper: Switch language using the Radix Select in settings page.
 * Assumes the page is already on the Settings page.
 */
async function switchLanguage(page: Page, nativeName: string) {
  // Click the Radix Select trigger to open the dropdown
  const trigger = page.locator("#language-select");
  await expect(trigger).toBeVisible({ timeout: 10000 });
  await trigger.click();

  // Wait for the listbox to appear
  await page.waitForSelector('[role="listbox"]', {
    state: "visible",
    timeout: 3000,
  });

  // Click the option matching the language name
  await page.locator('[role="option"]').filter({ hasText: nativeName }).click();

  // Wait for language to apply
  await page.waitForTimeout(500);
}

async function openSettings(page: Page) {
  await page.click('[data-testid="nav-settings"]');
  await expect(page.getByTestId("settings")).toBeVisible({ timeout: 10000 });
}

test.describe("Internationalization (i18n)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL, { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("dashboard")).toBeVisible({ timeout: 30000 });
  });

  test("should display language selector in Settings", async ({ page }) => {
    await openSettings(page);

    await expect(page.locator("#language-select")).toBeVisible({
      timeout: 10000,
    });
  });

  test("should switch from English to Korean", async ({ page }) => {
    await openSettings(page);

    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();

    await switchLanguage(page, "한국어");

    await expect(
      page.getByRole("heading", { name: "설정" }).first(),
    ).toBeVisible();
  });

  test("should switch from English to Japanese", async ({ page }) => {
    await openSettings(page);

    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();

    await switchLanguage(page, "日本語");

    await expect(
      page.getByRole("heading", { name: "設定" }).first(),
    ).toBeVisible();
  });

  test("should persist language preference across page navigation", async ({
    page,
  }) => {
    await openSettings(page);

    await switchLanguage(page, "한국어");
    await expect(
      page.getByRole("heading", { name: "설정" }).first(),
    ).toBeVisible();

    // Navigate to Dashboard
    await page.click('[data-testid="nav-dashboard"]');
    await expect(page.getByRole("heading", { name: "대시보드" })).toBeVisible();

    // Navigate to Games
    await page.click('[data-testid="nav-games"]');
    await expect(
      page.getByRole("heading", { name: "녹화된 게임" }).first(),
    ).toBeVisible({ timeout: 10000 });
  });

  test("should translate Dashboard page correctly", async ({ page }) => {
    await expect(
      page.getByRole("heading", { name: /Dashboard/i }),
    ).toBeVisible();

    // Switch to Korean
    await openSettings(page);
    await switchLanguage(page, "한국어");

    // Go back to Dashboard
    await page.click('[data-testid="nav-dashboard"]');

    await expect(page.getByText(/대시보드/).first()).toBeVisible();
  });

  test("should translate Settings page completely", async ({ page }) => {
    await openSettings(page);

    // Test English
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();

    // Switch to Korean
    await switchLanguage(page, "한국어");

    await expect(
      page.getByRole("heading", { name: "설정" }).first(),
    ).toBeVisible();
  });

  test("should handle all 3 languages on Settings page", async ({ page }) => {
    await openSettings(page);

    // Test English (default)
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();

    // Switch to Korean
    await switchLanguage(page, "한국어");
    await expect(
      page.getByRole("heading", { name: "설정" }).first(),
    ).toBeVisible();

    // Switch to Japanese
    await switchLanguage(page, "日本語");
    await expect(
      page.getByRole("heading", { name: "設定" }).first(),
    ).toBeVisible();

    // Switch back to English
    await switchLanguage(page, "English");
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  });

  test("should maintain language preference after page reload", async ({
    page,
  }) => {
    await openSettings(page);

    await switchLanguage(page, "한국어");
    await expect(
      page.getByRole("heading", { name: "설정" }).first(),
    ).toBeVisible();

    // Reload page
    await page.reload({ waitUntil: "domcontentloaded" });

    // Verify language is still Korean
    await expect(
      page.getByRole("heading", { name: "설정" }).first(),
    ).toBeVisible();
  });
});
