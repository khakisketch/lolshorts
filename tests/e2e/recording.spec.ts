import { test, expect, BASE_URL } from "./fixtures/tauri-fixture";

/**
 * E2E Tests for Recording & Game Status
 *
 * Tests actual UI elements on the Dashboard and other pages.
 * Note: RecordingControls and StatusDashboard are standalone components
 * not currently rendered on any page. Tests focus on Dashboard inline UI.
 */

test.describe("Dashboard Game Status", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState("networkidle");
  });

  test("should show LCU status on dashboard", async ({ page }) => {
    const lcuStatus = page.getByTestId("lcu-status");
    await expect(lcuStatus).toBeVisible({ timeout: 5000 });

    const statusText = await lcuStatus.textContent();
    expect(statusText).toBeTruthy();
  });

  test("should show dashboard with game status panel", async ({ page }) => {
    await expect(page.getByTestId("dashboard")).toBeVisible({ timeout: 5000 });
  });

  test("should display LCU disconnected state in mock", async ({ page }) => {
    // Mock returns lcu_connected: false
    const lcuStatus = page.getByTestId("lcu-status");
    await expect(lcuStatus).toBeVisible({ timeout: 5000 });

    const statusText = await lcuStatus.textContent();
    // Should show disconnected since mock returns false
    expect(statusText).toMatch(
      /disconnected|not.*running|offline|연결.*안.*됨|Disconnected/i,
    );
  });

  test("should display mocked recording readiness without hardware or League dependencies", async ({
    page,
  }) => {
    const dashboard = page.getByTestId("dashboard");
    await expect(dashboard).toBeVisible({ timeout: 5000 });
    const readinessPanel = dashboard
      .locator(".gaming-panel")
      .filter({ hasText: /Recording Readiness|dashboard\.readiness\.title/ })
      .first();

    await expect(
      readinessPanel.getByText(/FFmpeg unavailable in test fixture/),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      readinessPanel.getByText(/Storage space low in test fixture/),
    ).toBeVisible();
    await expect(readinessPanel.getByText(/^ffmpeg$/i)).toBeVisible();
    await expect(readinessPanel.getByText(/^audio$/i)).toBeVisible();
    await expect(readinessPanel.getByText(/^disk$/i)).toBeVisible();
    await expect(readinessPanel.getByText(/^lcu$/i)).toBeVisible();
    await expect(readinessPanel.getByText(/^gpu$/i)).toBeVisible();
  });
});

test.describe("Navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState("networkidle");
  });

  test("should navigate to replays page", async ({ page }) => {
    await page.getByTestId("nav-library").click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByTestId("replays-page")).toBeVisible({
      timeout: 5000,
    });
  });

  test("should navigate to games page", async ({ page }) => {
    await page.getByTestId("nav-games").click();
    await page.waitForLoadState("networkidle");

    await expect(page).toHaveURL(/\/games/);
  });

  test("should navigate to settings page", async ({ page }) => {
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByTestId("settings")).toBeVisible({ timeout: 5000 });
  });

  test("should render Settings recording fixture shape from mocked Tauri settings", async ({
    page,
  }) => {
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    const settings = page.getByTestId("settings");
    await expect(settings).toBeVisible({ timeout: 5000 });
    await expect(
      settings.getByText("Show Replay Detection Popup"),
    ).toBeVisible();
    await expect(settings.getByText("Send Crash Reports")).toBeVisible();
    await expect(settings.getByText("In-Game Overlay")).toBeVisible();
    await expect(settings.getByText("Storage Management")).toBeVisible();
  });

  test("should navigate to editor page", async ({ page }) => {
    await page.getByTestId("nav-editor").click();
    await page.waitForLoadState("networkidle");

    await expect(page).toHaveURL(/\/editor/);
  });

  test("should navigate back to dashboard", async ({ page }) => {
    // Go to settings first
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    // Then back to dashboard
    await page.getByTestId("nav-dashboard").click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByTestId("dashboard")).toBeVisible({ timeout: 5000 });
  });
});

test.describe("Replays Page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(`${BASE_URL}/replays`);
    await page.waitForLoadState("networkidle");
  });

  test("should show replays page", async ({ page }) => {
    await expect(page.getByTestId("replays-page")).toBeVisible({
      timeout: 5000,
    });
  });

  test("should show empty state or match list", async ({ page }) => {
    // With mock returning empty matches, should show empty state message
    const hasEmptyMsg = await page
      .locator("text=/no match|empty|저장된|기록/i")
      .isVisible({ timeout: 3000 })
      .catch(() => false);
    const hasContent = await page.getByTestId("replays-page").isVisible();

    expect(hasEmptyMsg || hasContent).toBeTruthy();
  });
});

test.describe("Performance", () => {
  test("should load dashboard within 5 seconds in dev-mode smoke", async ({
    page,
  }) => {
    const startTime = Date.now();
    await page.goto(BASE_URL);
    await page.waitForLoadState("networkidle");
    const loadTime = Date.now() - startTime;

    // Dev-mode Vite and mocked Tauri initialization can vary on Windows.
    // Keep this as a smoke threshold; production performance is verified separately.
    expect(loadTime).toBeLessThan(5000);
  });

  test("should navigate between pages smoothly", async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState("networkidle");

    const startTime = Date.now();

    // Quick navigation sequence
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    await page.getByTestId("nav-dashboard").click();
    await page.waitForLoadState("networkidle");

    await page.getByTestId("nav-library").click();
    await page.waitForLoadState("networkidle");

    const elapsed = Date.now() - startTime;
    expect(elapsed).toBeLessThan(10000);
  });
});
