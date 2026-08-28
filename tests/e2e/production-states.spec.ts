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
      { path: "/", assert: page.getByTestId("home"), name: "home" },
      {
        path: "/settings",
        assert: page.getByTestId("settings"),
        name: "settings",
      },
      // `/replays`·`/youtube` 는 결과의 탭으로 리다이렉트된다(옛 링크를 살려 두기
      // 위해 경로만 남겼다). 그래서 **자기 화면이 아니라 결과가 떠야** 맞다 —
      // 리다이렉트가 끊기면 여기가 먼저 깨진다.
      {
        path: "/replays",
        assert: page.getByTestId("results-page"),
        name: "replays-redirect",
      },
      {
        path: "/youtube",
        assert: page.getByTestId("results-page"),
        name: "youtube-redirect",
      },
      {
        path: "/auto-edit",
        assert: page.getByText(/Please login to access Auto-Edit/i),
        name: "auto-edit",
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

  test("FREE public edition exposes no payment or PRO checkout", async ({
    page,
  }, testInfo) => {
    const errors = collectUnexpectedErrors(page);

    await loginAsFreeUser(page);
    await page.goto(`${BASE_URL}/settings`, { waitUntil: "domcontentloaded" });
    const settings = page.getByTestId("settings");
    await expect(settings).toBeVisible({ timeout: 30000 });

    await settings.getByTestId("settings-nav-license").click();
    await expect(
      page.getByText(/Free public edition/i),
    ).toBeVisible();
    await expect(
      settings.getByRole("button", { name: /Upgrade to PRO/i }),
    ).toHaveCount(0);
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await attachScreenshot(page, testInfo, "free-public-edition");

    expect(errors).toEqual([]);
  });

  test("sharing is entered from results, not as its own gated screen", async ({
    page,
  }, testInfo) => {
    // **의도적으로 바뀐 규칙이다.** 예전에는 `/youtube` 가 독립 화면이고 FREE 는
    // "PRO Feature" 벽을 만났다. 지금은 업로드 자체가 로그인만 하면 무료이고
    // (예약·일괄 업로드만 PRO), 공유는 결과 라이브러리 위에 뜨는 대화상자다.
    // 그래서 이 테스트가 지키는 것은 PRO 벽이 아니라 **동선이 살아 있는가**다.
    const errors = collectUnexpectedErrors(page);

    await loginAsFreeUser(page);
    await page.goto(`${BASE_URL}/youtube`, { waitUntil: "domcontentloaded" });

    // 옛 경로는 죽지 않고 결과로 넘어간다.
    await expect(page).toHaveURL(/\/results/);
    await expect(page.getByTestId("results-page")).toBeVisible({
      timeout: 30000,
    });
    // 그리고 그 화면에서 막다른 길("PRO 전용")로 끝나지 않는다.
    await expect(page.getByText(/PRO Feature/i)).toHaveCount(0);
    await attachScreenshot(page, testInfo, "youtube-redirects-to-results");

    expect(errors).toEqual([]);
  });
});
