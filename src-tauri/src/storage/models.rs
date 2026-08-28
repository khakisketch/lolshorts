#![allow(clippy::upper_case_acronyms)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::video::auto_composer::{AutoEditOutputIntent, PlatformPreset};
use crate::video::output_validation::{OutputValidationReport, OutputValidationStatus};

/// Game metadata stored in the local SQLite database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameMetadata {
    pub game_id: String,
    pub champion: String,
    pub game_mode: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub result: Option<GameResult>,
    pub kda: Option<KDA>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameResult {
    Win,
    Loss,
    Remake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KDA {
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
}

impl KDA {
    pub fn ratio(&self) -> f64 {
        match self.deaths {
            0 => (self.kills + self.assists) as f64,
            deaths => (self.kills + self.assists) as f64 / deaths as f64,
        }
    }
}

/// Event data stored in the local SQLite database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    pub event_id: u64,
    pub event_type: EventType,
    pub timestamp: f64, // Game time in seconds
    pub priority: u8,   // 1-5, higher is more important
    pub participants: Vec<String>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    ChampionKill,
    Multikill(u8), // 2=double, 3=triple, 4=quadra, 5=penta
    TurretKill,
    InhibitorKill,
    DragonKill,
    BaronKill,
    Ace,
    FirstBlood,
    Custom(String),
}

impl EventType {
    pub fn default_priority(&self) -> u8 {
        match self {
            EventType::ChampionKill => 1,
            EventType::Multikill(kills) => match kills {
                2 => 2, // Double kill
                3 => 3, // Triple kill
                4 => 4, // Quadra kill
                5 => 5, // Penta kill
                _ => 3, // Other multikills
            },
            EventType::TurretKill => 2,
            EventType::InhibitorKill => 3,
            EventType::DragonKill => 3,
            EventType::BaronKill => 4,
            EventType::Ace => 4,
            EventType::FirstBlood => 3,
            EventType::Custom(_) => 2,
        }
    }
}

/// Clip metadata stored in the local SQLite database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipMetadata {
    pub file_path: String,
    pub thumbnail_path: Option<String>,
    pub event_type: EventType,
    pub event_time: f64, // Game time when event occurred
    pub priority: u8,
    pub duration: f64, // Clip duration in seconds
    /// 클립 **안에서** 하이라이트가 일어나는 지점(초).
    ///
    /// 이 값이 없던 동안 하류가 전부 "중앙 = 하이라이트"로 가정했고, 그래서
    /// 썸네일은 아무 일도 없는 프레임을 찍었고(13초 킬 클립의 6.5초 지점 —
    /// 킬은 10초에 있다) 이벤트 줌도 빌드업 구간에 걸렸다. 저장 시점에는 이미
    /// 알고 있는 값(`pre_duration`)인데 버리고 있었다.
    ///
    /// 예전 클립에는 없으므로 `default`. 없으면 소비하는 쪽이 중앙으로 되돌아간다.
    #[serde(default)]
    pub event_offset_secs: Option<f64>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub usage_count: u32, // Number of times this clip has been used in auto-edits
    /// 이 클립의 하이라이트 점수(`recording::highlight_score`).
    ///
    /// 자동 편집이 무엇을 먼저 쓸지 정하는 값이다. 그 전까지 정렬 기준은
    /// `priority` (1~5) 뿐이었는데, 그 눈금으로는 **퍼블·바론·게임종료가 전부
    /// 3점으로 동급**이라 순서가 사실상 무작위였다. 점수 눈금은 0~100 이고
    /// 상황 배수(체력·단독·열세·시점)가 곱해지므로 상한은 없다.
    ///
    /// 예전 클립에는 없다(`None`). 소비하는 쪽이 `priority` 로 되돌아간다.
    #[serde(default)]
    pub highlight_score: Option<f64>,
    /// 점수가 그렇게 나온 이유. 숫자가 아니라 이쪽이 화면에 나갈 값이다
    /// ("혼자서 · 1v3 · 체력 8%").
    #[serde(default)]
    pub score_reasons: Vec<crate::recording::highlight_score::ScoreReason>,
}

/// Ordering accepted by the paged clip-vault IPC endpoint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClipVaultSort {
    Best,
    Newest,
}

/// A non-empty game's clips as displayed in the clip vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipVaultGameGroup {
    pub game_id: String,
    pub game: Option<GameMetadata>,
    pub clips: Vec<ClipMetadata>,
    pub clip_count: usize,
}

/// One cursor-paginated clip-vault response. `next_cursor` is opaque to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipVaultPage {
    pub groups: Vec<ClipVaultGameGroup>,
    pub next_cursor: Option<String>,
    pub skipped_item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipVaultPageInput {
    pub sort: ClipVaultSort,
    pub cursor: Option<String>,
    pub game_limit: Option<usize>,
    /// Optional case-insensitive search across a game's champion, mode, and id.
    #[serde(default)]
    pub query: Option<String>,
    /// Optional exact game-mode filter.
    #[serde(default)]
    pub game_mode: Option<String>,
}

// ============================================================================
// Auto-Edit Usage Tracking (Quota System)
// ============================================================================

/// Auto-edit usage tracking for quota enforcement
///
/// Tracks monthly usage to enforce:
/// - FREE tier: 5 auto-edits per month
/// - PRO tier: Unlimited auto-edits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEditUsage {
    /// Month identifier (YYYY-MM format, e.g., "2025-01")
    pub month: String,

    /// Number of auto-edits used this month
    pub usage_count: u32,

    /// Last time the usage was updated
    pub last_updated: DateTime<Utc>,

    /// When this month's tracking period started
    pub period_start: DateTime<Utc>,
}

impl Default for AutoEditUsage {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            month: now.format("%Y-%m").to_string(),
            usage_count: 0,
            last_updated: now,
            period_start: now,
        }
    }
}

impl AutoEditUsage {
    /// Get current month identifier
    pub fn current_month() -> String {
        Utc::now().format("%Y-%m").to_string()
    }

    /// Check if this usage record is for the current month
    pub fn is_current_month(&self) -> bool {
        self.month == Self::current_month()
    }

    /// Reset usage for new month
    pub fn reset_for_month(month: String) -> Self {
        let now = Utc::now();
        Self {
            month,
            usage_count: 0,
            last_updated: now,
            period_start: now,
        }
    }
}

// ============================================================================
// Durable media jobs
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaJobKind {
    AutoEdit,
    PlatformExport,
    OutputValidation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaJobStatus {
    Queued,
    Running,
    Validating,
    Paused,
    Recoverable,
    Complete,
    Failed,
    Discarded,
}

impl MediaJobStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        use MediaJobStatus::*;
        matches!(
            (self, next),
            (Queued, Running | Paused | Failed | Discarded)
                | (Running, Validating | Paused | Recoverable | Failed)
                | (
                    Validating,
                    Running | Complete | Paused | Recoverable | Failed
                )
                | (Paused, Queued | Running | Discarded)
                | (Recoverable, Queued | Running | Discarded | Failed)
                | (Failed, Discarded)
                | (Complete, Complete)
                | (Discarded, Discarded)
        )
    }

    pub fn is_recoverable(self) -> bool {
        matches!(self, Self::Paused | Self::Recoverable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaJobPart {
    pub part_index: usize,
    pub part_count: usize,
    pub status: MediaJobStatus,
    pub progress_percentage: f64,
    pub trim_json: String,
    pub partial_path: Option<String>,
    pub output_path: Option<String>,
    pub validation: Option<OutputValidationReport>,
    pub file_fingerprint: Option<String>,
    pub attempt_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaJobSnapshot {
    pub job_id: String,
    pub user_id: String,
    pub kind: MediaJobKind,
    pub status: MediaJobStatus,
    pub recoverable: bool,
    pub current_stage: String,
    pub progress_percentage: f64,
    pub config_json: String,
    pub parts: Vec<MediaJobPart>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub quota_sync_pending: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformExportMetadata {
    pub export_id: String,
    pub job_id: String,
    pub result_id: String,
    pub preset: PlatformPreset,
    pub output_path: String,
    pub passthrough: bool,
    pub owns_file: bool,
    pub created_at: DateTime<Utc>,
    pub validation: OutputValidationReport,
}

// ============================================================================
// Auto-Edit Result Storage
// ============================================================================

/// Auto-edit result metadata for displaying in Results tab
///
/// Stores information about completed auto-edit videos to enable:
/// - Results browsing and playback
/// - Re-upload or delete operations
/// - YouTube upload status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEditResultMetadata {
    /// Unique result ID
    pub result_id: String,

    /// Job ID from auto-composition
    pub job_id: String,

    /// Path to the final video file
    pub output_path: String,

    /// Path to thumbnail (generated from video)
    pub thumbnail_path: Option<String>,

    /// When this auto-edit was created
    pub created_at: DateTime<Utc>,

    /// Total duration of the video (seconds)
    pub duration: f64,

    /// Number of clips used
    pub clip_count: usize,

    /// Game IDs included in this auto-edit
    pub game_ids: Vec<String>,

    /// Target duration requested (60, 120, or 180)
    pub target_duration: u32,

    /// Canvas template used (if any)
    pub canvas_template_name: Option<String>,

    /// Whether background music was used
    pub has_background_music: bool,

    /// YouTube upload status (if uploaded)
    pub youtube_status: Option<YouTubeUploadStatus>,

    /// File size in bytes
    pub file_size_bytes: u64,

    #[serde(default)]
    pub publish_title: String,

    #[serde(default)]
    pub publish_description: String,

    #[serde(default)]
    pub publish_tags: Vec<String>,

    #[serde(default = "default_upload_privacy")]
    pub publish_privacy_status: String,

    #[serde(default)]
    pub output_intent: String,

    #[serde(default)]
    pub framing_mode: String,

    #[serde(default)]
    pub platform_preset: String,

    /// Stable grouping contract. Legacy rows default to one standalone result;
    /// filenames are deliberately never parsed to infer a series.
    #[serde(default)]
    pub series_id: String,

    #[serde(default = "default_part_index")]
    pub part_index: usize,

    #[serde(default = "default_part_count")]
    pub part_count: usize,

    #[serde(default)]
    pub output_kind: String,

    #[serde(default)]
    pub validation: Option<OutputValidationReport>,

    #[serde(default)]
    pub platform_exports: Vec<PlatformExportMetadata>,
}

fn default_upload_privacy() -> String {
    "unlisted".to_string()
}

fn default_part_index() -> usize {
    1
}

fn default_part_count() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEditResultGroup {
    pub series_id: String,
    pub job_id: String,
    pub output_intent: AutoEditOutputIntent,
    pub outputs: Vec<AutoEditResultMetadata>,
    pub total_duration: f64,
    pub total_file_size_bytes: u64,
    pub validation_status: OutputValidationStatus,
}

/// YouTube upload status for auto-edit result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YouTubeUploadStatus {
    /// YouTube video ID (once uploaded)
    pub video_id: Option<String>,

    /// Upload status
    pub status: UploadStatus,

    /// When upload started
    pub upload_started_at: Option<DateTime<Utc>>,

    /// When upload completed
    pub upload_completed_at: Option<DateTime<Utc>>,

    /// Upload progress (0-100)
    pub progress: f64,

    /// Error message if upload failed
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum UploadStatus {
    #[serde(alias = "notuploaded")]
    NotUploaded,
    #[serde(alias = "queued")]
    Queued,
    #[serde(alias = "uploading")]
    Uploading,
    #[serde(alias = "processing")]
    Processing,
    #[serde(alias = "completed")]
    Completed,
    #[serde(alias = "failed")]
    Failed,
}

// ============================================================================
// Dashboard Statistics
// ============================================================================

/// Storage statistics for dashboard display
///
/// Provides quick overview of:
/// - Total number of games recorded
/// - Total number of clips created
/// - Total storage space used
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageStats {
    /// Total number of games with recorded clips
    pub total_games: usize,

    /// Total number of individual clips across all games
    pub total_clips: usize,

    /// Total storage used by all clips in bytes.
    ///
    /// NOTE: this is computed from the `clips` DB table only (sum of
    /// `fs::metadata(file_path).len()` for every known clip row) and does
    /// NOT include the rolling-buffer segments directory, replays, or
    /// auto-edit exports. Kept as-is for backward compatibility with
    /// existing frontend consumers -- see `recordings_dir_size_bytes` /
    /// `exports_dir_size_bytes` / `total_disk_usage_bytes` below for a
    /// fuller picture of real disk usage.
    pub total_size_bytes: u64,

    /// Real on-disk usage (bytes) of `base_path/recordings`, i.e. the
    /// rolling-buffer segments (segment mp4s, WASAPI loopback WAV, concat
    /// list) plus the flat extracted-clip directory. Unlike
    /// `total_size_bytes`, this is a filesystem walk and is unaware of the
    /// DB, so it also counts files the DB doesn't know about (e.g. stale
    /// segments) and won't count clip files a user relocated outside this
    /// directory.
    #[serde(default)]
    pub recordings_dir_size_bytes: u64,

    /// Real on-disk usage (bytes) of `base_path/exports` (auto-edit
    /// intermediate stages + final rendered Shorts + thumbnails).
    #[serde(default)]
    pub exports_dir_size_bytes: u64,

    /// Best-effort total disk footprint of everything this app manages
    /// under `base_path`: `recordings_dir_size_bytes + exports_dir_size_bytes`.
    /// This is the field to prefer for a "how much disk am I using" display;
    /// `total_size_bytes` undercounts because it ignores segments/exports.
    #[serde(default)]
    pub total_disk_usage_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::{ClipVaultPageInput, ClipVaultSort, UploadStatus};

    #[test]
    fn clip_vault_page_input_accepts_legacy_payload_without_filters() {
        let input: ClipVaultPageInput =
            serde_json::from_str(r#"{"sort":"newest","cursor":null,"game_limit":6}"#).unwrap();

        assert_eq!(input.sort, ClipVaultSort::Newest);
        assert!(input.query.is_none());
        assert!(input.game_mode.is_none());
    }

    #[test]
    fn upload_status_serializes_pascal_case_and_reads_legacy_lowercase() {
        assert_eq!(
            serde_json::to_string(&UploadStatus::NotUploaded).unwrap(),
            "\"NotUploaded\""
        );
        assert_eq!(
            serde_json::from_str::<UploadStatus>("\"notuploaded\"").unwrap(),
            UploadStatus::NotUploaded
        );
        assert_eq!(
            serde_json::from_str::<UploadStatus>("\"completed\"").unwrap(),
            UploadStatus::Completed
        );
    }
}
