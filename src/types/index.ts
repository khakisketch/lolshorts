// Event Filter Settings
export interface EventFilterSettings {
  record_kills: boolean;
  record_multikills: boolean;
  record_first_blood: boolean;
  record_deaths: boolean;
  record_shutdown: boolean;
  record_assists: boolean;
  record_dragon: boolean;
  record_baron: boolean;
  record_elder: boolean;
  record_herald: boolean;
  record_turret: boolean;
  record_inhibitor: boolean;
  record_nexus: boolean;
  record_ace: boolean;
  record_game_end: boolean;
  record_steal: boolean;
  min_priority: number;
}

// Game Mode Settings (from components/settings/GameModeSettings.tsx)
export interface GameModeSettings {
  record_ranked_solo: boolean;
  record_ranked_flex: boolean;
  record_normal: boolean;
  record_quick_play: boolean;
  record_aram: boolean;
  record_arena: boolean;
  record_special: boolean;
  record_custom: boolean;
  record_practice: boolean;
}

// Video Settings (from components/settings/VideoSettings.tsx)
export type Resolution = "r1920x1080" | "r2560x1440" | "r3840x2160";
export type FrameRate = "fps30" | "fps60" | "fps120" | "fps144";
export type BitratePreset = "low" | "medium" | "high" | "very_high";
export type VideoCodec = "h264" | "h265" | "av1";
export type EncoderPreference = "auto" | "nvenc" | "qsv" | "amf" | "software";

export interface VideoSettings {
  resolution: Resolution;
  frame_rate: FrameRate;
  bitrate_preset: BitratePreset;
  codec: VideoCodec;
  encoder: EncoderPreference;
}

// Audio Settings (from components/settings/AudioSettings.tsx)
export type SampleRate = "hz44100" | "hz48000";
export type AudioBitrate = "kbps128" | "kbps192" | "kbps256" | "kbps320";

export interface AudioSettings {
  record_microphone: boolean;
  microphone_device: string | null;
  microphone_volume: number; // 0-200
  record_system_audio: boolean;
  system_audio_device: string | null;
  system_audio_volume: number; // 0-200
  sample_rate: SampleRate;
  bitrate: AudioBitrate;
}

// Clip Timing Settings (from components/settings/ClipTimingSettings.tsx)
export interface EventTiming {
  pre_duration: number;
  post_duration: number;
}

export interface ClipTimingSettings {
  default_pre_duration: number;
  default_post_duration: number;
  event_timings: Record<string, EventTiming>;
  merge_consecutive_events: boolean;
  merge_time_threshold: number;
}

// Hotkey Settings (from components/settings/HotkeySettings.tsx)
export interface HotkeySettings {
  manual_save_clip: string;
  toggle_recording: string;
  delete_last_clip: string;
}

// Storage Settings
export interface StorageSettings {
  auto_delete_enabled: boolean;
  auto_delete_days: number;
  max_storage_gb: number;
  delete_exported_clips: boolean;
}

// Main Recording Settings Type
export interface RecordingSettings {
  video: VideoSettings;
  audio: AudioSettings;
  event_filter: EventFilterSettings;
  game_mode: GameModeSettings;
  clip_timing: ClipTimingSettings;
  hotkeys: HotkeySettings;
  storage: StorageSettings;
  auto_start_with_league: boolean;
  minimize_to_tray: boolean;
  show_notifications: boolean;
  show_replay_popup: boolean;
  crash_reporting_enabled: boolean;
  overlay_enabled: boolean;
}

// Recording Readiness Types
export interface ReadinessComponent {
  status: 'ok' | 'warning' | 'error';
  message: string;
}

export interface ReadinessBlocker {
  id: string;
  component?: string;
  message: string;
  action?: string;
  severity: 'critical' | 'warning';
}

export interface RecordingReadiness {
  ready: boolean;
  blockers: ReadinessBlocker[];
  component_statuses: {
    ffmpeg: ReadinessComponent;
    audio: ReadinessComponent;
    disk: ReadinessComponent;
    lcu: ReadinessComponent;
    gpu: ReadinessComponent;
  };
}

// UI component helper type (used in RecordingControls.tsx for simplified settings)
export type VideoQuality = "low" | "medium" | "high" | "ultra";

// Recommended Settings (matches Rust RecommendedSettings in platform_config/types.rs)
export interface VideoRecommendations {
  recommended_encoder: EncoderPreference;
  recommended_codec: string;
  recommended_bitrate_kbps: number;
  recommended_resolution: string;
  recommended_frame_rate: string;
  maximum_recording_hours: number;
}

export interface AudioRecommendations {
  recommended_sample_rate: string;
  recommended_bitrate: string;
  max_channels: number;
  enable_microphone_by_default: boolean;
  enable_system_audio_by_default: boolean;
}

export interface PerformanceRecommendations {
  enable_hardware_acceleration: boolean;
  recommended_buffer_size_mb: number;
  recommended_temp_cleanup_interval_minutes: number;
  recommended_concurrent_clips: number;
  enable_performance_monitoring: boolean;
}

export interface StorageRecommendations {
  recommended_clips_directory: string;
  minimum_free_space_gb: number;
  recommended_cleanup_threshold_gb: number;
  enable_auto_cleanup: boolean;
}

export interface RecommendedSettings {
  video: VideoRecommendations;
  audio: AudioRecommendations;
  performance: PerformanceRecommendations;
  storage: StorageRecommendations;
}
