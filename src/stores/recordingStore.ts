import { create } from 'zustand';
import { recordingApi, RecordingStatus as ApiRecordingStatus, isRecording as apiIsRecording } from '../api/recording';
import { RecordingSettings, RecordingReadiness } from '@/types';
import { EVENT_FILTER_DEFAULTS } from '@/components/settings/highlightPreset';

export interface RecordingStatus {
  isRecording: boolean;
  startTime: number | null;
  duration: number;
  gameProcessDetected: boolean;
  lcuConnected: boolean;
  state: ApiRecordingStatus['status'];
}

export interface RecordingStore {
  // State
  status: RecordingStatus;
  readiness: RecordingReadiness | null;
  settings: RecordingSettings;
  error: string | null;
  /**
   * Whether system audio is currently being captured. `null` when unknown
   * (not recording, or the last performance-stats fetch failed).
   */
  audioActive: boolean | null;
  /**
   * Whether the microphone is currently being captured. `null` when unknown
   * (not recording, or the last performance-stats fetch failed).
   */
  micActive: boolean | null;
  _pollInterval: number | null;

  // Actions
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  updateSettings: (settings: Partial<RecordingSettings>) => void;
  resetStatus: () => void;

  // Synchronization
  syncStatus: () => Promise<void>;
  startStatusPolling: () => void;
  stopStatusPolling: () => void;
}

// Default settings matching Rust backend defaults
const DEFAULT_SETTINGS: RecordingSettings = {
  video: {
    resolution: "r1920x1080",
    frame_rate: "fps60",
    bitrate_preset: "high",
    codec: "h264",
    encoder: "auto"
  },
  audio: {
    record_microphone: false,
    microphone_device: null,
    microphone_volume: 100,
    record_system_audio: true,
    system_audio_device: "default",
    system_audio_volume: 100,
    sample_rate: "hz48000",
    bitrate: "kbps192"
  },
  // 백엔드 `EventFilterSettings::default()` 미러를 그대로 쓴다.
  //
  // 손으로 적어 두었던 동안 이 표는 백엔드와 세 군데가 달랐다(어시스트 off,
  // 포탑 on, 문턱 2). 설정이 도착하기 전 잠깐 보이는 값이지만, 그 잠깐 동안
  // 기본 설정 화면은 어떤 프리셋과도 맞지 않아 "직접 설정" 배지를 달았다.
  event_filter: { ...EVENT_FILTER_DEFAULTS },
  game_mode: {
    record_ranked_solo: true,
    record_ranked_flex: true,
    record_normal: true,
    record_quick_play: true,
    record_aram: true,
    record_arena: true,
    record_special: true,
    record_custom: false,
    record_practice: false
  },
  clip_timing: {
    default_pre_duration: 15,
    default_post_duration: 5,
    event_timings: {},
    merge_consecutive_events: true,
    merge_time_threshold: 10
  },
  hotkeys: {
    manual_save_clip: "F9",
    toggle_recording: "F8",
    delete_last_clip: "F10"
  },
  storage: {
    auto_delete_enabled: false,
    auto_delete_days: 30,
    max_storage_gb: 50,
    delete_exported_clips: false
  },
  auto_start_with_league: true,
  minimize_to_tray: true,
  show_notifications: true,
  show_replay_popup: true,
  crash_reporting_enabled: false,
  overlay_enabled: true
};

export const useRecordingStore = create<RecordingStore>((set, get) => ({
  status: {
    isRecording: false,
    startTime: null,
    duration: 0,
    gameProcessDetected: false,
    lcuConnected: false,
    state: 'idle',
  },
  readiness: null,
  settings: DEFAULT_SETTINGS,
  error: null,
  audioActive: null,
  micActive: null,
  _pollInterval: null,

  startRecording: async () => {
    try {
      set({ error: null });
      await recordingApi.start();
      // Status will be updated by the next poll or sync
      await get().syncStatus();
    } catch (e) {
      const errorMessage = e instanceof Error ? e.message : 'Failed to start recording';
      set({ error: errorMessage });
      throw e;
    }
  },

  stopRecording: async () => {
    try {
      set({ error: null });
      await recordingApi.stop();
      // Status will be updated by the next poll or sync
      await get().syncStatus();
    } catch (e) {
      const errorMessage = e instanceof Error ? e.message : 'Failed to stop recording';
      set({ error: errorMessage });
      throw e;
    }
  },



  updateSettings: (newSettings) => {
    set((state) => ({
      settings: {
        ...state.settings,
        ...newSettings,
      }
    }));
  },

  resetStatus: () => {
    set({
      status: {
        isRecording: false,
        startTime: null,
        duration: 0,
        gameProcessDetected: false,
        lcuConnected: false,
        state: 'idle',
      },
      readiness: null,
      error: null,
      audioActive: null,
      micActive: null,
    });
  },

  syncStatus: async () => {
    try {
      const [backendStatus, readiness] = await Promise.all([
        recordingApi.getStatus(),
        recordingApi.getRecordingReadiness(),
      ]);

      const recording = apiIsRecording(backendStatus);

      // Only probe performance stats while actively recording - audio
      // capture health is meaningless (and not worth the extra IPC call)
      // when idle.
      let audioActive: boolean | null = null;
      let micActive: boolean | null = null;
      if (recording) {
        try {
          const perf = await recordingApi.getPerformanceStats();
          audioActive = perf.recording.audio_active;
          micActive = perf.recording.mic_active;
        } catch {
          audioActive = null;
          micActive = null;
        }
      }

      set((state) => ({
        error: null,
        readiness,
        audioActive,
        micActive,
        status: {
          ...state.status,
          isRecording: recording,
          state: backendStatus.status,
          // Note: start_time not available from backend, keep existing value
          duration: backendStatus.buffer_duration_secs,
        }
      }));
    } catch (e) {
      const errorMessage = e instanceof Error ? e.message : 'Failed to sync recording status';
      set((state) => ({
        error: errorMessage,
        audioActive: null,
        micActive: null,
        status: {
          ...state.status,
          isRecording: false,
          state: 'error',
        }
      }));
    }
  },


  startStatusPolling: () => {
    if (get()._pollInterval) return;
    get().syncStatus(); // Initial sync
    const id = window.setInterval(() => {
      get().syncStatus();
    }, 1000);
    set({ _pollInterval: id });
  },

  stopStatusPolling: () => {
    const id = get()._pollInterval;
    if (id) {
      clearInterval(id);
      set({ _pollInterval: null });
    }
  }
}));
