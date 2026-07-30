---
description: LoLShorts 앱을 띄워 화면 렌더를 확인한다 — 빠른 경로는 Playwright Tauri-mock으로 브라우저 렌더+스크린샷(UI 시각 검증용), 전체 경로는 tauri dev(실제 녹화/ffmpeg 필요 시).
---

# LoLShorts 실행·렌더 검증

## 언제 어떤 경로를 쓰나

| 목적 | 경로 |
|---|---|
| UI 변경 후 시각 검증(스크린샷), 빈 상태/뷰포트/locale 확인 | **A. Playwright Tauri-mock** (빠름, 헤드리스 가능) |
| 실제 녹화·ffmpeg·OAuth 등 백엔드 동작 확인 | **B. tauri dev** (실행 시간 김, 실기기 필요) |

주의: 일반 브라우저에서 `npm run dev`만 띄우면 Tauri IPC(`window.__TAURI_INTERNALS__`)가 없어 데이터 로드가 전부 실패한다. 브라우저 렌더는 반드시 A 경로(mock 주입)로.

## A. Playwright Tauri-mock 렌더 (권장: UI 검증)

`tests/e2e/fixtures/tauri-fixture.ts`가 addInitScript로 Tauri invoke 전체를 mock한다(온보딩 스킵, `loginAsFreeUser`/`loginAsProUser` 헬퍼 포함). vite dev 서버는 Playwright webServer 설정이 자동으로 띄운다(포트 5181).

1. `tests/e2e/__visual_check.spec.ts` 같은 **임시 spec**을 만든다 (사용 후 삭제):

```ts
import { test, expect } from "./fixtures/tauri-fixture";
import { loginAsProUser } from "./fixtures/tauri-fixture";

const SHOT_DIR = "<스크래치 디렉토리>/shots";
const ROUTES: Array<[string, string]> = [
  ["dashboard", "/"], ["editor", "/editor"], ["auto-edit", "/auto-edit"],
  ["youtube", "/youtube"], ["results", "/results"], ["settings", "/settings"],
];

for (const [vpName, vp] of [
  ["desktop", { width: 1280, height: 800 }],
  ["mobile390", { width: 390, height: 844 }],
] as const) {
  test(`screens @ ${vpName}`, async ({ page }) => {
    await page.setViewportSize(vp);
    // 한국어 확인 시: await page.addInitScript(() => localStorage.setItem("i18nextLng", "ko"));
    await loginAsProUser(page); // FREE 상태는 loginAsFreeUser
    for (const [name, route] of ROUTES) {
      await page.goto(route, { waitUntil: "domcontentloaded" });
      await page.waitForTimeout(1200);
      await page.screenshot({ path: `${SHOT_DIR}/${vpName}-${name}.png`, fullPage: true });
    }
    expect(true).toBe(true);
  });
}
```

2. 실행 (메인 e2e 리포트와 충돌하지 않게 별도 리포터/출력):
```
npx playwright test tests/e2e/__visual_check.spec.ts --project="Desktop Chrome" --reporter=line --output=test-results-visual
```
3. 스크린샷을 Read로 직접 보고 판정: 오버플로·한글 세로쪼개짐·빈 상태 탈출구·raw 코드값 라벨·390px 세로 reflow.
4. **임시 spec 삭제.**

한계: fixture mock은 `list_games`가 빈 배열이라 게임이 로드된 Editor 내부(타임라인/트림)는 이 경로로 못 본다 — 그건 B 경로 수동 QA. mock을 즉석에서 덮어쓰려면 test 본문에서 `page.addInitScript`로 `window.__TAURI_INTERNALS__.invoke`를 래핑하되, 앱이 기대하는 응답 shape(`src/types/storage.ts`)을 정확히 맞추지 않으면 로딩에 갇힌다.

## B. 실제 앱 (tauri dev)

```
npm run tauri:dev
```
- 요구: `.env`(Supabase/YouTube 키), 번들 ffmpeg(빌드 스크립트가 준비), Windows.
- 기동 시 게임 모니터링·전역 핫키(F8/F9/F10)·보존 정책 사이클이 실제로 돈다.
- 전체 e2e: `npx playwright test --project="Desktop Chrome"` (mock 기반 114개, ~3분).

## 자주 쓰는 검증 명령

- 백엔드: `cd src-tauri && cargo check --all-targets && cargo test --lib`
- 프론트: `npx tsc --noEmit && npx jest`
