import { pinnedPathsFor, useAutoEditStore } from "./autoEditStore";

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
describe("autoEditStore.buildConfig — caption_locale 스냅샷", () => {
  afterEach(() => {
    localStorage.removeItem("i18nextLng");
  });

  it("저장된 UI 언어를 그대로 실어 보낸다", () => {
    localStorage.setItem("i18nextLng", "ko");

    const config = useAutoEditStore.getState().buildConfig();
    expect(config.caption_locale).toBe("ko");
  });

  it("언어를 바꾸면 다음 buildConfig 호출부터 반영된다", () => {
    localStorage.setItem("i18nextLng", "en");
    expect(useAutoEditStore.getState().buildConfig().caption_locale).toBe("en");

    localStorage.setItem("i18nextLng", "ko");
    expect(useAutoEditStore.getState().buildConfig().caption_locale).toBe("ko");
  });

  it("언어가 저장돼 있지 않으면 영어로 본다", () => {
    // 감지기가 아직 캐시를 안 쓴 첫 실행 등 — 백엔드 기본값(en)과 맞춰 둔다.
    expect(useAutoEditStore.getState().buildConfig().caption_locale).toBe("en");
  });

  it("훅 자막 켜짐 여부와 독립적으로 언어는 항상 실어 보낸다", () => {
    // 꺼둔 사람이 나중에 다시 켤 수 있으므로, 끈 상태에서도 값 자체는 보낸다.
    localStorage.setItem("i18nextLng", "ko");
    useAutoEditStore.getState().setEnableHookCaptions(false);

    const config = useAutoEditStore.getState().buildConfig();
    expect(config.enable_hook_captions).toBe(false);
    expect(config.caption_locale).toBe("ko");
  });
});

/**
 * 홈에서 고른 클립이 자동편집까지 가는 길.
 *
 * 이 통로가 없던 동안 홈 카드의 선택 체크는 **아무 일도 하지 않는 장식**이었다
 * — 「하이라이트 영상 만들기」가 gameId 도 선택도 넘기지 않고 navigate 만 했다.
 * 다시 그렇게 되지 않도록 규칙을 여기에 못 박는다.
 */
describe("pinnedPathsFor — 그 선택이 이 요청의 답인가", () => {
  const pinned = {
    groups: [{ gameId: "game-1", paths: ["C:/clips/a.mp4"] }],
  };

  it("같은 판 하나만 골랐으면 적용된다", () => {
    expect(pinnedPathsFor(pinned, ["game-1"])).toEqual(["C:/clips/a.mp4"]);
  });

  it("다른 판이면 적용되지 않는다", () => {
    // 홈에서 고르고 나갔다가 나중에 `Games.tsx` 에서 다른 판으로 자동편집을
    // 열었을 때. 남은 선택이 조용히 그 판을 제한하면 사용자는 이유를 알 수 없다.
    expect(pinnedPathsFor(pinned, ["game-2"])).toBeNull();
  });

  it("여러 판에서 고른 경로를 중복 없이 평탄화한다", () => {
    const multi = {
      groups: [
        { gameId: "game-1", paths: ["C:/clips/a.mp4"] },
        { gameId: "game-2", paths: ["C:/clips/b.mp4", "C:/clips/a.mp4"] },
      ],
    };
    expect(pinnedPathsFor(multi, ["game-2", "game-1"])).toEqual([
      "C:/clips/a.mp4",
      "C:/clips/b.mp4",
    ]);
  });

  it('빈 목록은 "0개를 골랐다" 가 아니라 "고른 게 없다" 다', () => {
    expect(
      pinnedPathsFor({ groups: [{ gameId: "game-1", paths: [] }] }, ["game-1"]),
    ).toBeNull();
  });
});

describe("autoEditStore — 고른 클립을 백엔드까지", () => {
  beforeEach(() => {
    useAutoEditStore.getState().resetAll();
  });

  afterEach(() => {
    useAutoEditStore.getState().resetAll();
  });

  it("buildConfig 가 고른 경로를 실어 보낸다", () => {
    const store = useAutoEditStore.getState();
    store.setPinnedClips({
      groups: [
        {
          gameId: "game-1",
          paths: ["C:/clips/penta.mp4", "C:/clips/kill.mp4"],
        },
      ],
    });
    store.toggleGameSelection("game-1");

    expect(
      useAutoEditStore.getState().buildConfig().selected_clip_paths,
    ).toEqual(["C:/clips/penta.mp4", "C:/clips/kill.mp4"]);
  });

  it("여러 게임 ID와 중복 제거된 전체 경로를 wire 형식으로 만든다", () => {
    const store = useAutoEditStore.getState();
    store.setPinnedClips({
      groups: [
        { gameId: "game-1", paths: ["C:/clips/a.mp4"] },
        { gameId: "game-2", paths: ["C:/clips/b.mp4", "C:/clips/a.mp4"] },
      ],
    });
    store.setSelectedGameIds(["game-1", "game-2"]);

    const config = useAutoEditStore.getState().buildConfig();
    expect(config.game_ids).toEqual(["game-1", "game-2"]);
    expect(config.selected_clip_paths).toEqual([
      "C:/clips/a.mp4",
      "C:/clips/b.mp4",
    ]);
  });

  it("고른 게 없으면 필드 자체를 보내지 않는다", () => {
    // 빈 배열을 보내면 백엔드는 `NoClipsFound` 로 실패한다 — 자동 선택을 하라는
    // 뜻이면 필드가 아예 없어야 한다.
    useAutoEditStore.getState().toggleGameSelection("game-1");

    expect(
      useAutoEditStore.getState().buildConfig().selected_clip_paths,
    ).toBeUndefined();
  });

  it("null 로 눕히면 자동 선택으로 되돌아간다", () => {
    const store = useAutoEditStore.getState();
    store.setPinnedClips({
      groups: [{ gameId: "game-1", paths: ["C:/clips/a.mp4"] }],
    });
    store.setPinnedClips(null);
    store.toggleGameSelection("game-1");

    expect(
      useAutoEditStore.getState().buildConfig().selected_clip_paths,
    ).toBeUndefined();
  });

  it("판 미리 고르기는 몇 번을 불러도 결과가 같다", () => {
    // 예전에는 마운트 효과가 `toggleGameSelection` 을 불렀다. 그 효과가 한 번 더
    // 돌면(StrictMode 의 이중 호출, `getAllGames` 정체성 변화) 선택이 도로 풀려
    // "홈에서 만들기를 눌렀는데 게임이 안 골라져 있다" 가 됐다.
    const store = useAutoEditStore.getState();
    store.setSelectedGameIds(["game-1"]);
    store.setSelectedGameIds(["game-1"]);

    expect(useAutoEditStore.getState().selectedGameIds).toEqual(["game-1"]);
  });

  it("미리 고르기가 살아 있어야 고른 클립도 함께 실린다", () => {
    // 판 선택이 풀리면 `pinnedPathsFor` 가 null 을 내서 클립 선택도 같이 죽는다.
    const store = useAutoEditStore.getState();
    store.setPinnedClips({
      groups: [{ gameId: "game-1", paths: ["C:/clips/a.mp4"] }],
    });
    store.setSelectedGameIds(["game-1"]);
    useAutoEditStore.getState().setSelectedGameIds(["game-1"]);

    expect(
      useAutoEditStore.getState().buildConfig().selected_clip_paths,
    ).toEqual(["C:/clips/a.mp4"]);
  });

  it("resetAll 이 선택을 지운다", () => {
    useAutoEditStore.getState().setPinnedClips({
      groups: [{ gameId: "game-1", paths: ["C:/clips/a.mp4"] }],
    });
    useAutoEditStore.getState().resetAll();

    expect(useAutoEditStore.getState().pinnedClips).toBeNull();
  });

  it("스토리보드 순서와 트림을 보존하고 이전 고정 선택보다 우선한다", () => {
    const store = useAutoEditStore.getState();
    store.setSelectedGameIds(["game-1", "game-2"]);
    store.setPinnedClips({
      groups: [{ gameId: "game-1", paths: ["C:/clips/stale.mp4"] }],
    });
    store.setStoryboard([
      {
        game_id: "game-1",
        file_path: "C:/clips/a.mp4",
        order: 4,
        trim_start_secs: 2,
        trim_end_secs: 12,
        source_duration_secs: 20,
        event_type: "kill",
        highlight_score: 88,
        recommended_order: 0,
      },
      {
        game_id: "game-2",
        file_path: "C:/clips/b.mp4",
        order: 9,
        trim_start_secs: 1,
        trim_end_secs: 8,
        source_duration_secs: 15,
        event_type: "assist",
        highlight_score: 70,
        recommended_order: 1,
      },
    ]);
    store.moveStoryboardClip(1, 0);
    store.updateStoryboardTrim("C:/clips/b.mp4", 3, 7);

    const config = useAutoEditStore.getState().buildConfig();
    expect(config.selected_clip_paths).toBeUndefined();
    expect(config.storyboard).toEqual([
      {
        game_id: "game-2",
        file_path: "C:/clips/b.mp4",
        order: 0,
        trim_start_secs: 3,
        trim_end_secs: 7,
      },
      {
        game_id: "game-1",
        file_path: "C:/clips/a.mp4",
        order: 1,
        trim_start_secs: 2,
        trim_end_secs: 12,
      },
    ]);
  });

  it("출력 의도와 프레이밍 및 플랫폼 게시 메타데이터를 wire 형식으로 만든다", () => {
    const store = useAutoEditStore.getState();
    store.setOutputIntent("shorts_series");
    store.setFramingMode("safe_full_frame");
    store.setPlatformPreset("instagram_reels");
    store.setMetadata({
      title: "Ranked highlights",
      caption: "Best plays",
      tags: ["lol", "ranked"],
    });

    expect(useAutoEditStore.getState().buildConfig()).toMatchObject({
      output_intent: "shorts_series",
      framing_mode: "safe_full_frame",
      platform_preset: "instagram_reels",
      publish_metadata: {
        title: "Ranked highlights",
        description: "Best plays",
        tags: ["lol", "ranked"],
        privacy_status: "unlisted",
      },
    });
  });
});
