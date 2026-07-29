pub mod auth;
pub mod error;
pub mod hotkey;
pub mod lcu;
pub mod overlay;
pub mod recording;
pub mod settings;
pub mod storage;
pub mod supabase;
pub mod tray;
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
}
