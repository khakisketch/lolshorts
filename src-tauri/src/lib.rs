pub mod auth;
pub mod autostart;
pub mod error;
pub mod hotkey;
pub mod lcu;
pub mod overlay;
pub mod public_service_config;
pub mod recording;
pub mod settings;
pub mod storage;
pub mod supabase;
pub mod tray;
pub mod updater;
pub mod utils;
pub mod video;
pub mod youtube;

use std::sync::Arc;
use tokio::sync::RwLock;

// Re-export commonly used types
pub use error::{AppError, AppResult};

/// 모든 Tauri 명령에서 공유되는 애플리케이션 상태
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<storage::Storage>,
    /// The directory currently used by the recording backend. This can differ
    /// from `storage.base_path()/recordings` when startup enters a recovery
    /// recording path, so commands must not reconstruct it from `dirs`.
    pub recordings_dir: std::path::PathBuf,
    pub auth: Arc<auth::AuthManager>,
    pub recording_manager: Arc<RwLock<recording::RecordingManager>>,
    pub clip_manager: Arc<recording::auto_clip_manager::AutoClipManager>,
    pub game_monitor: Arc<recording::game_monitor::GameStateMonitor>,
    pub recording_settings: Arc<RwLock<settings::models::RecordingSettings>>,
    pub hotkey_manager: Arc<hotkey::HotkeyManager>,
    pub metrics_collector: Arc<utils::metrics::MetricsCollector>,
    pub cleanup_manager: Arc<utils::cleanup::CleanupManager>,
    pub auto_composer: Arc<video::auto_composer::AutoComposer>,
    pub video_processor: Arc<video::VideoProcessor>,
    pub youtube_manager: Arc<youtube::commands::YouTubeManager>,
    pub lcu_client: Arc<tokio::sync::Mutex<lcu::LcuClient>>,
    pub startup_issues: Arc<RwLock<Vec<String>>>,
    pub recording_disk_monitor: Arc<RwLock<Option<tokio::sync::watch::Sender<bool>>>>,
    pub update_manager: Arc<updater::AppUpdateManager>,
    pub media_job_executor: Arc<video::media_job_executor::MediaJobExecutor>,
    pub public_service_status: public_service_config::PublicServiceStatus,
    pub autostart_status: Arc<RwLock<autostart::AutostartStatus>>,
}
