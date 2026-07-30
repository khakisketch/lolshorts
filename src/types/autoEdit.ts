/**
 * Auto-Edit TypeScript Types
 * Frontend Auto-Edit types consumed after IPC normalization in src/api/video.ts.
 */

// ========================================================================
// Canvas Template Types
// ========================================================================

export interface Position {
  x: number;  // 0-100 percentage
  y: number;  // 0-100 percentage
}

export type BackgroundLayer =
  | { type: 'Color'; value: string }        // Hex color: "#RRGGBB"
  | { type: 'Gradient'; value: string }     // Two colors: "#RRGGBB:#RRGGBB"
  | { type: 'Image'; path: string };        // File path to background image

export type CanvasElement =
  | {
      type: 'Text';
      content: string;
      font: string;      // Font file path or system font name
      size: number;      // Font size in pixels
      color: string;     // Hex color: "#RRGGBB"
      outline?: string;  // Optional outline color
      position: Position;
    }
  | {
      type: 'Image';
      path: string;      // File path to image
      width: number;     // Width in pixels
      height: number;    // Height in pixels
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
  game_audio: number;       // 0-100
  background_music: number; // 0-100
}

// ========================================================================
// Auto-Edit Configuration
// ========================================================================

export interface AutoEditConfig {
  game_ids: string[];               // List of game IDs to select clips from
  target_duration: number;          // 60, 120, or 180 seconds
  canvas_template?: CanvasTemplate; // Optional canvas overlay
  background_music?: BackgroundMusic; // Optional background music
  audio_levels?: AudioLevels;       // Optional audio mixing levels
  // Experimental: auto zoom-in on kill/event timestamps. Backend defaults to
  // false (serde default) when omitted.
  enable_event_zoom?: boolean;
  /**
   * 각 클립 앞머리 훅 자막. 생략하면 백엔드가 **켜진 것으로** 본다
   * (`#[serde(default = "default_true")]`) — 기본 결과물이 그대로 올릴 만해야 하므로.
   */
  enable_hook_captions?: boolean;
}

// ========================================================================
// Auto-Edit Progress & Result
// ========================================================================

export type AutoEditStatus =
  | 'Idle'
  | 'SelectingClips'
  | 'PreparingClips'
  | 'Concatenating'
  | 'ApplyingCanvas'
  | 'MixingAudio'
  | 'Complete'
  | 'Failed';

export interface AutoEditProgress {
  job_id: string;
  status: AutoEditStatus;
  progress_percentage: number;  // 0-100
  current_stage: string;        // Human-readable current stage
  clips_selected: number;
  total_clips: number;
  estimated_completion_seconds?: number;
  output_path?: string;
}

export interface AutoEditResult {
  job_id: string;
  output_path: string;
  duration: number;           // Actual duration in seconds
  clips_used: number;
  file_size_bytes: number;
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
  gameAudioVolume: number;      // 0-100
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

export type AutoEditStep = 'configure' | 'preview' | 'generating' | 'complete';

// ========================================================================
// Error Handling Types
// ========================================================================

export interface VideoError {
  message: string;                  // User-friendly error message
  error_type: string;               // Error category (e.g., "FileNotFound", "FfmpegNotFound")
  recovery_suggestions: string[];   // Actionable steps for recovery
  technical_details?: string;       // Optional technical information
}

// ========================================================================
// Quota Management Types
// ========================================================================

export interface AutoEditQuotaInfo {
  tier: string;           // User's subscription tier (FREE or PRO)
  is_pro: boolean;        // Whether user is PRO tier
  usage: number;          // Number of auto-edits used this month
  limit: number;          // Monthly limit (5 for FREE, u32::MAX for PRO)
  remaining: number;      // Remaining auto-edits this month
  month: string;          // Current month (YYYY-MM)
}

// ========================================================================
// Auto-Edit Results Storage Types
// ========================================================================

export type UploadStatus =
  | 'NotUploaded'
  | 'Queued'
  | 'Uploading'
  | 'Processing'
  | 'Completed'
  | 'Failed';

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
}
