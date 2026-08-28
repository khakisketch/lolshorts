import { test, expect, BASE_URL } from './fixtures/tauri-fixture';

test('English core navigation remains usable in the secondary browser engines', async ({
  page,
}, testInfo) => {
  test.skip(testInfo.project.name.includes('Chrome'), 'Chromium has the full locale matrix');
  await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
  await expect(page.getByTestId('home')).toBeVisible({ timeout: 30_000 });

  await page.getByTestId('nav-library').click();
  await expect(page).toHaveURL(/\/results/);
  await expect(page.getByTestId('results-page')).toBeVisible();

  await page.getByTestId('nav-settings').click();
  await expect(page.getByTestId('settings')).toBeVisible();
  await page.getByTestId('settings-nav-app').click();
  await expect(page.getByTestId('check-app-update')).toBeVisible();

  const hasOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1,
  );
  expect(hasOverflow).toBe(false);
});
