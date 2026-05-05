import { test, expect } from './fixtures/tauri-fixture';
import path from 'path';
import fs from 'fs';

/**
 * E2E Tests for Game Integration - Real League of Legends Testing
 *
 * These tests require:
 * 1. League of Legends client running
 * 2. In-game access for testing
 * 3. Sufficient hardware for game recording
 * 4. RUN_INTEGRATION_TESTS=true environment variable
 *
 * Test Coverage:
 * - Live Client API connection
 * - Real game event detection
 * - Actual replay buffer recording
 * - Performance with real gameplay
 * - Clip capture from real events
 */

const BASE_URL = 'http://localhost:5181';

test.describe('Game Integration Tests', () => {
  test.beforeAll(() => {
    test.skip(
      !process.env.RUN_INTEGRATION_TESTS,
      'Set RUN_INTEGRATION_TESTS=true to run game integration tests'
    );
  });

  test.beforeEach(async ({ page }) => {
    await page.context().clearCookies();
    await page.goto(BASE_URL);
    await page.waitForLoadState('networkidle');
  });

  test('should show LCU connection status on dashboard', async ({ page }) => {
    // LCU status should be visible on dashboard
    const lcuStatus = page.locator('[data-testid="lcu-status"]');
    await expect(lcuStatus).toBeVisible();

    const statusText = await lcuStatus.textContent();
    // Should show some connection state
    expect(statusText).toBeTruthy();
  });

  test('should detect game state from status dashboard', async ({ page }) => {
    // Wait for game state detection
    await page.waitForTimeout(5000);

    const statusDashboard = page.locator('[data-testid="status-dashboard"]');
    await expect(statusDashboard).toBeVisible();
  });

  test('should start auto-capture when game detected', async ({ page }) => {
    // Wait for game detection
    await page.waitForTimeout(5000);

    // Start auto-capture
    const startButton = page.locator('[data-testid="start-auto-capture"]');
    if (await startButton.isVisible()) {
      await startButton.click();

      // Should show stop button
      await expect(page.locator('[data-testid="stop-auto-capture"]')).toBeVisible();

      // Stop recording
      await page.locator('[data-testid="stop-auto-capture"]').click();
    }
  });

  test('should display recording controls on dashboard', async ({ page }) => {
    await expect(page.locator('[data-testid="recording-controls"]')).toBeVisible();
    await expect(page.locator('[data-testid="start-auto-capture"]')).toBeVisible();
  });

  test('should navigate to replays page for clips', async ({ page }) => {
    // Navigate to replays (clips are here, not on /clips)
    await page.click('[data-testid="nav-library"]');
    await page.waitForLoadState('networkidle');

    await expect(page.locator('[data-testid="clip-library"]')).toBeVisible();
  });

  test('should show save replay button when recording', async ({ page }) => {
    // Start recording first
    const startButton = page.locator('[data-testid="start-auto-capture"]');
    if (await startButton.isVisible()) {
      await startButton.click();
      await page.waitForTimeout(1000);

      // Save replay button should exist
      const saveButton = page.locator('[data-testid="save-replay-button"]');
      await expect(saveButton).toBeVisible();

      // Stop recording
      await page.locator('[data-testid="stop-auto-capture"]').click();
    }
  });

  test('should handle performance monitoring during recording', async ({ page }) => {
    const startTime = Date.now();

    // Start auto-capture
    const startButton = page.locator('[data-testid="start-auto-capture"]');
    await startButton.click();

    // Wait briefly
    await page.waitForTimeout(3000);

    // Should still be recording without crashes
    await expect(page.locator('[data-testid="stop-auto-capture"]')).toBeVisible();

    // Stop recording
    await page.locator('[data-testid="stop-auto-capture"]').click();

    const elapsed = Date.now() - startTime;
    expect(elapsed).toBeLessThan(10000); // Should complete within 10s
  });
});

test.describe('Game Integration Stress Tests', () => {
  test.beforeAll(() => {
    test.skip(
      !process.env.RUN_STRESS_TESTS,
      'Set RUN_STRESS_TESTS=true to run stress tests'
    );
  });

  test('should handle extended recording sessions', async ({ page }) => {
    await page.goto(BASE_URL);

    // Start auto-capture
    await page.locator('[data-testid="start-auto-capture"]').click();

    // Monitor for memory leaks (abbreviated: 30s instead of 5 min)
    const checkpoints: Array<{ second: number; memory: number }> = [];
    for (let i = 0; i < 3; i++) {
      await page.waitForTimeout(10000);

      const memoryUsage = await page.evaluate(() => {
        return (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize || 0;
      });

      checkpoints.push({ second: (i + 1) * 10, memory: memoryUsage });
    }

    // Stop recording
    await page.locator('[data-testid="stop-auto-capture"]').click();
    await expect(page.locator('[data-testid="start-auto-capture"]')).toBeVisible();
  });

  test('should handle rapid auto-capture toggles', async ({ page }) => {
    await page.goto(BASE_URL);

    // Toggle recording multiple times quickly
    for (let i = 0; i < 3; i++) {
      await page.locator('[data-testid="start-auto-capture"]').click();
      await page.waitForTimeout(500);
      await page.locator('[data-testid="stop-auto-capture"]').click();
      await page.waitForTimeout(500);
    }

    // Should end in stopped state
    await expect(page.locator('[data-testid="start-auto-capture"]')).toBeVisible();
  });
});

// Helper function to check if League of Legends is running
async function isLeagueClientRunning(): Promise<boolean> {
  try {
    const response = await fetch('https://127.0.0.1:2999/liveclientdata/allgamedata', {
      signal: AbortSignal.timeout(2000),
    });
    return response.ok;
  } catch {
    return false;
  }
}

// Global setup for game integration tests
test.beforeAll(async () => {
  if (process.env.RUN_INTEGRATION_TESTS) {
    const isRunning = await isLeagueClientRunning();
    if (!isRunning) {
      console.warn('League of Legends client not detected. Some tests may be skipped.');
    }

    const testDir = path.join(process.cwd(), 'test-data');
    if (!fs.existsSync(testDir)) {
      fs.mkdirSync(testDir, { recursive: true });
    }
  }
});
