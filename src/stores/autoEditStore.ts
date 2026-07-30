import { create } from 'zustand';
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
} from '@/types/autoEdit';

interface AutoEditStore {
  // Step flow
  currentStep: AutoEditStep;
  setCurrentStep: (step: AutoEditStep) => void;

  // Game selection
  availableGames: GameSelection[];
  selectedGameIds: string[];
  setAvailableGames: (games: GameSelection[]) => void;
  toggleGameSelection: (gameId: string) => void;
  clearGameSelection: () => void;

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
    return localStorage.getItem('i18nextLng') ?? 'en';
  } catch {
    return 'en';
  }
}

export const useAutoEditStore = create<AutoEditStore>((set, get) => ({
  // Initial state
  currentStep: 'configure',

  availableGames: [],
  selectedGameIds: [],

  targetDuration: 60,

  enableEventZoom: false,
  enableHookCaptions: true,

  currentTemplate: null,
  availableTemplates: [],
  isEditingCanvas: false,

  backgroundMusic: null,
  audioLevels: DEFAULT_AUDIO_LEVELS,

  metadata: {
    title: '',
    caption: '',
    tags: [],
  },

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
        ? selectedGameIds.filter(id => id !== gameId)
        : [...selectedGameIds, gameId],
    });
  },

  clearGameSelection: () => set({ selectedGameIds: [] }),

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

  setAudioLevels: (levels) => set({
    audioLevels: { ...get().audioLevels, ...levels },
  }),

  clearAudio: () => set({
    backgroundMusic: null,
    audioLevels: DEFAULT_AUDIO_LEVELS,
  }),

  // Progress & Result
  setJobId: (id) => set({ jobId: id }),

  setProgress: (progress) => set({ progress }),

  setResult: (result) => set({ result }),

  setError: (error) => set({ error }),

  setMetadata: (metadata) => set({
    metadata: { ...get().metadata, ...metadata },
  }),

  // Build final config for backend
  buildConfig: (): AutoEditConfig => {
    const {
      selectedGameIds,
      targetDuration,
      currentTemplate,
      backgroundMusic,
      audioLevels,
      enableEventZoom,
      enableHookCaptions,
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
    };

    if (currentTemplate) {
      config.canvas_template = currentTemplate;
    }

    if (backgroundMusic) {
      config.background_music = backgroundMusic;
    }

    return config;
  },

  // Reset all state
  resetAll: () => set({
    currentStep: 'configure',
    selectedGameIds: [],
    targetDuration: 60,
    enableEventZoom: false,
    currentTemplate: null,
    isEditingCanvas: false,
    backgroundMusic: null,
    audioLevels: DEFAULT_AUDIO_LEVELS,
    metadata: {
      title: '',
      caption: '',
      tags: [],
    },
    jobId: null,
    progress: null,
    result: null,
    error: null,
  }),

  // Reset only progress/result (for new generation)
  resetProgress: () => set({
    jobId: null,
    progress: null,
    result: null,
    error: null,
    currentStep: 'configure',
  }),
}));
