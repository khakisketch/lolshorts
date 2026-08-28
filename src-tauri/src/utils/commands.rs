use crate::auth::command_policy::require_command_access;
use crate::error::{AppError, AppResult};
use crate::utils::health::{DiagnosticCheck, DiagnosticState, DiagnosticsStatus};
use crate::utils::metrics::{HealthStatus, RecordingMetrics, SystemMetrics};
/// Tauri commands for production utilities
///
/// Exposes metrics, health status, and system info to frontend
use crate::AppState;
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

const DIAGNOSTIC_LOG_LINE_LIMIT: usize = 300;

async fn run_utility_blocking<T, F>(operation: &'static str, task: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| AppError::Internal(format!("{operation} task failed: {error}")))
}

/// Get current recording performance metrics
#[tauri::command]
pub async fn get_recording_metrics(state: State<'_, AppState>) -> AppResult<RecordingMetrics> {
    Ok(state.metrics_collector.get_recording_metrics().await)
}

/// Get current system resource metrics
#[tauri::command]
pub async fn get_system_metrics(state: State<'_, AppState>) -> AppResult<SystemMetrics> {
    Ok(state.metrics_collector.get_system_metrics().await)
}

/// Get current system health status
#[tauri::command]
pub async fn get_health_status(state: State<'_, AppState>) -> AppResult<HealthStatus> {
    Ok(state.metrics_collector.check_health().await)
}

/// Get commercial-readiness diagnostics for updater, signing, and runtime configuration.
#[tauri::command]
pub async fn get_diagnostics_status(state: State<'_, AppState>) -> AppResult<DiagnosticsStatus> {
    collect_diagnostics_status(&state).await
}

async fn collect_diagnostics_status(state: &AppState) -> AppResult<DiagnosticsStatus> {
    let mut diagnostics =
        crate::utils::health::get_diagnostics_status(&state.public_service_status);
    let startup_issues = state.startup_issues.read().await;

    if startup_issues.is_empty() {
        diagnostics.checks.push(DiagnosticCheck {
            key: "startup_runtime",
            label: "Startup runtime",
            status: DiagnosticState::Ok,
            message: "Application runtime initialized without degraded fallbacks.".to_string(),
            action: "No action required.".to_string(),
        });
    } else {
        diagnostics.checks.push(DiagnosticCheck {
            key: "startup_runtime",
            label: "Startup runtime",
            status: DiagnosticState::Warning,
            message: startup_issues.join(" "),
            action:
                "Review disk permissions, SQLite health, recording folder access, and hotkey conflicts."
                    .to_string(),
        });
    }

    let storage = state.storage.clone();
    let storage_health =
        run_utility_blocking("SQLite health", move || storage.health_check()).await?;
    match storage_health {
        Ok(health) if health.integrity_ok => diagnostics.checks.push(DiagnosticCheck {
            key: "sqlite_health",
            label: "SQLite local data",
            status: DiagnosticState::Ok,
            message: format!(
                "Local SQLite integrity check passed ({} bytes).",
                health.database_size_bytes
            ),
            action: "No action required.".to_string(),
        }),
        Ok(health) => diagnostics.checks.push(DiagnosticCheck {
            key: "sqlite_health",
            label: "SQLite local data",
            status: DiagnosticState::Blocked,
            message: format!(
                "Local SQLite integrity check failed: {}.",
                health.integrity_message
            ),
            action:
                "Use a backup, preserve the database file, and follow database recovery guidance."
                    .to_string(),
        }),
        Err(error) => diagnostics.checks.push(DiagnosticCheck {
            key: "sqlite_health",
            label: "SQLite local data",
            status: DiagnosticState::Blocked,
            message: format!("Local SQLite health could not be checked: {}.", error),
            action: "Check database permissions and preserve the app data directory for support."
                .to_string(),
        }),
    }

    match crate::utils::ffmpeg::get_ffmpeg_path() {
        Ok(_path) => diagnostics.checks.push(DiagnosticCheck {
            key: "ffmpeg_runtime",
            label: "FFmpeg runtime",
            status: DiagnosticState::Ok,
            message: "FFmpeg binary is available and passed its usability probe.".to_string(),
            action: "No action required; field QA must still verify real capture output."
                .to_string(),
        }),
        Err(error) => diagnostics.checks.push(DiagnosticCheck {
            key: "ffmpeg_runtime",
            label: "FFmpeg runtime",
            status: DiagnosticState::Blocked,
            message: format!("FFmpeg binary is unavailable: {}.", error),
            action: "Bundle FFmpeg with the installer or install it before recording tests."
                .to_string(),
        }),
    }

    let recording_readiness = crate::recording::commands::collect_recording_readiness(state).await;
    diagnostics.checks.push(DiagnosticCheck {
        key: "recording_readiness",
        label: "Recording readiness",
        status: if recording_readiness.ready {
            DiagnosticState::Ok
        } else {
            DiagnosticState::Blocked
        },
        message: if recording_readiness.ready {
            format!(
                "{} warning(s); runtime field capture still requires E5 evidence.",
                recording_readiness.warnings.len()
            )
        } else {
            recording_readiness
                .blockers
                .iter()
                .map(|blocker| format!("{}: {}", blocker.component, blocker.message))
                .collect::<Vec<_>>()
                .join("; ")
        },
        action: if recording_readiness.ready {
            "Run real LoL/LCU/replay/audio/GPU Field QA before any readiness claim.".to_string()
        } else {
            "Resolve blockers before attempting recording Field QA.".to_string()
        },
    });

    diagnostics.checks.push(DiagnosticCheck {
        key: "payment_deferred",
        label: "Payment boundary",
        status: DiagnosticState::Ok,
        message:
            "Toss/live billing and paid PRO sales are deferred for this non-payment Windows RC track."
                .to_string(),
        action:
            "Do not enable live payment keys until all non-payment Field QA gates and separate payment QA pass."
                .to_string(),
    });

    diagnostics.checks.push(DiagnosticCheck {
        key: "field_evidence",
        label: "Field QA evidence",
        status: DiagnosticState::Warning,
        message:
            "E5 evidence is tracked by the release process and is not embedded in this installation."
                .to_string(),
        action:
            "Release owners must complete docs/FIELD_QA_COMMERCIAL_READINESS.md before stable publication."
                .to_string(),
    });

    diagnostics.overall_status = crate::utils::health::overall_status(&diagnostics.checks);
    Ok(diagnostics)
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsBundleExport {
    pub output_path: String,
    pub redacted: bool,
    pub generated_at: String,
    pub included_logs: usize,
}

#[derive(Debug, Serialize)]
struct DiagnosticsBundleFile {
    generated_at: String,
    app_version: String,
    redacted: bool,
    diagnostics: DiagnosticsStatus,
    storage_health: Option<crate::storage::StorageHealth>,
    storage_setting_keys: Vec<String>,
    system: DiagnosticsSystemSummary,
    logs: Vec<DiagnosticsLogExcerpt>,
    privacy_notice: String,
}

#[derive(Debug, Serialize)]
struct DiagnosticsSystemSummary {
    os: String,
    arch: String,
    app_data_dir: String,
}

#[derive(Debug, Serialize)]
struct DiagnosticsLogExcerpt {
    file_name: String,
    lines: Vec<String>,
}

#[tauri::command]
pub async fn export_diagnostics_bundle(
    state: State<'_, AppState>,
    redact: Option<bool>,
) -> AppResult<DiagnosticsBundleExport> {
    let redact = redact.unwrap_or(true);
    let generated_at = Utc::now().to_rfc3339();
    let mut diagnostics = collect_diagnostics_status(&state).await?;
    let storage = state.storage.clone();
    let base_path = storage.base_path().to_path_buf();

    run_utility_blocking("Diagnostics export", move || {
        if redact {
            redact_diagnostics_status(&mut diagnostics);
        }
        let storage_health = storage.health_check().ok();
        let storage_setting_keys = storage.diagnostic_setting_keys().unwrap_or_else(|error| {
            if redact {
                vec!["setting_keys_unavailable".to_string()]
            } else {
                vec![format!("setting_keys_unavailable: {error}")]
            }
        });
        let logs = collect_log_excerpts(&base_path, redact);
        let included_logs = logs.len();
        let bundle = DiagnosticsBundleFile {
            generated_at: generated_at.clone(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            redacted: redact,
            diagnostics,
            storage_health,
            storage_setting_keys,
            system: DiagnosticsSystemSummary {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                app_data_dir: if redact {
                    "%APPDATA%\\lolshorts".to_string()
                } else {
                    base_path.display().to_string()
                },
            },
            logs,
            privacy_notice: "Bundle excludes known credential values, user-profile paths, and media contents; review before sharing with support.".to_string(),
        };

        let output_dir = base_path.join("diagnostics");
        fs::create_dir_all(&output_dir).map_err(|error| AppError::Io(error.to_string()))?;
        let output_path = output_dir.join(format!(
            "diagnostics_{}.json",
            Utc::now().format("%Y%m%d_%H%M%S")
        ));
        let json = serde_json::to_string_pretty(&bundle)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        fs::write(&output_path, json).map_err(|error| AppError::Io(error.to_string()))?;

        Ok(DiagnosticsBundleExport {
            output_path: output_path.display().to_string(),
            redacted: redact,
            generated_at,
            included_logs,
        })
    })
    .await?
}

fn redact_diagnostics_status(status: &mut DiagnosticsStatus) {
    for check in &mut status.checks {
        check.message = redact_sensitive_text(&check.message);
        check.action = redact_sensitive_text(&check.action);
    }
}

fn collect_log_excerpts(base_path: &Path, redact: bool) -> Vec<DiagnosticsLogExcerpt> {
    let log_dir = base_path.join("logs");
    let mut entries: Vec<PathBuf> = match fs::read_dir(&log_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect(),
        Err(_) => return Vec::new(),
    };

    entries.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    entries.reverse();

    entries
        .into_iter()
        .take(3)
        .filter_map(|path| {
            let file_name = path.file_name()?.to_string_lossy().to_string();
            let text = fs::read_to_string(&path).ok()?;
            let mut lines: Vec<String> = text
                .lines()
                .rev()
                .take(DIAGNOSTIC_LOG_LINE_LIMIT)
                .map(|line| {
                    if redact {
                        redact_sensitive_text(line)
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            lines.reverse();
            Some(DiagnosticsLogExcerpt { file_name, lines })
        })
        .collect()
}

fn redact_sensitive_text(line: &str) -> String {
    const SENSITIVE_MARKERS: &[&str] = &[
        "token",
        "refresh_token",
        "access_token",
        "authorization",
        "cookie",
        "secret",
        "password",
        "payment",
        "toss",
        "supabase",
        "signing",
        "private_key",
        "client_secret",
    ];

    let lower = line.to_ascii_lowercase();
    if SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
    {
        "[redacted sensitive diagnostic line]".to_string()
    } else {
        redact_user_profile_path(line)
    }
}

fn redact_user_profile_path(value: &str) -> String {
    let Some(home) = dirs::home_dir() else {
        return value.to_string();
    };
    let home = home.to_string_lossy();
    let mut redacted = value.replace(home.as_ref(), "%USERPROFILE%");
    // Logs can contain either separator style regardless of the current OS.
    redacted = redacted.replace(&home.replace('\\', "/"), "%USERPROFILE%");
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_redaction_removes_secret_lines() {
        assert_eq!(
            redact_sensitive_text("access_token=abc123"),
            "[redacted sensitive diagnostic line]"
        );
        assert_eq!(
            redact_sensitive_text("ordinary warning"),
            "ordinary warning"
        );
    }

    #[test]
    fn diagnostics_redaction_removes_user_profile_paths() {
        if let Some(home) = dirs::home_dir() {
            let input = format!("Log path: {}\\LoLShorts\\app.log", home.display());
            let redacted = redact_sensitive_text(&input);
            assert!(!redacted.contains(home.to_string_lossy().as_ref()));
            assert!(redacted.contains("%USERPROFILE%"));
        }
    }
}

/// Get application version info
#[tauri::command]
pub fn get_app_version() -> AppResult<String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Force cleanup of temporary files
#[tauri::command]
pub async fn force_cleanup(state: State<'_, AppState>) -> AppResult<u64> {
    state
        .cleanup_manager
        .cleanup_on_startup()
        .await
        .map(|_| 0) // Return 0 as it's async, actual cleanup happens in background
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Get disk space info for recordings directory using sysinfo for real data
#[tauri::command]
pub async fn get_disk_space_info(state: State<'_, AppState>) -> AppResult<DiskSpaceInfo> {
    let recordings_path = state.storage.base_path().join("recordings");
    run_utility_blocking("Disk-space probe", move || {
        let snapshot = crate::utils::disk::query_disk_space(&recordings_path).ok();
        let known = snapshot.is_some();
        let (total_space, available_space) = snapshot
            .map(|space| (space.total_bytes, space.available_bytes))
            .unwrap_or((0, 0));
        let total_gb = total_space as f64 / (1024.0 * 1024.0 * 1024.0);
        let available_gb = available_space as f64 / (1024.0 * 1024.0 * 1024.0);

        DiskSpaceInfo {
            known,
            available_gb,
            total_gb,
            used_gb: (total_gb - available_gb).max(0.0),
        }
    })
    .await
}

#[derive(serde::Serialize)]
pub struct DiskSpaceInfo {
    pub known: bool,
    pub available_gb: f64,
    pub total_gb: f64,
    pub used_gb: f64,
}

/// Validate that a path is absolute and canonical (prevents path traversal)
fn validate_path(file_path: &str) -> AppResult<std::path::PathBuf> {
    let path = std::path::Path::new(file_path);
    if !path.is_absolute() {
        return Err(AppError::Validation(
            "Only absolute paths are allowed".to_string(),
        ));
    }
    path.canonicalize()
        .map_err(|e| AppError::Io(format!("Invalid path: {}", e)))
}

/// Show file/folder in system file explorer
#[tauri::command]
pub async fn show_in_folder(state: State<'_, AppState>, file_path: String) -> AppResult<()> {
    require_command_access(&state.auth, "show_in_folder")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let canonical = validate_path(&file_path)?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&canonical)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open explorer: {}", e)))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&canonical)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open Finder: {}", e)))?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = canonical.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| AppError::Io(format!("Failed to open file manager: {}", e)))?;
        }
    }

    Ok(())
}

/// Open file with default system application
#[tauri::command]
pub async fn open_file_with_default_app(
    state: State<'_, AppState>,
    file_path: String,
) -> AppResult<()> {
    require_command_access(&state.auth, "open_file_with_default_app")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    let canonical = validate_path(&file_path)?;
    if !canonical.exists() {
        return Err(AppError::NotFound("File does not exist".to_string()));
    }

    #[cfg(target_os = "windows")]
    {
        // Use explorer.exe to open file with default app - avoids cmd shell injection
        std::process::Command::new("explorer")
            .arg(&canonical)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open file: {}", e)))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&canonical)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open file: {}", e)))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&canonical)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open file: {}", e)))?;
    }

    Ok(())
}

/// Check if file exists at given path
#[tauri::command]
pub async fn check_file_exists(state: State<'_, AppState>, file_path: String) -> AppResult<bool> {
    require_command_access(&state.auth, "check_file_exists")
        .map_err(|e| AppError::Auth(e.to_string()))?;
    Ok(std::path::Path::new(&file_path).exists())
}

/// Get comprehensive FFmpeg information
#[tauri::command]
pub async fn get_ffmpeg_info() -> AppResult<crate::utils::ffmpeg::FFmpegInfo> {
    run_utility_blocking("FFmpeg information probe", || {
        crate::utils::ffmpeg::get_ffmpeg_info().map_err(|e| AppError::Internal(e.to_string()))
    })
    .await?
}

/// Get hardware-accelerated encoders
#[tauri::command]
pub async fn get_hardware_encoders() -> AppResult<Vec<crate::utils::ffmpeg::EncoderInfo>> {
    run_utility_blocking("Hardware encoder probe", || {
        crate::utils::ffmpeg::get_hardware_encoders().map_err(|e| AppError::Internal(e.to_string()))
    })
    .await?
}

/// Get available video encoders
#[tauri::command]
pub async fn get_video_encoders() -> AppResult<Vec<crate::utils::ffmpeg::EncoderInfo>> {
    crate::utils::ffmpeg::get_video_encoders().map_err(|e| AppError::Internal(e.to_string()))
}
