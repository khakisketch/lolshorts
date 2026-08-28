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
  // 언어 선택기는 「관리 > 앱」 칸에 있다(예전의 단일 "고급 설정" 토글은 없다 —
  // 그 구조가 각 칸을 세 화면분으로 만들어 칸별 접힘으로 바뀌었다).
  await page.getByTestId("settings-nav-app").click();
  await expect(page.locator("#language-select")).toBeVisible({
    timeout: 10000,
  });
}

test.describe("Internationalization (i18n)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL, { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home")).toBeVisible({ timeout: 30000 });
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

    // 홈으로 — 화면 이름이 "대시보드" 에서 홈으로 바뀌었고, 제목은 그 화면이
    // 무엇을 하는지 말한다. 판 메타데이터를 못 얻으면 이 제목으로 떨어진다
    // (얻으면 챔피언 이름이 제목이 된다 — `GameSummary`).
    await page.click('[data-testid="nav-dashboard"]');
    await expect(
      page.getByRole("heading", { name: "이번 판 하이라이트" }),
    ).toBeVisible({ timeout: 10000 });

    // 결과로 — `nav-games` 는 사라졌다(가진 것은 모두 결과에 있다).
    await page.click('[data-testid="nav-library"]');
    await expect(page).toHaveURL(/\/results/);
  });

  test("should translate the home page correctly", async ({ page }) => {
    await expect(
      page.getByRole("heading", { name: /This game's highlights/i }),
    ).toBeVisible();

    await openSettings(page);
    await switchLanguage(page, "한국어");

    await page.click('[data-testid="nav-dashboard"]');

    await expect(page.getByText(/이번 판 하이라이트/).first()).toBeVisible();
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
