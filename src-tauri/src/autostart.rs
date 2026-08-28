use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutostartStatus {
    pub configured: bool,
    pub enabled: bool,
    pub error_code: Option<String>,
}

impl Default for AutostartStatus {
    fn default() -> Self {
        Self {
            configured: false,
            enabled: false,
            error_code: Some("AUTOSTART_NOT_PROBED".to_string()),
        }
    }
}

fn query(app: &AppHandle) -> AutostartStatus {
    match app.autolaunch().is_enabled() {
        Ok(enabled) => AutostartStatus {
            configured: true,
            enabled,
            error_code: None,
        },
        Err(error) => {
            tracing::warn!(error = %error, "Unable to query Windows autostart state");
            AutostartStatus {
                configured: false,
                enabled: false,
                error_code: Some("AUTOSTART_QUERY_FAILED".to_string()),
            }
        }
    }
}

pub fn apply(app: &AppHandle, enabled: bool) -> Result<AutostartStatus, String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    result.map_err(|error| {
        tracing::warn!(error = %error, enabled, "Unable to update Windows autostart state");
        "AUTOSTART_UPDATE_FAILED".to_string()
    })?;

    let status = query(app);
    if !status.configured || status.enabled != enabled {
        return Err("AUTOSTART_RECONCILE_FAILED".to_string());
    }
    Ok(status)
}

pub async fn apply_and_store(
    app: &AppHandle,
    shared_status: &Arc<RwLock<AutostartStatus>>,
    enabled: bool,
) -> Result<AutostartStatus, String> {
    match apply(app, enabled) {
        Ok(status) => {
            *shared_status.write().await = status.clone();
            Ok(status)
        }
        Err(error_code) => {
            *shared_status.write().await = AutostartStatus {
                configured: false,
                enabled: false,
                error_code: Some(error_code.clone()),
            };
            Err(error_code)
        }
    }
}

#[tauri::command]
pub async fn get_autostart_status(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<AutostartStatus, String> {
    let status = query(&app);
    *state.autostart_status.write().await = status.clone();
    Ok(status)
}

#[tauri::command]
pub async fn set_launch_on_windows_startup(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    enabled: bool,
) -> Result<AutostartStatus, String> {
    apply_and_store(&app, &state.autostart_status, enabled).await
}
