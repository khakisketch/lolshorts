import { useAutoEditStore } from './autoEditStore';

/**
 * 훅 자막은 픽셀로 구워진다 — 나중에 UI 언어를 바꿔도 이미 만든 영상은
 * 안 바뀐다. 그래서 `buildConfig()` 가 백엔드로 보내는 시점의 UI 언어를
 * 정확히 스냅샷하는지가 자막 언어의 유일한 진입점이다. 여기서 어긋나면
 * 한국어 UI 사용자가 영어 자막 영상을 받는 식의 결함이 조용히 생긴다.
 *
 * `i18next-browser-languagedetector` 가 실제로 언어를 캐시하는 곳은
 * `localStorage['i18nextLng']` 다(`i18n.ts` 의 `detection.caches` 설정,
 * e2e 픽스처도 같은 키를 쓴다) — 그래서 여기서도 그 키를 직접 조작한다.
 */
describe('autoEditStore.buildConfig — caption_locale 스냅샷', () => {
  afterEach(() => {
    localStorage.removeItem('i18nextLng');
  });

  it('저장된 UI 언어를 그대로 실어 보낸다', () => {
    localStorage.setItem('i18nextLng', 'ko');

    const config = useAutoEditStore.getState().buildConfig();
    expect(config.caption_locale).toBe('ko');
  });

  it('언어를 바꾸면 다음 buildConfig 호출부터 반영된다', () => {
    localStorage.setItem('i18nextLng', 'en');
    expect(useAutoEditStore.getState().buildConfig().caption_locale).toBe('en');

    localStorage.setItem('i18nextLng', 'ko');
    expect(useAutoEditStore.getState().buildConfig().caption_locale).toBe('ko');
  });

  it('언어가 저장돼 있지 않으면 영어로 본다', () => {
    // 감지기가 아직 캐시를 안 쓴 첫 실행 등 — 백엔드 기본값(en)과 맞춰 둔다.
    expect(useAutoEditStore.getState().buildConfig().caption_locale).toBe('en');
  });

  it('훅 자막 켜짐 여부와 독립적으로 언어는 항상 실어 보낸다', () => {
    // 꺼둔 사람이 나중에 다시 켤 수 있으므로, 끈 상태에서도 값 자체는 보낸다.
    localStorage.setItem('i18nextLng', 'ko');
    useAutoEditStore.getState().setEnableHookCaptions(false);

    const config = useAutoEditStore.getState().buildConfig();
    expect(config.enable_hook_captions).toBe(false);
    expect(config.caption_locale).toBe('ko');
  });
});
