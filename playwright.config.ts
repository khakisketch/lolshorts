import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright E2E Testing Configuration for LoLShorts
 *
 * Tests the full application stack:
 * - Tauri desktop application
 * - React frontend
 * - Rust backend commands
 * - Authentication flows
 * - Recording functionality
 * - Video processing operations
 */

export default defineConfig({
  testDir: "./tests/e2e",

  // Maximum time one test can run
  timeout: 90 * 1000,

  // Fail the build on CI if you accidentally left test.only in the source code
  forbidOnly: !!process.env.CI,

  // Retry on CI only
  retries: process.env.CI ? 2 : 0,

  // Desktop browser matrix tests share one Vite dev server and Tauri mock layer.
  // Keep local/CI runs bounded to avoid browser process starvation on Windows.
  workers: 2,

  // Reporter to use
  reporter: [
    ["html", { outputFolder: "playwright-report", open: "never" }],
    ["json", { outputFile: "test-results/results.json" }],
    ["junit", { outputFile: "test-results/junit.xml" }],
  ],

  // Shared settings for all tests
  use: {
    // Base URL for the application
    baseURL: "http://127.0.0.1:5181",

    // Collect trace when retrying the failed test
    trace: "on-first-retry",

    // Screenshot on failure
    screenshot: "only-on-failure",

    // Video on failure
    video: "retain-on-failure",

    // Maximum time each action can take
    actionTimeout: 10 * 1000,
  },

  // Configure projects for different browsers/scenarios
  projects: [
    {
      name: "Desktop Chrome",
      testIgnore: /cross-browser-english\.spec\.ts/,
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1280, height: 720 },
      },
    },

    {
      name: "Desktop Firefox",
      testMatch: /cross-browser-english\.spec\.ts/,
      use: {
        ...devices["Desktop Firefox"],
        viewport: { width: 1280, height: 720 },
      },
    },

    {
      name: "Desktop Edge",
      testMatch: /cross-browser-english\.spec\.ts/,
      use: {
        ...devices["Desktop Edge"],
        viewport: { width: 1280, height: 720 },
      },
    },
  ],

  // Run local dev server before starting tests
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 5181",
    url: "http://127.0.0.1:5181",
    timeout: 120 * 1000,
    reuseExistingServer: !process.env.CI,
  },
});
