import { Page, TestInfo } from "@playwright/test";
import {
  test,
  expect,
  BASE_URL,
  loginAsFreeUser,
} from "./fixtures/tauri-fixture";

function collectUnexpectedErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") {
      errors.push(`console.error: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    errors.push(`pageerror: ${error.message}`);
  });
  return errors;
}

async function attachScreenshot(page: Page, testInfo: TestInfo, name: string) {
  const path = testInfo.outputPath(`${name}.png`);
  await page.screenshot({ path, fullPage: true });
  await testInfo.attach(name, { path, contentType: "image/png" });
}

test.describe("Production candidate browser states", () => {
  test("core routes render stable empty or gated states", async ({
    page,
  }, testInfo) => {
    const errors = collectUnexpectedErrors(page);
    const routes = [
      { path: "/", assert: page.getByTestId("dashboard"), name: "dashboard" },
      {
        path: "/settings",
        assert: page.getByTestId("settings"),
        name: "settings",
      },
      {
        path: "/replays",
        assert: page.getByTestId("replays-page"),
        name: "replays",
      },
      {
        path: "/auto-edit",
        assert: page.getByText(/Please login to access Auto-Edit/i),
        name: "auto-edit",
      },
      {
        path: "/youtube",
        assert: page.getByText(/Authentication Required/i),
        name: "youtube-gated",
      },
    ];

    for (const route of routes) {
      await page.goto(`${BASE_URL}${route.path}`, {
        waitUntil: "domcontentloaded",
      });
      await expect(route.assert).toBeVisible({ timeout: 30000 });
      await attachScreenshot(page, testInfo, route.name);
    }

    expect(errors).toEqual([]);
  });

  test("FREE user payment upgrade is visible but checkout is deferred", async ({
    page,
  }, testInfo) => {
    const errors = collectUnexpectedErrors(page);

    await loginAsFreeUser(page);
    await page.goto(`${BASE_URL}/settings`, { waitUntil: "domcontentloaded" });
    const settings = page.getByTestId("settings");
    await expect(settings).toBeVisible({ timeout: 30000 });

    await settings.getByRole("button", { name: /Upgrade to PRO/i }).click();
    await expect(page.getByRole("dialog")).toBeVisible({ timeout: 10000 });
    await expect(
      page.getByText(/Payment and PRO upgrades are deferred/i),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: /Select Plan/i }),
    ).toBeDisabled();
    await attachScreenshot(page, testInfo, "payment-deferred");

    expect(errors).toEqual([]);
  });

  test("FREE user cannot open PRO YouTube workflow from local state alone", async ({
    page,
  }, testInfo) => {
    const errors = collectUnexpectedErrors(page);

    await loginAsFreeUser(page);
    await page.goto(`${BASE_URL}/youtube`, { waitUntil: "domcontentloaded" });

    await expect(page.getByText(/PRO Feature/i)).toBeVisible({
      timeout: 30000,
    });
    await expect(page.getByText(/exclusive/i)).toBeVisible();
    await expect(
      page.locator("#main-content").getByRole("button", {
        name: /Upgrade to PRO/i,
      }),
    ).toBeVisible();
    await attachScreenshot(page, testInfo, "youtube-free-gate");

    expect(errors).toEqual([]);
  });
});
