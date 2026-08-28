// Event Filter Settings
//
// 백엔드 `EventFilterSettings`(settings/models.rs)의 화면쪽 대응물. 전체 필드
// 미러는 `components/settings/highlightPreset.ts` 의 `CanonicalEventFilter` 이고,
// 이 타입은 **화면이 실제로 읽고 쓰는 것**만 담는다.
export interface EventFilterSettings {
  record_kills: boolean;
  record_multikills: boolean;
  record_first_blood: boolean;
  /** 연속킬 중인 상대를 잡은 것 — 내가 딴 킬이다(데스 계열이 아니다). */
  record_shutdown: boolean;
  /** 10초 안에 솔로킬 2개 이상 — 1vX. */
  record_outplay: boolean;
  /** 체력이 바닥인 채로 따낸 킬. */
  record_low_hp: boolean;
  record_deaths: boolean;
  /** 킬 직후 5초 안에 죽은 것. 죽는 이벤트라 `record_deaths` 가 부모다. */
  record_trade_kill: boolean;
  /** 내가 퍼블을 당한 것. 마찬가지로 `record_deaths` 가 부모다. */
  record_first_blood_victim: boolean;
  record_assists: boolean;
  record_dragon: boolean;
  record_baron: boolean;
  record_elder: boolean;
  record_herald: boolean;
  record_voidgrubs: boolean;
  record_atakhan: boolean;
  record_turret: boolean;
  record_inhibitor: boolean;
  /**
   * 백엔드에 **소비처가 없다** — 어떤 트리거도 이 플래그를 보지 않는다.
   * 화면에 노출하지 않는 이유는 `settingSpecs.ts` 의 `SCENE_FLAGS` 주석 참조.
   * 설정 파일 왕복에서 값을 잃지 않으려고 타입에는 남긴다.
   */
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
  /** Persisted Rust settings schema; retained in the IPC contract for migrations. */
  schema_version: number;
  video: VideoSettings;
  audio: AudioSettings;
  event_filter: EventFilterSettings;
  game_mode: GameModeSettings;
  clip_timing: ClipTimingSettings;
  hotkeys: HotkeySettings;
  storage: StorageSettings;
  launch_on_windows_startup: boolean;
  minimize_to_tray: boolean;
  show_notifications: boolean;
  show_replay_popup: boolean;
  crash_reporting_enabled: boolean;
  overlay_enabled: boolean;
}

// Recording Readiness Types
export interface ReadinessComponent {
  status: "ok" | "warning" | "error";
  message: string;
}

export interface ReadinessBlocker {
  id: string;
  component?: string;
  message: string;
  action?: string;
  severity: "critical" | "warning";
}

export interface RecordingReadiness {
  ready: boolean;
  blockers: ReadinessBlocker[];
  component_statuses: {
    ffmpeg: ReadinessComponent;
    ffprobe: ReadinessComponent;
    audio: ReadinessComponent;
    disk: ReadinessComponent;
    lcu: ReadinessComponent;
    gpu: ReadinessComponent;
    nvenc: ReadinessComponent;
    capture?: ReadinessComponent;
    overlay_exclusion?: ReadinessComponent;
    release_config?: ReadinessComponent;
    supabase?: ReadinessComponent;
    youtube?: ReadinessComponent;
    updater?: ReadinessComponent;
    telemetry?: ReadinessComponent;
    autostart?: ReadinessComponent;
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

// PlatformConfig IPC contract (matches Rust platform_config/types.rs).
export type PlatformKind = "Windows" | "MacOS" | "Linux";
export type HardwareGpuVendor =
  | "Nvidia"
  | "Amd"
  | "Intel"
  | "Apple"
  | "Unknown";

export interface CpuInfo {
  model: string;
  cores: number;
  logical_cores: number;
  max_frequency: number;
  has_avx: boolean;
  has_avx2: boolean;
  architecture: string;
}

export interface GpuInfo {
  name: string;
  vendor: HardwareGpuVendor;
  memory_mb: number;
  driver_version: string;
  is_primary: boolean;
  supports_encoding: boolean;
  supports_nvenc: boolean;
  supports_amf: boolean;
  supports_qsv: boolean;
}

export interface MemoryInfo {
  total_gb: number;
  available_gb: number;
  speed_mhz: number | null;
}

export interface DisplayInfo {
  id: string;
  name: string;
  resolution: [number, number];
  refresh_rate: number;
  is_primary: boolean;
  scaling_factor: number;
}

export interface AudioDeviceInfo {
  input_devices: AudioDeviceInfoItem[];
  output_devices: AudioDeviceInfoItem[];
  default_input: string | null;
  default_output: string | null;
}

export interface AudioDeviceInfoItem {
  id: string;
  name: string;
  channels: number;
  sample_rate: number;
  is_default: boolean;
}

export interface PlatformStorageInfo {
  total_space_gb: number;
  free_space_gb: number;
  install_drive: string;
  temp_directory: string;
}

export interface HardwareCapabilities {
  cpu: CpuInfo;
  gpu: GpuInfo[];
  memory: MemoryInfo;
  displays: DisplayInfo[];
  audio_devices: AudioDeviceInfo;
  storage: PlatformStorageInfo;
}

export interface PlatformFeatures {
  supports_windows_capture: boolean;
  supports_ffmpeg_native: boolean;
  supports_core_graphics: boolean;
  supports_hardware_acceleration: boolean;
  supports_system_tray: boolean;
  supports_global_hotkeys: boolean;
  supports_file_associations: boolean;
  supports_auto_start: boolean;
  supports_notifications: boolean;
  supports_api_detection: boolean;
  supports_window_enumeration: boolean;
}

export interface PlatformConfig {
  platform: PlatformKind;
  hardware: HardwareCapabilities;
  default_overrides: RecordingSettings;
  features: PlatformFeatures;
  recommended_settings: RecommendedSettings;
}
