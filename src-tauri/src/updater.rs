use crate::{AppError, AppResult, AppState};
use parking_lot::RwLock;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

pub const UPDATE_PROGRESS_EVENT: &str = "app-update-progress";
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateStatus {
    Disabled,
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Installing,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppUpdateSnapshot {
    pub status: AppUpdateStatus,
    pub current_version: String,
    pub available_version: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub progress_percentage: f64,
    pub error_code: Option<String>,
}

impl AppUpdateSnapshot {
    fn new(current_version: String, enabled: bool) -> Self {
        Self {
            status: if enabled {
                AppUpdateStatus::Idle
            } else {
                AppUpdateStatus::Disabled
            },
            current_version,
            available_version: None,
            notes: None,
            published_at: None,
            progress_percentage: 0.0,
            error_code: (!enabled).then(|| "updater_disabled".to_string()),
        }
    }
}

/// Owns updater state independently of authentication. The actual artifact is
/// retained in Rust so the frontend can never substitute a URL or signature
/// between `check` and `install`.
pub struct AppUpdateManager {
    enabled: bool,
    snapshot: Arc<RwLock<AppUpdateSnapshot>>,
    pending: Mutex<Option<Update>>,
    operation: Mutex<()>,
}

impl AppUpdateManager {
    pub fn new(current_version: impl Into<String>, enabled: bool) -> Self {
        Self {
            enabled,
            snapshot: Arc::new(RwLock::new(AppUpdateSnapshot::new(
                current_version.into(),
                enabled,
            ))),
            pending: Mutex::new(None),
            operation: Mutex::new(()),
        }
    }

    pub fn snapshot(&self) -> AppUpdateSnapshot {
        self.snapshot.read().clone()
    }

    fn updater_error(code: &'static str, message: impl Into<String>) -> AppError {
        AppError::Updater {
            code: code.to_string(),
            message: message.into(),
        }
    }

    fn publish(&self, app: &AppHandle) -> AppUpdateSnapshot {
        let snapshot = self.snapshot();
        if let Err(error) = app.emit(UPDATE_PROGRESS_EVENT, snapshot.clone()) {
            tracing::warn!("Could not emit updater progress: {error}");
        }
        snapshot
    }

    fn fail(&self, app: &AppHandle, code: &'static str, message: impl Into<String>) -> AppError {
        crate::utils::telemetry::capture_operational_error("updater", code);
        let message = message.into();
        {
            let mut snapshot = self.snapshot.write();
            snapshot.status = AppUpdateStatus::Failed;
            snapshot.error_code = Some(code.to_string());
        }
        self.publish(app);
        Self::updater_error(code, message)
    }

    pub async fn check(&self, app: &AppHandle) -> AppResult<AppUpdateSnapshot> {
        if !self.enabled {
            return Err(Self::updater_error(
                "updater_disabled",
                "Application updates are disabled for this build",
            ));
        }
        let _operation = self.operation.lock().await;
        *self.pending.lock().await = None;
        {
            let mut snapshot = self.snapshot.write();
            snapshot.status = AppUpdateStatus::Checking;
            snapshot.progress_percentage = 0.0;
            snapshot.error_code = None;
        }
        self.publish(app);

        let updater = app
            .updater()
            .map_err(|error| self.fail(app, "update_check_failed", error.to_string()))?;
        match tokio::time::timeout(UPDATE_CHECK_TIMEOUT, updater.check()).await {
            Ok(Ok(Some(update))) => {
                {
                    let mut snapshot = self.snapshot.write();
                    snapshot.status = AppUpdateStatus::Available;
                    snapshot.available_version = Some(update.version.clone());
                    snapshot.notes = update.body.clone();
                    snapshot.published_at = update.date.map(|date| date.to_string());
                    snapshot.progress_percentage = 0.0;
                    snapshot.error_code = None;
                }
                *self.pending.lock().await = Some(update);
                Ok(self.publish(app))
            }
            Ok(Ok(None)) => {
                *self.pending.lock().await = None;
                {
                    let mut snapshot = self.snapshot.write();
                    snapshot.status = AppUpdateStatus::UpToDate;
                    snapshot.available_version = None;
                    snapshot.notes = None;
                    snapshot.published_at = None;
                    snapshot.progress_percentage = 100.0;
                    snapshot.error_code = None;
                }
                Ok(self.publish(app))
            }
            Ok(Err(error)) => Err(self.fail(app, "update_check_failed", error.to_string())),
            Err(_) => Err(self.fail(
                app,
                "update_check_timeout",
                "Update check exceeded the 30 second safety limit",
            )),
        }
    }

    pub async fn install(&self, app: &AppHandle) -> AppResult<AppUpdateSnapshot> {
        if !self.enabled {
            return Err(Self::updater_error(
                "updater_disabled",
                "Application updates are disabled for this build",
            ));
        }
        let _operation = self.operation.lock().await;
        let update = self.pending.lock().await.as_ref().cloned().ok_or_else(|| {
            Self::updater_error("update_install_failed", "No checked update is available")
        })?;

        {
            let mut snapshot = self.snapshot.write();
            snapshot.status = AppUpdateStatus::Downloading;
            snapshot.progress_percentage = 0.0;
            snapshot.error_code = None;
        }
        self.publish(app);

        let snapshot = Arc::clone(&self.snapshot);
        let progress_app = app.clone();
        let mut downloaded = 0_u64;
        let result = tokio::time::timeout(
            UPDATE_DOWNLOAD_TIMEOUT,
            update.download(
                move |chunk_length, content_length| {
                    downloaded = downloaded.saturating_add(chunk_length as u64);
                    let percentage = content_length
                        .filter(|length| *length > 0)
                        .map(|length| downloaded as f64 / length as f64 * 100.0)
                        .unwrap_or(0.0)
                        .clamp(0.0, 100.0);
                    let current = {
                        let mut current = snapshot.write();
                        current.status = AppUpdateStatus::Downloading;
                        current.progress_percentage = percentage;
                        current.clone()
                    };
                    let _ = progress_app.emit(UPDATE_PROGRESS_EVENT, current);
                },
                || {},
            ),
        )
        .await;

        let bytes = match result {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error)) => {
                let code = signature_error_code(&error);
                return Err(self.fail(app, code, error.to_string()));
            }
            Err(_) => {
                return Err(self.fail(
                    app,
                    "update_download_timeout",
                    "Update download exceeded the 15 minute safety limit",
                ));
            }
        };

        {
            let mut snapshot = self.snapshot.write();
            snapshot.status = AppUpdateStatus::Installing;
            snapshot.progress_percentage = 100.0;
        }
        self.publish(app);
        if let Err(error) = update.install(bytes) {
            return Err(self.fail(app, "update_install_failed", error.to_string()));
        }
        *self.pending.lock().await = None;
        Ok(self.publish(app))
    }
}

fn signature_error_code(error: &tauri_plugin_updater::Error) -> &'static str {
    match error {
        tauri_plugin_updater::Error::Minisign(_)
        | tauri_plugin_updater::Error::Base64(_)
        | tauri_plugin_updater::Error::SignatureUtf8(_) => "update_signature_invalid",
        _ => "update_install_failed",
    }
}

#[tauri::command]
pub async fn get_app_update_status(state: State<'_, AppState>) -> AppResult<AppUpdateSnapshot> {
    Ok(state.update_manager.snapshot())
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<AppUpdateSnapshot> {
    state.update_manager.check(&app).await
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<AppUpdateSnapshot> {
    let recording_busy = matches!(
        state.recording_manager.read().await.get_status().await,
        crate::recording::RecordingStatus::Recording
            | crate::recording::RecordingStatus::Buffering
            | crate::recording::RecordingStatus::Processing
    );
    if recording_busy || state.auto_composer.is_busy().await {
        return Err(AppUpdateManager::updater_error(
            "update_busy",
            "Stop recording and wait for media jobs to finish before installing",
        ));
    }
    let storage = Arc::clone(&state.storage);
    let media_jobs_active = tokio::task::spawn_blocking(move || storage.has_active_media_jobs())
        .await
        .map_err(|error| {
            AppUpdateManager::updater_error(
                "update_busy_check_failed",
                format!("Could not inspect media jobs before update: {error}"),
            )
        })?
        .map_err(|error| {
            AppUpdateManager::updater_error(
                "update_busy_check_failed",
                format!("Could not inspect media jobs before update: {error}"),
            )
        })?;
    if media_jobs_active {
        return Err(AppUpdateManager::updater_error(
            "update_busy",
            "Wait for queued or running media jobs to finish before installing",
        ));
    }
    state.update_manager.install(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_snapshot_is_explicit_and_stable() {
        let manager = AppUpdateManager::new("1.2.0", false);
        let snapshot = manager.snapshot();
        assert_eq!(snapshot.status, AppUpdateStatus::Disabled);
        assert_eq!(snapshot.error_code.as_deref(), Some("updater_disabled"));
        assert_eq!(snapshot.current_version, "1.2.0");
    }

    #[test]
    fn enabled_snapshot_starts_idle() {
        let manager = AppUpdateManager::new("1.2.0", true);
        let snapshot = manager.snapshot();
        assert_eq!(snapshot.status, AppUpdateStatus::Idle);
        assert!(snapshot.error_code.is_none());
    }
}
