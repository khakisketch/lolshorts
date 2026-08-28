import AxeBuilder from '@axe-core/playwright';
import { test, expect, BASE_URL } from './fixtures/tauri-fixture';

const coreLocales = ['en', 'ko', 'ja', 'zh-CN', 'es', 'pt-BR'] as const;
const viewports = [
  { width: 800, height: 600 },
  { width: 1280, height: 800 },
  { width: 3440, height: 1440 },
] as const;

async function expectNoHorizontalOverflow(page: import('@playwright/test').Page) {
  const overflow = await page.evaluate(() => {
    const root = document.documentElement;
    const offenders = Array.from(document.querySelectorAll<HTMLElement>('main, [data-testid="settings"]'))
      .filter((element) => element.scrollWidth > element.clientWidth + 1)
      .map((element) => ({
        testId: element.dataset.testid ?? element.tagName,
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
      }));
    return {
      page: { scrollWidth: root.scrollWidth, clientWidth: root.clientWidth },
      offenders,
    };
  });
  expect(overflow.page.scrollWidth).toBeLessThanOrEqual(overflow.page.clientWidth + 1);
  expect(overflow.offenders).toEqual([]);
}

test.describe('core locale responsive and accessibility matrix', () => {
  test('six core locales fit all supported Chromium viewports', async ({ page }, testInfo) => {
    test.skip(!testInfo.project.name.includes('Chrome'), 'The locale matrix is Chromium-only');

    for (const locale of coreLocales) {
      await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
      await page.evaluate((language) => localStorage.setItem('i18nextLng', language), locale);
      for (const viewport of viewports) {
        await page.setViewportSize(viewport);
        await page.goto(`${BASE_URL}/settings`, { waitUntil: 'domcontentloaded' });
        await expect(page.getByTestId('settings')).toBeVisible({ timeout: 30_000 });
        await page.getByTestId('settings-nav-app').click();
        await expect(page.getByTestId('check-app-update')).toBeVisible();
        await expectNoHorizontalOverflow(page);
      }

      await page.setViewportSize({ width: 1280, height: 800 });
      const results = await new AxeBuilder({ page })
        .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
        .analyze();
      expect(
        results.violations.filter(
          (violation) => violation.impact === 'serious' || violation.impact === 'critical',
        ),
      ).toEqual([]);
    }
  });

  test('updater can be checked, deferred, and returns focus using only the keyboard', async ({
    page,
  }, testInfo) => {
    test.skip(!testInfo.project.name.includes('Chrome'), 'Covered once in Chromium');
    await page.addInitScript(() => {
      window.__TEST_UPDATE_STATE__ = {
        status: 'idle',
        current_version: '1.2.0',
        available_version: '1.3.0',
        notes: 'Accessibility and updater reliability improvements',
        published_at: '2026-08-10T00:00:00Z',
        progress_percentage: 0,
        error_code: null,
      };
    });
    await page.goto(`${BASE_URL}/settings`, { waitUntil: 'domcontentloaded' });
    await expect(page.getByTestId('settings')).toBeVisible({ timeout: 30_000 });
    await page.getByTestId('settings-nav-app').click();

    const checkButton = page.getByTestId('check-app-update');
    await checkButton.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByTestId('app-update-dialog');
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole('button', { name: /Install update/i })).toBeFocused();

    const results = await new AxeBuilder({ page }).include('[data-testid="app-update-dialog"]').analyze();
    expect(
      results.violations.filter(
        (violation) => violation.impact === 'serious' || violation.impact === 'critical',
      ),
    ).toEqual([]);

    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(checkButton).toBeFocused();
  });
});
