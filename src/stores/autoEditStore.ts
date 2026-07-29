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

export const useAutoEditStore = create<AutoEditStore>((set, get) => ({
  // Initial state
  currentStep: 'configure',

  availableGames: [],
  selectedGameIds: [],

  targetDuration: 60,

  enableEventZoom: false,

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
    } = get();

    const config: AutoEditConfig = {
      game_ids: selectedGameIds,
      target_duration: targetDuration,
      enable_event_zoom: enableEventZoom,
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
