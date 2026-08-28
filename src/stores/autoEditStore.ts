import { create } from "zustand";
import {
  CanvasTemplate,
  CanvasTemplateInfo,
  BackgroundMusic,
  AudioLevels,
  AutoEditConfig,
  AutoEditProgress,
  AutoEditResult,
  GameSelection,
  DurationOption,
  AutoEditStep,
  VideoError,
  AutoEditMetadata,
  AutoEditPlanClip,
  AutoEditOutputIntent,
  AutoEditFramingMode,
  PlatformPreset,
} from "@/types/autoEdit";

export interface PinnedClipGroup {
  gameId: string;
  paths: string[];
}

/** 다른 화면에서 넘어온 직접 선택. 파일 경로는 URL 대신 런타임 상태로만 이동한다. */
export interface PinnedClipSelection {
  groups: PinnedClipGroup[];
}

/**
 * 지금 요청에 그 선택이 실제로 적용되는가.
 *
 * 판이 늘거나 바뀌었으면 그 선택은 이 요청의 답이 아니다. 이 규칙이 `buildConfig`
 * 와 화면 안내 두 곳에 흩어지면 "안내는 나오는데 안 걸리거나" 그 반대가 되므로
 * 한 함수로 둔다.
 */
export function pinnedPathsFor(
  pinned: PinnedClipSelection | null,
  selectedGameIds: string[],
): string[] | null {
  if (!pinned || pinned.groups.length === 0) return null;
  const pinnedGameIds = [
    ...new Set(pinned.groups.map((group) => group.gameId)),
  ];
  const selected = new Set(selectedGameIds);
  if (
    selected.size !== pinnedGameIds.length ||
    pinnedGameIds.some((gameId) => !selected.has(gameId))
  ) {
    return null;
  }

  const paths = [
    ...new Set(
      pinned.groups.flatMap((group) =>
        group.paths.filter((path) => path.length > 0),
      ),
    ),
  ];
  return paths.length > 0 ? paths : null;
}

interface AutoEditStore {
  // Step flow
  currentStep: AutoEditStep;
  setCurrentStep: (step: AutoEditStep) => void;

  // Game selection
  availableGames: GameSelection[];
  selectedGameIds: string[];
  setAvailableGames: (games: GameSelection[]) => void;
  toggleGameSelection: (gameId: string) => void;
  /**
   * 선택을 **그 목록으로 놓는다** — 토글이 아니다.
   *
   * 다른 화면에서 `?gameId=` 를 들고 들어올 때 쓴다. 예전에는 마운트 효과가
   * `toggleGameSelection` 을 불렀는데, 그 효과가 한 번 더 돌면(StrictMode 의
   * 이중 호출, 또는 `getAllGames` 의 정체성이 바뀔 때) **선택이 도로 풀린다**.
   * 같은 값을 몇 번 넣어도 결과가 같아야 한다.
   */
  setSelectedGameIds: (gameIds: string[]) => void;
  clearGameSelection: () => void;

  /**
   * 다른 화면(홈)에서 이미 고른 클립 — 자동 선택 대신 이것만 쓴다.
   *
   * **어느 판에서 고른 것인지 함께 들고 다닌다.** 경로만 들고 있으면, 홈에서
   * 고른 뒤 나중에 다른 화면(`Games.tsx`)에서 다른 판으로 자동편집을 열었을 때
   * 남아 있던 선택이 조용히 그 판을 제한한다 — 사용자는 이유를 알 수 없다.
   * `buildConfig` 는 지금 고른 판이 정확히 그 판일 때만 이 선택을 보낸다.
   */
  pinnedClips: PinnedClipSelection | null;
  setPinnedClips: (pinned: PinnedClipSelection | null) => void;

  // Duration
  targetDuration: DurationOption;
  setTargetDuration: (duration: DurationOption) => void;

  // Effects (experimental)
  enableEventZoom: boolean;
  setEnableEventZoom: (enabled: boolean) => void;

  /**
   * 훅 자막 — 각 클립 앞머리에 "무슨 장면이고 왜 볼 만한지" 한 줄.
   *
   * 기본이 켜짐인 이유: 자막 없는 세로 클립은 쇼츠가 아니라 세로 상자에 든
   * 클립이다. 아무것도 건드리지 않은 사용자의 결과물이 그대로 올릴 만해야 한다.
   */
  enableHookCaptions: boolean;
  setEnableHookCaptions: (enabled: boolean) => void;

  // Canvas template
  currentTemplate: CanvasTemplate | null;
  availableTemplates: CanvasTemplateInfo[];
  isEditingCanvas: boolean;
  setCurrentTemplate: (template: CanvasTemplate | null) => void;
  setAvailableTemplates: (templates: CanvasTemplateInfo[]) => void;
  setIsEditingCanvas: (editing: boolean) => void;
  clearCanvas: () => void;

  // Audio
  backgroundMusic: BackgroundMusic | null;
  audioLevels: AudioLevels;
  setBackgroundMusic: (music: BackgroundMusic | null) => void;
  setAudioLevels: (levels: Partial<AudioLevels>) => void;
  clearAudio: () => void;

  // Progress & Result
  jobId: string | null;
  progress: AutoEditProgress | null;
  result: AutoEditResult | null;
  error: VideoError | null;
  metadata: AutoEditMetadata;
  setJobId: (id: string | null) => void;
  setProgress: (progress: AutoEditProgress | null) => void;
  setResult: (result: AutoEditResult | null) => void;
  setError: (error: VideoError | null) => void;
  setMetadata: (metadata: Partial<AutoEditMetadata>) => void;

  storyboard: AutoEditPlanClip[];
  recommendedStoryboard: AutoEditPlanClip[];
  storyboardPast: AutoEditPlanClip[][];
  storyboardFuture: AutoEditPlanClip[][];
  setStoryboard: (clips: AutoEditPlanClip[]) => void;
  moveStoryboardClip: (from: number, to: number) => void;
  updateStoryboardTrim: (path: string, start: number, end: number) => void;
  removeStoryboardClip: (path: string) => void;
  clearStoryboard: () => void;
  resetStoryboardToRecommendation: () => void;
  undoStoryboard: () => void;
  redoStoryboard: () => void;
  outputIntent: AutoEditOutputIntent;
  framingMode: AutoEditFramingMode;
  platformPreset: PlatformPreset;
  setOutputIntent: (intent: AutoEditOutputIntent) => void;
  setFramingMode: (mode: AutoEditFramingMode) => void;
  setPlatformPreset: (preset: PlatformPreset) => void;

  // Actions
  buildConfig: () => AutoEditConfig;
  resetAll: () => void;
  resetProgress: () => void;
}

const DEFAULT_AUDIO_LEVELS: AudioLevels = {
  game_audio: 70,
  background_music: 30,
};

/**
 * 지금 UI 가 쓰고 있는 언어 코드.
 *
 * 훅 자막은 픽셀로 구워져서 나중에 언어를 바꿔도 이미 만든 영상은 안 바뀐다 —
 * 그래서 이 store 는 zustand 상태가 아니라 요청 시점에 직접 읽는다.
 *
 * `i18next` 싱글턴(`@/i18n`)을 직접 import 하지 않는 이유: 그 모듈은 import
 * 되는 순간 `i18next-browser-languagedetector` 등 부수효과가 있는 초기화 체인이
 * 돈다. 이 store 는 컴포넌트가 마운트되기 전에도 import 될 수 있는 순수 데이터
 * 계층이라, 그 초기화를 강제로 끌어오는 대신 감지기가 실제로 캐시하는 저장소
 * 키(`i18nextLng` — `i18n.ts` 의 `detection.caches` 설정)를 직접 읽는다. 같은
 * 키를 e2e 픽스처(`tests/e2e/fixtures/tauri-fixture.ts`)도 이미 이 이름으로 쓴다.
 */
function currentUiLanguage(): string {
  try {
    return localStorage.getItem("i18nextLng") ?? "en";
  } catch {
    return "en";
  }
}

export const useAutoEditStore = create<AutoEditStore>((set, get) => ({
  // Initial state
  currentStep: "configure",

  availableGames: [],
  selectedGameIds: [],
  pinnedClips: null,

  targetDuration: 60,

  enableEventZoom: false,
  enableHookCaptions: true,

  currentTemplate: null,
  availableTemplates: [],
  isEditingCanvas: false,

  backgroundMusic: null,
  audioLevels: DEFAULT_AUDIO_LEVELS,

  metadata: {
    title: "",
    caption: "",
    tags: [],
  },
  storyboard: [],
  recommendedStoryboard: [],
  storyboardPast: [],
  storyboardFuture: [],
  outputIntent: "single_short",
  framingMode: "lol_focus_stack",
  platformPreset: "youtube_shorts",

  jobId: null,
  progress: null,
  result: null,
  error: null,

  // Step management
  setCurrentStep: (step) => set({ currentStep: step }),

  // Game selection
  setAvailableGames: (games) => set({ availableGames: games }),

  toggleGameSelection: (gameId) => {
    const { selectedGameIds } = get();
    const isSelected = selectedGameIds.includes(gameId);

    set({
      selectedGameIds: isSelected
        ? selectedGameIds.filter((id) => id !== gameId)
        : [...selectedGameIds, gameId],
    });
  },

  setSelectedGameIds: (gameIds) => set({ selectedGameIds: [...gameIds] }),

  clearGameSelection: () => set({ selectedGameIds: [] }),

  setPinnedClips: (pinned) =>
    set({
      pinnedClips:
        pinned &&
        pinned.groups.some((group) =>
          group.paths.some((path) => path.length > 0),
        )
          ? {
              groups: pinned.groups
                .map((group) => ({
                  gameId: group.gameId,
                  paths: [...new Set(group.paths.filter(Boolean))],
                }))
                .filter((group) => group.paths.length > 0),
            }
          : null,
    }),

  // Duration
  setTargetDuration: (duration) => set({ targetDuration: duration }),

  // Effects (experimental)
  setEnableEventZoom: (enabled) => set({ enableEventZoom: enabled }),

  setEnableHookCaptions: (enabled) => set({ enableHookCaptions: enabled }),

  // Canvas template
  setCurrentTemplate: (template) => set({ currentTemplate: template }),

  setAvailableTemplates: (templates) => set({ availableTemplates: templates }),

  setIsEditingCanvas: (editing) => set({ isEditingCanvas: editing }),

  clearCanvas: () => set({ currentTemplate: null, isEditingCanvas: false }),

  // Audio
  setBackgroundMusic: (music) => set({ backgroundMusic: music }),

  setAudioLevels: (levels) =>
    set({
      audioLevels: { ...get().audioLevels, ...levels },
    }),

  clearAudio: () =>
    set({
      backgroundMusic: null,
      audioLevels: DEFAULT_AUDIO_LEVELS,
    }),

  // Progress & Result
  setJobId: (id) => set({ jobId: id }),

  setProgress: (progress) => set({ progress }),

  setResult: (result) => set({ result }),

  setError: (error) => set({ error }),

  setMetadata: (metadata) =>
    set({
      metadata: { ...get().metadata, ...metadata },
    }),

  setStoryboard: (clips) =>
    set(() => {
      const normalized = clips.map((clip, index) => ({
        ...clip,
        order: index,
      }));
      return {
        storyboard: normalized,
        recommendedStoryboard: normalized.map((clip) => ({ ...clip })),
        storyboardPast: [],
        storyboardFuture: [],
      };
    }),

  moveStoryboardClip: (from, to) => {
    const current = get().storyboard;
    const clips = [...current];
    if (from < 0 || to < 0 || from >= clips.length || to >= clips.length)
      return;
    const [clip] = clips.splice(from, 1);
    clips.splice(to, 0, clip);
    set({
      storyboard: clips.map((item, index) => ({ ...item, order: index })),
      storyboardPast: [
        ...get().storyboardPast,
        current.map((clip) => ({ ...clip })),
      ].slice(-50),
      storyboardFuture: [],
    });
  },

  updateStoryboardTrim: (path, start, end) =>
    set({
      storyboardPast: [
        ...get().storyboardPast,
        get().storyboard.map((clip) => ({ ...clip })),
      ].slice(-50),
      storyboardFuture: [],
      storyboard: get().storyboard.map((clip) =>
        clip.file_path === path
          ? {
              ...clip,
              trim_start_secs: Math.max(0, Math.min(start, end - 0.1)),
              trim_end_secs: Math.min(
                clip.source_duration_secs,
                Math.max(end, start + 0.1),
              ),
            }
          : clip,
      ),
    }),

  removeStoryboardClip: (path) =>
    set({
      storyboardPast: [
        ...get().storyboardPast,
        get().storyboard.map((clip) => ({ ...clip })),
      ].slice(-50),
      storyboardFuture: [],
      storyboard: get()
        .storyboard.filter((clip) => clip.file_path !== path)
        .map((clip, index) => ({ ...clip, order: index })),
    }),

  clearStoryboard: () =>
    set({
      storyboard: [],
      recommendedStoryboard: [],
      storyboardPast: [],
      storyboardFuture: [],
    }),
  resetStoryboardToRecommendation: () =>
    set({
      storyboardPast: [
        ...get().storyboardPast,
        get().storyboard.map((clip) => ({ ...clip })),
      ].slice(-50),
      storyboardFuture: [],
      storyboard: get().recommendedStoryboard.map((clip, index) => ({
        ...clip,
        order: index,
      })),
    }),
  undoStoryboard: () => {
    const past = get().storyboardPast;
    if (past.length === 0) return;
    const previous = past[past.length - 1];
    set({
      storyboard: previous.map((clip) => ({ ...clip })),
      storyboardPast: past.slice(0, -1),
      storyboardFuture: [
        get().storyboard.map((clip) => ({ ...clip })),
        ...get().storyboardFuture,
      ].slice(0, 50),
    });
  },
  redoStoryboard: () => {
    const future = get().storyboardFuture;
    if (future.length === 0) return;
    const next = future[0];
    set({
      storyboard: next.map((clip) => ({ ...clip })),
      storyboardPast: [
        ...get().storyboardPast,
        get().storyboard.map((clip) => ({ ...clip })),
      ].slice(-50),
      storyboardFuture: future.slice(1),
    });
  },
  setOutputIntent: (outputIntent) => set({ outputIntent }),
  setFramingMode: (framingMode) => set({ framingMode }),
  setPlatformPreset: (platformPreset) => set({ platformPreset }),

  // Build final config for backend
  buildConfig: (): AutoEditConfig => {
    const {
      selectedGameIds,
      pinnedClips,
      targetDuration,
      currentTemplate,
      backgroundMusic,
      audioLevels,
      enableEventZoom,
      enableHookCaptions,
      storyboard,
      outputIntent,
      framingMode,
      platformPreset,
      metadata,
    } = get();

    const config: AutoEditConfig = {
      game_ids: selectedGameIds,
      target_duration: targetDuration,
      enable_event_zoom: enableEventZoom,
      enable_hook_captions: enableHookCaptions,
      // 자막은 픽셀로 구워지므로 지금 이 순간의 UI 언어를 스냅샷해 보낸다 — 나중에
      // 언어를 바꿔도 이미 만든 영상에는 영향이 없어야 한다. 지역 서브태그가
      // 붙을 수 있다(`ko-KR`); 백엔드가 앞 두 글자만 본다.
      caption_locale: currentUiLanguage(),
      // Always sent: `audio_levels` also carries the GAME audio volume, so it is
      // meaningful without background music. The Rust side takes it by value
      // (AutoEditConfig.audio_levels is not Option), so omitting it used to fail
      // deserialization with "missing field audio_levels".
      audio_levels: audioLevels,
      output_intent: outputIntent,
      framing_mode: framingMode,
      platform_preset: platformPreset,
      publish_metadata: {
        title: metadata.title,
        description: metadata.caption,
        tags: metadata.tags,
        privacy_status: "unlisted",
      },
    };

    // 다른 화면에서 고른 클립은 **그 판 하나만 고른 상태일 때만** 보낸다.
    // 조용히 적용하면 사용자는 왜 클립 3개짜리 영상이 나왔는지 알 길이 없다 —
    // 화면 안내(`AutoEditSettings`)도 같은 함수로 판정한다.
    if (storyboard.length > 0) {
      config.storyboard = storyboard.map((clip, index) => ({
        game_id: clip.game_id,
        file_path: clip.file_path,
        order: index,
        trim_start_secs: clip.trim_start_secs,
        trim_end_secs: clip.trim_end_secs,
      }));
    }

    const pinnedPaths = pinnedPathsFor(pinnedClips, selectedGameIds);
    if (!config.storyboard && pinnedPaths) {
      config.selected_clip_paths = pinnedPaths;
    }

    if (currentTemplate) {
      config.canvas_template = currentTemplate;
    }

    if (backgroundMusic) {
      config.background_music = backgroundMusic;
    }

    return config;
  },

  // Reset all state
  resetAll: () =>
    set({
      currentStep: "configure",
      selectedGameIds: [],
      pinnedClips: null,
      targetDuration: 60,
      enableEventZoom: false,
      currentTemplate: null,
      isEditingCanvas: false,
      backgroundMusic: null,
      audioLevels: DEFAULT_AUDIO_LEVELS,
      metadata: {
        title: "",
        caption: "",
        tags: [],
      },
      storyboard: [],
      recommendedStoryboard: [],
      storyboardPast: [],
      storyboardFuture: [],
      outputIntent: "single_short",
      framingMode: "lol_focus_stack",
      platformPreset: "youtube_shorts",
      jobId: null,
      progress: null,
      result: null,
      error: null,
    }),

  // Reset only progress/result (for new generation)
  resetProgress: () =>
    set({
      jobId: null,
      progress: null,
      result: null,
      error: null,
      currentStep: "configure",
    }),
}));
