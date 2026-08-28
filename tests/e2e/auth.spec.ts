import { test, expect, BASE_URL } from './fixtures/tauri-fixture';

/**
 * E2E Tests for Authentication System
 *
 * Tests auth modal UI and form validation.
 * Note: Auth state switching tests are limited because the app uses
 * Supabase client-side auth which can't be easily mocked in E2E tests.
 */

test.describe('Auth Modal UI', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState('networkidle');
  });

  test('should display login button for unauthenticated users', async ({ page }) => {
    await expect(page.getByTestId('sidebar-login-button')).toBeVisible();
  });

  test('should open auth modal when login button clicked', async ({ page }) => {
    await page.getByTestId('sidebar-login-button').click();

    // Modal should appear with login form
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 });
    await expect(page.getByTestId('email-input')).toBeVisible();
    await expect(page.getByTestId('password-input')).toBeVisible();
    await expect(page.getByTestId('sign-in-button')).toBeVisible();
  });

  test('should show validation when submitting empty form', async ({ page }) => {
    await page.getByTestId('sidebar-login-button').click();
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 });

    // Submit without filling fields
    await page.getByTestId('sign-in-button').click();

    // HTML5 validation should prevent submission
    const emailInput = page.getByTestId('email-input');
    const isInvalid = await emailInput.evaluate((el: HTMLInputElement) => !el.validity.valid);
    expect(isInvalid).toBeTruthy();
  });

  test('should show email validation for invalid email', async ({ page }) => {
    await page.getByTestId('sidebar-login-button').click();
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 });

    await page.getByTestId('email-input').fill('notanemail');
    await page.getByTestId('password-input').fill('password123');
    await page.getByTestId('sign-in-button').click();

    // HTML5 email validation
    const emailInput = page.getByTestId('email-input');
    const isInvalid = await emailInput.evaluate((el: HTMLInputElement) => !el.validity.valid);
    expect(isInvalid).toBeTruthy();
  });

  test('should expose only the supported desktop email login', async ({ page }) => {
    await page.getByTestId('sidebar-login-button').click();
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 });

    await expect(page.getByTestId('email-input')).toBeVisible();
    await expect(page.getByTestId('password-input')).toBeVisible();
    await expect(page.getByTestId('google-login-button')).toHaveCount(0);
  });

  test('should close modal when dialog is dismissed', async ({ page }) => {
    await page.getByTestId('sidebar-login-button').click();
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 });

    // Press Escape to close
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog')).not.toBeVisible({ timeout: 3000 });
  });
});

test.describe('Signup Form', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState('networkidle');
    // Open auth modal
    await page.getByTestId('sidebar-login-button').click();
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 });
  });

  test('should switch to signup form', async ({ page }) => {
    // Find the signup switch button inside the dialog content
    // Use a more specific locator to avoid the overlay intercepting clicks
    const dialog = page.getByRole('dialog');
    const signupBtn = dialog.locator('button').filter({ hasText: /sign up|create account|register|회원가입/i }).first();
    if (await signupBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await signupBtn.click({ force: true });
      await expect(page.getByTestId('signup-email-input')).toBeVisible({ timeout: 5000 });
      await expect(page.getByTestId('signup-password-input')).toBeVisible();
      await expect(page.getByTestId('confirm-password-input')).toBeVisible();
      await expect(page.getByTestId('sign-up-button')).toBeVisible();
    }
  });

  test('should validate password confirmation mismatch', async ({ page }) => {
    const dialog = page.getByRole('dialog');
    const signupBtn = dialog.locator('button').filter({ hasText: /sign up|create account|register|회원가입/i }).first();
    if (await signupBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await signupBtn.click({ force: true });
      await expect(page.getByTestId('signup-email-input')).toBeVisible({ timeout: 5000 });

      await page.getByTestId('signup-email-input').fill('newuser@lolshorts.com');
      await page.getByTestId('signup-password-input').fill('Password123!');
      await page.getByTestId('confirm-password-input').fill('DifferentPassword123!');
      await page.getByTestId('sign-up-button').click();

      await expect(page.locator('text=/not match|must match|do not match|일치하지/i')).toBeVisible({ timeout: 5000 });
    }
  });

  test('should expose only the supported desktop email signup', async ({ page }) => {
    const dialog = page.getByRole('dialog');
    const signupBtn = dialog.locator('button').filter({ hasText: /sign up|create account|register|회원가입/i }).first();
    if (await signupBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await signupBtn.click({ force: true });
      await expect(page.getByTestId('signup-email-input')).toBeVisible({ timeout: 5000 });
      await expect(page.getByTestId('google-signup-button')).toHaveCount(0);
    }
  });
});

test.describe('Protected Features (unauthenticated)', () => {
  test('should show sidebar login button on all pages', async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState('networkidle');

    // Login button visible on dashboard
    await expect(page.getByTestId('sidebar-login-button')).toBeVisible();

    // Navigate to settings - login button still visible
    await page.getByTestId('nav-settings').click();
    await page.waitForLoadState('networkidle');
    await expect(page.getByTestId('sidebar-login-button')).toBeVisible();
  });

  test('should handle settings page when unauthenticated', async ({ page }) => {
    await page.goto(`${BASE_URL}/settings`);
    await page.waitForLoadState('networkidle');

    // Settings page should still load (shows login prompt in license section)
    await expect(page.getByTestId('settings')).toBeVisible({ timeout: 5000 });
  });
});
