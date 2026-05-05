#![allow(clippy::upper_case_acronyms)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub usage_count: u32, // Number of times this clip has been used in auto-edits
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Total number of games with recorded clips
    pub total_games: usize,

    /// Total number of individual clips across all games
    pub total_clips: usize,

    /// Total storage used by all clips in bytes
    pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::UploadStatus;

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
