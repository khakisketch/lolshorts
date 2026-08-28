/**
 * Auto-Edit TypeScript Types
 * Frontend Auto-Edit types consumed after IPC normalization in src/api/video.ts.
 */

// ========================================================================
// Canvas Template Types
// ========================================================================

export interface Position {
  x: number; // 0-100 percentage
  y: number; // 0-100 percentage
}

export type BackgroundLayer =
  | { type: "Color"; value: string } // Hex color: "#RRGGBB"
  | { type: "Gradient"; value: string } // Two colors: "#RRGGBB:#RRGGBB"
  | { type: "Image"; path: string }; // File path to background image

export type CanvasElement =
  | {
      type: "Text";
      content: string;
      font: string; // Font file path or system font name
      size: number; // Font size in pixels
      color: string; // Hex color: "#RRGGBB"
      outline?: string; // Optional outline color
      position: Position;
    }
  | {
      type: "Image";
      path: string; // File path to image
      width: number; // Width in pixels
      height: number; // Height in pixels
      position: Position;
    };

export interface CanvasTemplate {
  id: string;
  name: string;
  background: BackgroundLayer;
  elements: CanvasElement[];
}

export interface CanvasTemplateInfo {
  id: string;
  name: string;
  element_count: number;
}

// ========================================================================
// Audio Types
// ========================================================================

export interface BackgroundMusic {
  file_path: string;
  loop_music: boolean;
}

export interface AudioLevels {
  game_audio: number; // 0-100
  background_music: number; // 0-100
}

// ========================================================================
// Auto-Edit Configuration
// ========================================================================

export interface AutoEditConfig {
  game_ids: string[]; // List of game IDs to select clips from
  target_duration: number; // 60, 120, or 180 seconds
  /**
   * 화면에서 고른 클립의 파일 경로. 보내면 자동 선택 대신 **이것만** 쓴다.
   *
   * 경로가 사실상 PK 다 — 백엔드의 `selected_clip_ids` 는 로딩 순서로 매기는
   * 위치 카운터라 프론트가 지목할 수 없다(`auto_composer/types.rs` 참고).
   * 목표 길이는 이때 예산이 아니다: 사용자가 직접 고른 것이므로 다 넣는다.
   */
  selected_clip_paths?: string[];
  /** Exact, user-reviewed timeline. Cannot be combined with selected_clip_paths. */
  storyboard?: StoryboardClip[];
  output_intent?: AutoEditOutputIntent;
  framing_mode?: AutoEditFramingMode;
  platform_preset?: PlatformPreset;
  publish_metadata?: PublishMetadata;
  canvas_template?: CanvasTemplate; // Optional canvas overlay
  background_music?: BackgroundMusic; // Optional background music
  audio_levels?: AudioLevels; // Optional audio mixing levels
  // Experimental: auto zoom-in on kill/event timestamps. Backend defaults to
  // false (serde default) when omitted.
  enable_event_zoom?: boolean;
  /**
   * 각 클립 앞머리 훅 자막. 생략하면 백엔드가 **켜진 것으로** 본다
   * (`#[serde(default = "default_true")]`) — 기본 결과물이 그대로 올릴 만해야 하므로.
   */
  enable_hook_captions?: boolean;
  /**
   * 훅 자막을 구울 언어. 자막은 픽셀로 구워져서 나중에 UI 언어를 바꿔도 이미
   * 만든 영상은 안 바뀐다 — 그래서 요청 시점의 `i18next` 언어(`i18n.language`,
   * `ko-KR` 처럼 지역 서브태그가 붙을 수 있음)를 그대로 스냅샷해 보낸다.
   * 백엔드는 `ko` 로 시작하지 않으면 전부 영어로 본다(ko/en 둘만 사람이 다듬어
   * 유지되는 로케일 — 나머지 18개는 자막 문구가 없다).
   * 생략하면 백엔드가 영어로 본다(`CaptionLocale::default()`).
   */
  caption_locale?: string;
}

export interface StoryboardClip {
  game_id: string;
  file_path: string;
  order: number;
  trim_start_secs: number;
  trim_end_secs: number;
}

export type AutoEditOutputIntent =
  | "single_short"
  | "shorts_series"
  | "vertical_video";
export type AutoEditFramingMode =
  | "lol_focus_stack"
  | "safe_full_frame"
  | "center_crop";
export type PlatformPreset = "youtube_shorts" | "tiktok" | "instagram_reels";

export type OutputValidationStatus =
  | "valid"
  | "warning"
  | "invalid"
  | "unknown";
export interface OutputValidationIssue {
  code: string;
  severity: "warning" | "error";
  message: string;
}
export interface OutputValidationReport {
  contract_version: number;
  status: OutputValidationStatus;
  preset: PlatformPreset;
  probed_at: string;
  width: number;
  height: number;
  fps: number;
  is_cfr: boolean;
  sample_aspect_ratio: string;
  display_aspect_ratio: string;
  duration: number;
  video_codec: string;
  audio_codec: string | null;
  pixel_format: string;
  sample_rate: number | null;
  audio_channels: number | null;
  file_size_bytes: number;
  bitrate_bps: number;
  decode_smoke_passed: boolean;
  issues: OutputValidationIssue[];
}

export type MediaJobKind =
  | "auto_edit"
  | "platform_export"
  | "output_validation";
export type MediaJobStatus =
  | "queued"
  | "running"
  | "validating"
  | "paused"
  | "recoverable"
  | "complete"
  | "failed"
  | "discarded";
export interface MediaJobPart {
  part_index: number;
  part_count: number;
  status: MediaJobStatus;
  progress_percentage: number;
  trim_json: string;
  partial_path: string | null;
  output_path: string | null;
  validation: OutputValidationReport | null;
  file_fingerprint: string | null;
  attempt_count: number;
}
export interface MediaJobSnapshot {
  job_id: string;
  user_id: string;
  kind: MediaJobKind;
  status: MediaJobStatus;
  recoverable: boolean;
  current_stage: string;
  progress_percentage: number;
  config_json: string;
  parts: MediaJobPart[];
  error_code: string | null;
  error_message: string | null;
  retry_count: number;
  quota_sync_pending: boolean;
  created_at: string;
  updated_at: string;
}

export interface PublishMetadata {
  title: string;
  description: string;
  tags: string[];
  privacy_status: string;
}

export interface AutoEditPlanClip extends StoryboardClip {
  source_duration_secs: number;
  event_offset_secs?: number | null;
  event_type: string;
  highlight_score: number;
  recommended_order: number;
  thumbnail_path?: string | null;
}

export interface AutoEditPlan {
  clips: AutoEditPlanClip[];
  estimated_duration_secs: number;
  recommended_output_intent: AutoEditOutputIntent;
  estimated_part_count: number;
}

// ========================================================================
// Auto-Edit Progress & Result
// ========================================================================

export type AutoEditStatus =
  | "Idle"
  | "Queued"
  | "SelectingClips"
  | "PreparingClips"
  | "Concatenating"
  | "ApplyingCanvas"
  | "MixingAudio"
  | "Complete"
  | "Failed"
  | "Cancelled";

export interface AutoEditJobReceipt {
  job_id: string;
  status: AutoEditStatus;
}

export type AutoEditOutputKind =
  | "short"
  | "short_series_part"
  | "vertical_video";

export interface AutoEditOutput {
  result_id: string;
  output_path: string;
  duration: number;
  clips_used: number;
  file_size_bytes: number;
  output_kind: AutoEditOutputKind;
  part_index?: number | null;
  part_count?: number | null;
}

export interface AutoEditProgress {
  job_id: string;
  status: AutoEditStatus;
  progress_percentage: number; // 0-100
  current_stage: string; // Human-readable current stage
  clips_selected: number;
  total_clips: number;
  estimated_completion_seconds?: number;
  output_path?: string;
  outputs: AutoEditOutput[];
  error?: string;
}

export interface AutoEditResult {
  job_id: string;
  output_path: string;
  duration: number; // Actual duration in seconds
  clips_used: number;
  file_size_bytes: number;
  outputs?: AutoEditOutput[];
}

// ========================================================================
// Frontend-Only Types (UI State)
// ========================================================================

export interface AutoEditMetadata {
  title: string;
  caption: string;
  tags: string[];
}

export interface ShortsReadiness {
  isReady: boolean;
  checks: {
    duration: { label: string; passed: boolean; message: string };
    clipCount: { label: string; passed: boolean; message: string };
    template: { label: string; passed: boolean; message: string };
    music: { label: string; passed: boolean; message: string };
  };
}

export interface GameSelection {
  game_id: string;
  champion: string;
  game_mode: string;
  date: string;
  clip_count: number;
  selected: boolean;
}

export type DurationOption = 60 | 120 | 180;

export interface AudioMixerState {
  gameAudioVolume: number; // 0-100
  backgroundMusicVolume: number; // 0-100
  musicFile: File | null;
  loopMusic: boolean;
}

export interface CanvasEditorState {
  currentTemplate: CanvasTemplate | null;
  availableTemplates: CanvasTemplateInfo[];
  isEditing: boolean;
  selectedElementIndex: number | null;
}

export type AutoEditStep =
  | "configure"
  | "storyboard"
  | "preview"
  | "generating"
  | "complete";

// ========================================================================
// Error Handling Types
// ========================================================================

export interface VideoError {
  message: string; // User-friendly error message
  error_type: string; // Error category (e.g., "FileNotFound", "FfmpegNotFound")
  recovery_suggestions: string[]; // Actionable steps for recovery
  technical_details?: string; // Optional technical information
}

// ========================================================================
// Quota Management Types
// ========================================================================

export interface AutoEditQuotaInfo {
  tier: string; // User's subscription tier (FREE or PRO)
  is_pro: boolean; // Whether user is PRO tier
  usage: number; // Number of auto-edits used this month
  limit: number; // Monthly limit (5 for FREE, u32::MAX for PRO)
  remaining: number; // Remaining auto-edits this month
  month: string; // Current month (YYYY-MM)
}

// ========================================================================
// Auto-Edit Results Storage Types
// ========================================================================

export type UploadStatus =
  | "NotUploaded"
  | "Queued"
  | "Uploading"
  | "Processing"
  | "Completed"
  | "Failed";

export interface YouTubeUploadStatus {
  video_id: string | null;
  status: UploadStatus;
  upload_started_at: string | null;
  upload_completed_at: string | null;
  progress: number;
  error: string | null;
}

export interface AutoEditResultMetadata {
  result_id: string;
  job_id: string;
  output_path: string;
  thumbnail_path: string | null;
  created_at: string;
  duration: number;
  clip_count: number;
  game_ids: string[];
  target_duration: number;
  canvas_template_name: string | null;
  has_background_music: boolean;
  youtube_status: YouTubeUploadStatus | null;
  file_size_bytes: number;
  publish_title?: string;
  publish_description?: string;
  publish_tags?: string[];
  publish_privacy_status?: string;
  output_intent?: string;
  framing_mode?: string;
  platform_preset?: string;
  series_id?: string;
  part_index?: number;
  part_count?: number;
  output_kind?: AutoEditOutputKind | string;
  validation?: OutputValidationReport | null;
  platform_exports?: PlatformExportMetadata[];
}

export interface PlatformExportMetadata {
  export_id: string;
  job_id: string;
  result_id: string;
  preset: PlatformPreset;
  output_path: string;
  passthrough: boolean;
  owns_file: boolean;
  created_at: string;
  validation: OutputValidationReport;
}

export interface AutoEditResultGroup {
  series_id: string;
  job_id: string;
  output_intent: AutoEditOutputIntent;
  outputs: AutoEditResultMetadata[];
  total_duration: number;
  total_file_size_bytes: number;
  validation_status: OutputValidationStatus;
}
