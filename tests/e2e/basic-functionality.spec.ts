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

  test("home renders correctly", async ({ page }) => {
    await page.goto(BASE_URL);
    await expect(page.locator('[data-testid="home"]')).toBeVisible();
    await expect(page.locator('[data-testid="home-status"]')).toBeVisible();
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
    // 오디오 상세는 「소리」 칸의 접힌 「고급 설정」 안으로 내려갔다.
    await page.getByTestId("settings-nav-sound").click();
    // `<details>` 는 `<summary>` 를 눌러야 열린다 — details 자체를 클릭해도 안 열린다.
    await page.getByTestId("advanced-sound").locator("summary").click();
    await expect(page.locator('[data-testid="audio-settings"]')).toBeVisible();
  });

  test("navigation between sections works", async ({ page }) => {
    await page.goto(BASE_URL);

    // Navigate to settings
    await page.click('[data-testid="nav-settings"]');
    await expect(page.locator('[data-testid="settings"]')).toBeVisible();

    // Navigate to results
    await page.click('[data-testid="nav-library"]');
    await expect(page).toHaveURL(/\/results/);

    // Navigate back to home
    await page.click('[data-testid="nav-dashboard"]');
    await expect(page.locator('[data-testid="home"]')).toBeVisible();
  });

  test("connection status is visible on home", async ({ page }) => {
    await page.goto(BASE_URL);
    const status = page.locator('[data-testid="home-status"]');
    await expect(status).toBeVisible();
    expect(await status.textContent()).toBeTruthy();
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
