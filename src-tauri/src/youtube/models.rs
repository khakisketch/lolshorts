use serde::{Deserialize, Serialize};

fn default_scheduled_upload_status() -> ScheduledUploadStatus {
    ScheduledUploadStatus::Pending
}

/// Lifecycle state for a queued scheduled upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScheduledUploadStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ScheduledUploadStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            ScheduledUploadStatus::Completed
                | ScheduledUploadStatus::Failed
                | ScheduledUploadStatus::Cancelled
        )
    }
}

/// Schedule information for a queued upload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSchedule {
    /// ISO 8601 datetime string for when this upload should actually execute.
    ///
    /// NOTE: this is **not** wired to YouTube's native `status.publishAt`
    /// scheduled-publish feature. Instead, [`crate::youtube::commands::start_upload_scheduler`]
    /// polls this queue every 60 seconds while the app is running and fires
    /// the real upload API call once `scheduled_at` has passed. The video is
    /// therefore only ever uploaded to YouTube once it is actually due, and
    /// nothing happens if the app is closed at the scheduled time — the
    /// upload runs the next time the app is open and polls past the due time.
    pub scheduled_at: Option<String>,
    /// Position in the upload queue (0-based)
    pub queue_position: Option<u32>,
}

/// A pending scheduled upload entry saved to storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledUpload {
    pub id: String,
    pub video_path: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub privacy_status: String,
    pub thumbnail_path: Option<String>,
    pub schedule: UploadSchedule,
    pub created_at: i64, // Unix timestamp
    #[serde(default = "default_scheduled_upload_status")]
    pub status: ScheduledUploadStatus,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub completed_video_id: Option<String>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

/// Parameters for scheduling a YouTube upload via the Tauri command.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleUploadParams {
    pub video_path: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub privacy_status: String,
    pub thumbnail_path: Option<String>,
    pub scheduled_at: Option<String>,
}

/// Structured event emitted after scheduled upload execution finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledUploadEvent {
    pub upload_id: String,
    pub status: ScheduledUploadStatus,
    pub video_id: Option<String>,
    pub title: String,
    pub attempts: u32,
    pub error_message: Option<String>,
}

/// YouTube authentication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub expires_at: Option<i64>,
    pub has_refresh_token: bool,
}

/// Upload history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadHistoryEntry {
    pub video_id: String,
    pub title: String,
    pub uploaded_at: i64, // Unix timestamp
    pub privacy_status: String,
    pub thumbnail_url: Option<String>,
    pub view_count: Option<u64>,
}

/// YouTube quota information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub daily_limit: u64,
    pub used: u64,
    pub remaining: u64,
    pub reset_at: i64, // Unix timestamp (midnight Pacific Time)
}

impl QuotaInfo {
    /// YouTube Data API v3 daily quota limit (10,000 units)
    pub const DAILY_LIMIT: u64 = 10_000;

    /// Create new quota info
    pub fn new(used: u64) -> Self {
        let now = chrono::Utc::now();

        // Calculate Pacific midnight (23:59:59) for quota reset
        // These values are always valid, so we use expect() with clear reasoning
        let naive_midnight = now
            .with_timezone(&chrono_tz::US::Pacific)
            .date_naive()
            .and_hms_opt(23, 59, 59)
            .expect("23:59:59 is always a valid time");

        // Handle timezone conversion safely - use latest() for DST edge cases
        let reset_timestamp = naive_midnight
            .and_local_timezone(chrono_tz::US::Pacific)
            .latest()
            .map(|dt| dt.with_timezone(&chrono::Utc).timestamp())
            .unwrap_or_else(|| {
                // Fallback: use current time + 24 hours if timezone conversion fails
                (now + chrono::Duration::hours(24)).timestamp()
            });

        Self {
            daily_limit: Self::DAILY_LIMIT,
            used,
            remaining: Self::DAILY_LIMIT.saturating_sub(used),
            reset_at: reset_timestamp,
        }
    }
}
