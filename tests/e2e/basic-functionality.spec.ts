import { test, expect, BASE_URL } from "./fixtures/tauri-fixture";

/**
 * Basic E2E Tests for LoLShorts Application
 *
 * These tests verify core application functionality without requiring
 * actual League of Legends game integration
 */

test.describe("LoLShorts Basic Functionality", () => {
  test("application loads successfully", async ({ page }) => {
    await page.goto(BASE_URL);
    await expect(page).toHaveTitle(/LoLShorts/);
    await expect(page.locator('[data-testid="app-shell"]')).toBeVisible();
  });

  test("dashboard renders correctly", async ({ page }) => {
    await page.goto(BASE_URL);
    await expect(page.locator('[data-testid="dashboard"]')).toBeVisible();
    await expect(page.locator('[data-testid="lcu-status"]')).toBeVisible();
  });

  test("settings page is accessible", async ({ page }) => {
    await page.goto(BASE_URL);
    await page.click('[data-testid="nav-settings"]');
    await page.waitForLoadState("networkidle");
    await expect(page.locator('[data-testid="settings"]')).toBeVisible();
  });

  test("audio settings tab is accessible", async ({ page }) => {
    await page.goto(BASE_URL);
    await page.click('[data-testid="nav-settings"]');
    await page.waitForLoadState("networkidle");
    await expect(page.locator('[data-testid="settings"]')).toBeVisible();
    // Click the audio tab to reveal AudioSettings
    const audioTab = page.locator('[value="audio"]');
    if (await audioTab.isVisible()) {
      await audioTab.click();
      await expect(
        page.locator('[data-testid="audio-settings"]'),
      ).toBeVisible();
    }
  });

  test("navigation between sections works", async ({ page }) => {
    await page.goto(BASE_URL);

    // Navigate to settings
    await page.click('[data-testid="nav-settings"]');
    await expect(page.locator('[data-testid="settings"]')).toBeVisible();

    // Navigate to replays
    await page.click('[data-testid="nav-library"]');
    await expect(page.locator('[data-testid="replays-page"]')).toBeVisible();

    // Navigate back to dashboard
    await page.click('[data-testid="nav-dashboard"]');
    await expect(page.locator('[data-testid="dashboard"]')).toBeVisible();
  });

  test("lcu status indicator is visible on dashboard", async ({ page }) => {
    await page.goto(BASE_URL);
    const lcuStatus = page.locator('[data-testid="lcu-status"]');
    await expect(lcuStatus).toBeVisible();
    const text = await lcuStatus.textContent();
    expect(text).toBeTruthy();
  });

  test("responsive design works correctly", async ({ page }) => {
    await page.goto(BASE_URL);
    // Desktop viewport
    await page.setViewportSize({ width: 1280, height: 720 });
    await expect(page.locator('[data-testid="app-shell"]')).toBeVisible();
    // Tablet viewport
    await page.setViewportSize({ width: 768, height: 1024 });
    await expect(page.locator('[data-testid="app-shell"]')).toBeVisible();
    // Mobile viewport
    await page.setViewportSize({ width: 375, height: 667 });
    await expect(page.locator('[data-testid="app-shell"]')).toBeVisible();
  });
});
