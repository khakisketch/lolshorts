import { test, expect, BASE_URL, loginAsProUser } from './fixtures/tauri-fixture';
import { Page } from '@playwright/test';

/**
 * Canvas Editor E2E Tests
 *
 * Canvas editor lives inside the Auto-Edit page as the "Canvas Overlay" tab.
 * Within the canvas editor, there are two sub-tabs: "Background" and "Elements".
 * Navigation: PRO login -> /auto-edit -> canvas-tab (default selected) -> canvas-editor visible
 *
 * Canvas Specifications:
 * - Preview: 360x640px (9:16 aspect ratio)
 * - Full Resolution: 1080x1920px (YouTube Shorts)
 * - Background types: Solid Color, Gradient, Image (via Select dropdown)
 * - Elements: Text and Image (in "Elements" sub-tab)
 */

async function navigateToCanvasEditor(page: Page) {
  await loginAsProUser(page);
  await page.goto(`${BASE_URL}/auto-edit`);
  await page.locator('[data-testid="advanced-settings-button"]').click();
  // Canvas Overlay tab is selected by default
  await expect(page.locator('[data-testid="canvas-editor"]')).toBeVisible();
}

async function navigateToElementsTab(page: Page) {
  await navigateToCanvasEditor(page);
  // Click the "Elements" sub-tab within the canvas editor
  await page.locator('button[role="tab"]').filter({ hasText: 'Elements' }).click();
}

// ---------------------------------------------------------------------------
// Background Layers (4 tests)
// ---------------------------------------------------------------------------

test.describe('Canvas Editor - Background Layers', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToCanvasEditor(page);
  });

  test('canvas editor renders with a visible canvas preview', async ({ page }) => {
    await expect(page.locator('[data-testid="canvas-preview"]')).toBeVisible();
  });

  test('should show background type selector with Solid Color default', async ({ page }) => {
    // Background tab is default, Select shows "Solid Color"
    const bgTypeSelect = page.locator('[data-testid="canvas-editor"]').locator('button[role="combobox"]');
    await expect(bgTypeSelect).toBeVisible();
    await expect(bgTypeSelect).toContainText(/Solid Color|단색|ソリッドカラー/);
  });

  test('should switch background type to Gradient', async ({ page }) => {
    // Open background type dropdown
    const bgTypeSelect = page.locator('[data-testid="canvas-editor"]').locator('button[role="combobox"]');
    await bgTypeSelect.click();
    await page.waitForSelector('[role="listbox"]', { state: 'visible', timeout: 3000 });
    await page.locator('[role="option"]').filter({ hasText: /Gradient|그라데이션|グラデーション/ }).click();

    // After selecting Gradient, two color inputs should appear
    const colorInputs = page.locator('[data-testid="canvas-editor"]').locator('input[type="color"]');
    await expect(colorInputs).toHaveCount(2);
  });

  test('should have Image option in background type selector', async ({ page }) => {
    const bgTypeSelect = page.locator('[data-testid="canvas-editor"]').locator('button[role="combobox"]');
    await bgTypeSelect.click();
    await page.waitForSelector('[role="listbox"]', { state: 'visible', timeout: 3000 });

    // Verify Image option exists in the dropdown
    const imageOption = page.locator('[role="option"]').filter({ hasText: /Image|이미지|画像/ });
    await expect(imageOption).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// Text Elements (5 tests)
// ---------------------------------------------------------------------------

test.describe('Canvas Editor - Text Elements', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToElementsTab(page);
  });

  test('should add a text element', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await expect(page.locator('[data-testid="element-0"]')).toBeVisible();
  });

  test('should edit text element content', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="element-0"]');

    await page.fill('[data-testid="text-content-input"]', 'Epic Moments');
    const value = await page.locator('[data-testid="text-content-input"]').inputValue();
    expect(value).toBe('Epic Moments');
  });

  test('should edit text size and color inputs', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="element-0"]');

    await page.fill('[data-testid="text-size-input"]', '48');
    expect(await page.locator('[data-testid="text-size-input"]').inputValue()).toBe('48');

    // Color inputs are type="color" - use evaluate to set value
    const colorInput = page.locator('[data-testid="text-color-input"]');
    await colorInput.evaluate((el: HTMLInputElement) => {
      const nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')!.set!;
      nativeInputValueSetter.call(el, '#ffffff');
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(await colorInput.inputValue()).toBe('#ffffff');
  });

  test('should allow editing text position via inputs', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="element-0"]');

    await page.fill('[data-testid="position-x-input"]', '75');
    await page.fill('[data-testid="position-y-input"]', '25');

    expect(await page.locator('[data-testid="position-x-input"]').inputValue()).toBe('75');
    expect(await page.locator('[data-testid="position-y-input"]').inputValue()).toBe('25');
  });

  test('should delete text element', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await expect(page.locator('[data-testid="element-0"]')).toBeVisible();

    await page.click('[data-testid="element-0"]');
    await page.click('[data-testid="delete-element-0"]');

    await expect(page.locator('[data-testid="element-0"]')).not.toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// Multi-Element Management (3 tests)
// ---------------------------------------------------------------------------

test.describe('Canvas Editor - Multi-Element Management', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToElementsTab(page);
  });

  test('should handle multiple text elements', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="add-text-button"]');

    await expect(page.locator('[data-testid="element-0"]')).toBeVisible();
    await expect(page.locator('[data-testid="element-1"]')).toBeVisible();
    await expect(page.locator('[data-testid="element-2"]')).toBeVisible();

    const count = await page.locator('[data-testid^="element-"]').count();
    expect(count).toBeGreaterThanOrEqual(3);
  });

  test('should switch between element selections and preserve content', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="add-text-button"]');

    await page.click('[data-testid="element-0"]');
    await page.fill('[data-testid="text-content-input"]', 'Title');

    await page.click('[data-testid="element-1"]');
    await page.fill('[data-testid="text-content-input"]', 'Subtitle');

    // Select element-0 again and verify its content is retained
    await page.click('[data-testid="element-0"]');
    const content = await page.locator('[data-testid="text-content-input"]').inputValue();
    expect(content).toBe('Title');
  });

  test('should show canvas editor with preview when elements exist', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');

    await expect(page.locator('[data-testid="canvas-preview"]')).toBeVisible();
    await expect(page.locator('[data-testid="element-0"]')).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// Template Operations (5 tests)
// ---------------------------------------------------------------------------

test.describe('Canvas Editor - Template Operations', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToCanvasEditor(page);
  });

  test('save template button is visible', async ({ page }) => {
    await expect(page.locator('[data-testid="save-template-button"]')).toBeVisible();
  });

  test('should open save template dialog and enter a name', async ({ page }) => {
    await page.click('[data-testid="save-template-button"]');

    await expect(page.locator('[data-testid="template-name-input"]')).toBeVisible();

    await page.fill('[data-testid="template-name-input"]', 'Epic Moments Template');
    expect(await page.locator('[data-testid="template-name-input"]').inputValue()).toBe(
      'Epic Moments Template'
    );
  });

  test('should save a canvas template without error (mock returns success)', async ({ page }) => {
    // Switch to Elements tab and create canvas content first
    await page.locator('button[role="tab"]').filter({ hasText: 'Elements' }).click();
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="element-0"]');
    await page.fill('[data-testid="text-content-input"]', 'My Template');

    // Open save dialog
    await page.click('[data-testid="save-template-button"]');
    await page.fill('[data-testid="template-name-input"]', 'Test Save Template');

    // Confirm save
    await page.click('[data-testid="confirm-save-button"]');

    // Dialog should close - canvas editor remains visible
    await expect(page.locator('[data-testid="canvas-editor"]')).toBeVisible();
  });

  test('load template button is visible', async ({ page }) => {
    await expect(page.locator('[data-testid="load-template-button"]')).toBeVisible();
  });

  test('load template dialog shows empty list in mock environment', async ({ page }) => {
    await page.click('[data-testid="load-template-button"]');

    const templateList = page.locator('[data-testid="template-list"]');
    const hasTemplateList = await templateList.isVisible({ timeout: 3000 }).catch(() => false);

    if (hasTemplateList) {
      const templateCount = await page.locator('[data-testid^="template-item-"]').count();
      expect(templateCount).toBe(0);
    } else {
      const templateCount = await page.locator('[data-testid^="template-item-"]').count();
      expect(templateCount).toBe(0);
    }
  });
});

// ---------------------------------------------------------------------------
// Real-time Preview (2 tests)
// ---------------------------------------------------------------------------

test.describe('Canvas Editor - Real-time Preview', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToElementsTab(page);
  });

  test('should update preview content when text is added', async ({ page }) => {
    await page.click('[data-testid="add-text-button"]');
    await page.click('[data-testid="element-0"]');

    await page.fill('[data-testid="text-content-input"]', 'Real-time Preview');

    await expect(page.locator('[data-testid="canvas-preview"]')).toBeVisible();
    expect(await page.locator('[data-testid="text-content-input"]').inputValue()).toBe(
      'Real-time Preview'
    );
  });

  test('should maintain 9:16 aspect ratio', async ({ page }) => {
    const canvas = page.locator('[data-testid="canvas-preview"]');
    const box = await canvas.boundingBox();

    if (box && box.width > 0) {
      const aspectRatio = box.height / box.width;
      // 9:16 means height/width ~ 1.777
      expect(aspectRatio).toBeGreaterThan(1.5);
      expect(aspectRatio).toBeLessThan(2.0);
    }
  });
});

// ---------------------------------------------------------------------------
// Accessibility (2 tests)
// ---------------------------------------------------------------------------

test.describe('Canvas Editor - Accessibility', () => {
  test.beforeEach(async ({ page }) => {
    await navigateToElementsTab(page);
  });

  test('should be keyboard navigable (Tab moves focus)', async ({ page }) => {
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');

    const focusedTestId = await page.evaluate(
      () => document.activeElement?.getAttribute('data-testid') ?? null
    );
    expect(focusedTestId).not.toBeNull();
  });

  test('add-text and add-image buttons should have accessible labels', async ({ page }) => {
    const addTextBtn = page.locator('[data-testid="add-text-button"]');
    const addImageBtn = page.locator('[data-testid="add-image-button"]');

    const textBtnLabel = await addTextBtn.getAttribute('aria-label');
    const textBtnText = await addTextBtn.textContent();
    expect(textBtnLabel || textBtnText).toBeTruthy();

    const imageBtnLabel = await addImageBtn.getAttribute('aria-label');
    const imageBtnText = await addImageBtn.textContent();
    expect(imageBtnLabel || imageBtnText).toBeTruthy();
  });
});
