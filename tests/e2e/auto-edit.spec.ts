import { test, expect, BASE_URL, loginAsFreeUser } from './fixtures/tauri-fixture';
import { Page } from '@playwright/test';

/**
 * Auto-Edit E2E Tests
 *
 * Tests what is verifiable in the mock environment:
 * - Page loads with FREE quota access
 * - UI elements are visible and interactive
 * - Tab navigation works
 * - Generate button is disabled when no games selected (mock returns [])
 *
 * Note: Full workflow tests (game selection -> generation -> result) require
 * real games in the environment. In mock mode, get_all_games returns [].
 */

test.setTimeout(120000);

async function navigateToAutoEdit(page: Page) {
  await page.goto(`${BASE_URL}/auto-edit`, { waitUntil: 'commit' });
  await loginAsFreeUser(page);
  await page.reload({ waitUntil: 'commit' });
  await expect(page.getByRole('heading', { name: 'Auto-Edit' })).toBeVisible({ timeout: 90000 });
  await page.locator('[data-testid="advanced-settings-button"]').click();
}

test.beforeAll(async ({ request }) => {
  await request.get(`${BASE_URL}/auto-edit`, { timeout: 60000 });
});

test.describe('Auto-Edit Configuration', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAutoEdit(page);
  });

  test('page loads with FREE quota access and shows auto-edit content', async ({ page }) => {
    // Verify we landed on the auto-edit page and it rendered content
    await expect(page.getByRole('heading', { name: 'Auto-Edit' })).toBeVisible();
  });

  test('shows no games alert when game list is empty', async ({ page }) => {
    // Mock returns no games, so alert should be shown instead of grid
    await expect(page.locator('[data-testid="quick-create"]').getByRole('alert').filter({ hasText: /No games available/i })).toBeVisible();
  });

  test('duration button 60s is visible', async ({ page }) => {
    await expect(page.locator('[data-testid="duration-60"]')).toBeVisible();
  });

  test('duration button 120s is visible', async ({ page }) => {
    await expect(page.locator('[data-testid="duration-120"]')).toBeVisible();
  });

  test('duration button 180s is visible', async ({ page }) => {
    await expect(page.locator('[data-testid="duration-180"]')).toBeVisible();
  });

  test('clicking duration-60 button shows selected state', async ({ page }) => {
    const btn = page.locator('[data-testid="duration-60"]');
    await btn.click();
    // Radix/shadcn buttons mark active state via aria-pressed, data-state, or class
    const isActive =
      (await btn.getAttribute('aria-pressed')) === 'true' ||
      (await btn.getAttribute('data-state')) === 'on' ||
      (await btn.evaluate((el) => el.className.includes('active') || el.className.includes('primary') || el.className.includes('selected')));
    expect(isActive).toBe(true);
  });

  test('clicking duration-120 button does not throw', async ({ page }) => {
    const btn = page.locator('[data-testid="duration-120"]');
    await btn.click();
    // Just verifying the click succeeds and page stays stable
    await expect(btn).toBeVisible();
  });

  test('canvas tab exists and is clickable', async ({ page }) => {
    const tab = page.locator('[data-testid="canvas-tab"]');
    await expect(tab).toBeVisible();
    await tab.click();
    // After clicking, the canvas tab should be selected (data-state="active" for Radix Tabs)
    await expect(tab).toHaveAttribute('data-state', 'active');
  });

  test('audio tab exists and is clickable', async ({ page }) => {
    const tab = page.locator('[data-testid="audio-tab"]');
    await expect(tab).toBeVisible();
    await tab.click();
    await expect(tab).toHaveAttribute('data-state', 'active');
  });

  test('generate button exists and is disabled when no games are selected', async ({ page }) => {
    // Mock returns no games, so generate should always be disabled
    await expect(page.locator('[data-testid="generate-button"]')).toBeDisabled();
  });
});

test.describe('Auto-Edit Error Handling', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToAutoEdit(page);
  });

  test('generate button is disabled when no games selected (mock environment)', async ({ page }) => {
    // This is the primary guard: with empty game list, generate must be disabled
    const generateButton = page.locator('[data-testid="generate-button"]');
    await expect(generateButton).toBeVisible();
    await expect(generateButton).toBeDisabled();
  });

  test('error section elements are not visible in initial state (no error)', async ({ page }) => {
    // In initial state without generation, error section should not be visible
    const errorSection = page.locator('[data-testid="error-section"]');
    const isVisible = await errorSection.isVisible({ timeout: 1000 }).catch(() => false);
    expect(isVisible).toBe(false);

    // Page should still be stable - generate button exists
    await expect(page.locator('[data-testid="generate-button"]')).toBeVisible();
  });

  test('tabs are accessible and can be switched back and forth', async ({ page }) => {
    const canvasTab = page.locator('[data-testid="canvas-tab"]');
    const audioTab = page.locator('[data-testid="audio-tab"]');

    // Click canvas tab
    await canvasTab.click();
    await expect(canvasTab).toHaveAttribute('data-state', 'active');

    // Switch to audio tab
    await audioTab.click();
    await expect(audioTab).toHaveAttribute('data-state', 'active');

    // Switch back to canvas tab
    await canvasTab.click();
    await expect(canvasTab).toHaveAttribute('data-state', 'active');
  });
});
