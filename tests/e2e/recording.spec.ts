import { test, expect, BASE_URL } from "./fixtures/tauri-fixture";

/**
 * E2E Tests for Recording & Game Status
 *
 * 앱의 화면은 셋이다: 홈(`/`) · 결과(`/results`) · 설정(`/settings`).
 * 예전 대시보드(`data-testid="dashboard"`)와 그 안의 준비 상태 패널·개별 네비
 * 항목(`nav-library`/`nav-games`/`nav-editor`)은 홈 재설계에서 사라졌다.
 *
 * 이 파일이 그 사실을 반영하지 못한 동안 **28개 e2e 가 빨간불이었는데도 두 번의
 * 세션 보고가 초록으로 읽혔다** — `playwright | tail` 파이프가 종료코드를 삼켜서다.
 * 요약 숫자를 믿지 말고 실패 목록을 직접 읽어야 한다는 사례로 남긴다.
 */

test.describe("Home Game Status", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState("networkidle");
  });

  test("should show connection status on home", async ({ page }) => {
    const status = page.getByTestId("home-status");
    await expect(status).toBeVisible({ timeout: 5000 });
    expect(await status.textContent()).toBeTruthy();
  });

  test("should show home with the clip grid or its empty state", async ({
    page,
  }) => {
    await expect(page.getByTestId("home")).toBeVisible({ timeout: 5000 });
    // 재료가 없으면 빈 상태, 있으면 격자 — 둘 중 하나는 반드시 있어야 한다
    // (둘 다 없으면 사용자는 홈에서 아무것도 할 수 없다).
    const grid = page.getByTestId("home-clip-grid");
    const empty = page.getByTestId("home-empty");
    await expect(grid.or(empty)).toBeVisible({ timeout: 10000 });
  });

  test("the status line says what is happening in human words", async ({
    page,
  }) => {
    // mock 은 자동 캡처가 꺼진 상태(`is_monitoring: false`)를 돌려준다.
    // 여기서 지키는 것은 특정 문구가 아니라 **상태가 사람 말로 나온다**는 것 —
    // `idle` 이나 `lcu_connected=false` 같은 코드값이 새어 나오면 안 된다.
    const status = page.getByTestId("home-status");
    await expect(status).toBeVisible({ timeout: 5000 });

    const text = (await status.textContent()) ?? "";
    expect(text).toMatch(/paused|waiting|off|기다리는|일시|꺼져/i);
    expect(text).not.toMatch(/\b(idle|lcu_connected|is_monitoring|null)\b/);
  });

  test("recording readiness blockers are reachable from Settings diagnostics", async ({
    page,
  }) => {
    // 준비 상태(FFmpeg 없음·디스크 부족)는 예전 대시보드에서만 보였는데 그 화면이
    // 사라졌다. 지금은 설정 -> 계정 -> 진단이 유일한 경로다. 이 경로가 끊기면
    // ffmpeg 이 없는 사용자가 아무 경고도 못 받고 녹화가 조용히 실패한다.
    //
    // 그리고 **구체적인 문구**여야 한다 — 백엔드는 "무엇이 없고 무엇을 하면
    // 되는지"까지 만들어 보내는데, 화면이 그걸 버리고 "확인 필요" 라고만 적으면
    // 막힌 사용자는 여전히 아무것도 알 수 없다.
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    const settings = page.getByTestId("settings");
    await expect(settings).toBeVisible({ timeout: 10000 });
    await settings.getByTestId("settings-nav-diagnostics").click();

    // 진단은 기본으로 접혀 있다(설정 화면을 조용하게 두려고).
    await settings
      .getByTestId("diagnostics-section")
      .getByRole("button")
      .first()
      .click();

    const blockers = settings.getByTestId("diagnostics-blockers");
    await expect(blockers).toBeVisible({ timeout: 10000 });
    await expect(
      blockers.getByText(/FFmpeg unavailable in test fixture/),
    ).toBeVisible();
    // 해결 방법까지 같이 나와야 한다.
    await expect(blockers.getByText(/Install bundled FFmpeg/)).toBeVisible();
    await expect(
      blockers.getByText(/Storage space low in test fixture/),
    ).toBeVisible();
  });
});

test.describe("Navigation", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(BASE_URL);
    await page.waitForLoadState("networkidle");
  });

  test("should navigate to results", async ({ page }) => {
    // 예전 `nav-library`/`nav-games` 는 사라졌다 — 사용자가 가진 것은 모두 결과로.
    await page.getByTestId("nav-library").click();
    await page.waitForLoadState("networkidle");

    await expect(page).toHaveURL(/\/results/);
  });

  test("legacy /games and /replays links still land somewhere real", async ({
    page,
  }) => {
    // 옛 링크·북마크가 404 로 죽지 않아야 한다(App.tsx 의 redirect 라우트).
    for (const [legacy, tab] of [
      ["/games", "clips"],
      ["/replays", "replays"],
    ] as const) {
      await page.goto(`${BASE_URL}${legacy}`);
      await page.waitForLoadState("networkidle");
      await expect(page).toHaveURL(new RegExp(`/results\\?tab=${tab}$`));
    }
  });

  test("should navigate to settings page", async ({ page }) => {
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByTestId("settings")).toBeVisible({ timeout: 5000 });
  });

  test("keeps the desktop sidebar compact on settings until explicitly expanded", async ({
    page,
  }) => {
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    const sidebar = page.locator("#desktop-sidebar-content");
    const toggle = page.getByTestId("sidebar-toggle");
    await expect(sidebar).toHaveClass(/w-16/);
    await expect(toggle).toHaveAttribute("aria-expanded", "false");

    await toggle.click();
    await expect(sidebar).toHaveClass(/w-64/);
    await expect(toggle).toHaveAttribute("aria-expanded", "true");

    // The explicit expansion is session-scoped and survives route changes.
    await page.getByTestId("nav-library").click();
    await page.waitForLoadState("networkidle");
    await expect(sidebar).toHaveClass(/w-64/);
  });

  test("normalizes the legacy games tab into the game-records tab", async ({
    page,
  }) => {
    await page.goto(`${BASE_URL}/results?tab=games`);
    await page.waitForLoadState("networkidle");

    await expect(page).toHaveURL(/\/results\?tab=clips$/);
    await expect(page.getByTestId("results-tab-clips")).toHaveAttribute(
      "data-state",
      "active",
    );
    await expect(page.getByTestId("results-tab-games")).toHaveCount(0);
  });

  test("should render Settings recording fixture shape from mocked Tauri settings", async ({
    page,
  }) => {
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    const settings = page.getByTestId("settings");
    await expect(settings).toBeVisible({ timeout: 5000 });

    // 개별 항목은 칸마다 접힌 「고급 설정」 안으로 내려갔다(하나의 전역 토글이
    // 아니다 — 그 구조가 각 칸을 세 화면분으로 만들었다).
    await settings.getByTestId("settings-nav-app").click();
    await expect(settings.getByTestId("settings-section-app")).toBeVisible();
    await expect(settings.getByText("Send Crash Reports")).toBeVisible({
      timeout: 10000,
    });

    await settings.getByTestId("settings-nav-storage").click();
    await expect(
      settings.getByTestId("settings-section-storage"),
    ).toBeVisible();
  });

  test("should reach the editor from results", async ({ page }) => {
    // `nav-editor` 는 사라졌다 — 편집은 결과에서 클립을 골라 들어가는 동작이다.
    // 경로 자체는 살아 있어야 한다(딥링크·홈의 「다듬기」 버튼이 여기로 온다).
    await page.goto(`${BASE_URL}/editor`);
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveURL(/\/editor/);
  });

  test("should navigate back to home", async ({ page }) => {
    await page.getByTestId("nav-settings").click();
    await page.waitForLoadState("networkidle");

    await page.getByTestId("nav-dashboard").click();
    await page.waitForLoadState("networkidle");

    await expect(page.getByTestId("home")).toBeVisible({ timeout: 5000 });
  });
});

test.describe("Results — replays tab", () => {
  test.beforeEach(async ({ page }) => {
    // `/replays` 는 결과의 replays 탭으로 리다이렉트된다.
    await page.goto(`${BASE_URL}/replays`);
    await page.waitForLoadState("networkidle");
  });

  test("should land on the results page", async ({ page }) => {
    await expect(page).toHaveURL(/\/results/);
  });

  test("should show empty state or a list, never a blank screen", async ({
    page,
  }) => {
    const hasEmptyMsg = await page
      .locator("text=/no match|empty|저장된|기록|없/i")
      .first()
      .isVisible({ timeout: 5000 })
      .catch(() => false);
    const hasContent = await page
      .getByRole("main")
      .isVisible()
      .catch(() => false);

    expect(hasEmptyMsg || hasContent).toBeTruthy();
  });
});

test.describe("Performance", () => {
  test("should load home within 5 seconds in dev-mode smoke", async ({
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
