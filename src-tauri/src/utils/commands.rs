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
use sysinfo::Disks;
use tauri::State;

const DIAGNOSTIC_LOG_LINE_LIMIT: usize = 300;

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
    let mut diagnostics = crate::utils::health::get_diagnostics_status();
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

    match state.storage.health_check() {
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
        Ok(path) => diagnostics.checks.push(DiagnosticCheck {
            key: "ffmpeg_runtime",
            label: "FFmpeg runtime",
            status: DiagnosticState::Ok,
            message: format!("FFmpeg binary is available at {}.", path.display()),
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

    let recording_readiness = crate::recording::commands::collect_recording_readiness(&state).await;
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

    let youtube_client_id_present = env_present("YOUTUBE_CLIENT_ID");
    let youtube_client_secret_present = env_present("YOUTUBE_CLIENT_SECRET");
    diagnostics.checks.push(DiagnosticCheck {
        key: "youtube_config",
        label: "YouTube configuration",
        status: if youtube_client_id_present && youtube_client_secret_present {
            DiagnosticState::Warning
        } else {
            DiagnosticState::Warning
        },
        message: if youtube_client_id_present && youtube_client_secret_present {
            "YouTube OAuth configuration is present; production account behavior still requires real test-account Field QA.".to_string()
        } else {
            "YouTube OAuth credentials are not fully configured; upload features are disabled or limited.".to_string()
        },
        action: "Validate OAuth redirect, token refresh, quota errors, upload retry, and sign-out with a real test account.".to_string(),
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
        status: DiagnosticState::Blocked,
        message:
            "E5 evidence is required before public production/commercial readiness claims."
                .to_string(),
        action:
            "Complete docs/FIELD_QA_COMMERCIAL_READINESS.md with tester, machine, logs, screenshots, and sample files."
                .to_string(),
    });

    diagnostics.overall_status = crate::utils::health::overall_status(&diagnostics.checks);
    Ok(diagnostics)
}

fn env_present(name: &str) -> bool {
    std::env::var(name)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
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
    let diagnostics = collect_diagnostics_status(&state).await?;
    let storage_health = state.storage.health_check().ok();
    let storage_setting_keys = state
        .storage
        .diagnostic_setting_keys()
        .unwrap_or_else(|error| vec![format!("setting_keys_unavailable: {}", error)]);
    let logs = collect_log_excerpts(state.storage.base_path(), redact);
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
            app_data_dir: state.storage.base_path().display().to_string(),
        },
        logs,
        privacy_notice: "Bundle excludes known credential values and includes log excerpts only; review before sharing with support.".to_string(),
    };

    let output_dir = state.storage.base_path().join("diagnostics");
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
        line.to_string()
    }
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
    // Get disk usage from sysinfo
    let disks = Disks::new_with_refreshed_list();
    let recordings_path = state.storage.base_path().join("recordings");

    // Find the disk where recordings are stored
    let mut total_space = 0;
    let mut available_space = 0;

    // Default fallback (if no disk found)
    let mut found_disk = false;

    for disk in &disks {
        if recordings_path.starts_with(disk.mount_point()) {
            total_space = disk.total_space();
            available_space = disk.available_space();
            found_disk = true;
            break;
        }
    }

    // If not found by mount point, try the first disk as fallback or use system root
    if !found_disk && !disks.is_empty() {
        // Try C: on Windows or / on Linux/Mac
        #[cfg(target_os = "windows")]
        let root = std::path::Path::new("C:\\");
        #[cfg(not(target_os = "windows"))]
        let root = std::path::Path::new("/");

        for disk in &disks {
            if disk.mount_point() == root {
                total_space = disk.total_space();
                available_space = disk.available_space();
                found_disk = true;
                break;
            }
        }

        // If still not found, just use the first one
        if !found_disk {
            if let Some(disk) = disks.first() {
                total_space = disk.total_space();
                available_space = disk.available_space();
            }
        }
    }

    // Convert bytes to GB
    let total_gb = total_space as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = available_space as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = total_gb - available_gb;

    Ok(DiskSpaceInfo {
        available_gb,
        total_gb,
        used_gb,
    })
}

#[derive(serde::Serialize)]
pub struct DiskSpaceInfo {
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
pub async fn show_in_folder(file_path: String) -> AppResult<()> {
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
pub async fn open_file_with_default_app(file_path: String) -> AppResult<()> {
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
pub async fn check_file_exists(file_path: String) -> AppResult<bool> {
    Ok(std::path::Path::new(&file_path).exists())
}

/// Get comprehensive FFmpeg information
#[tauri::command]
pub async fn get_ffmpeg_info() -> AppResult<crate::utils::ffmpeg::FFmpegInfo> {
    crate::utils::ffmpeg::get_ffmpeg_info().map_err(|e| AppError::Internal(e.to_string()))
}

/// Get hardware-accelerated encoders
#[tauri::command]
pub async fn get_hardware_encoders() -> AppResult<Vec<crate::utils::ffmpeg::EncoderInfo>> {
    crate::utils::ffmpeg::get_hardware_encoders().map_err(|e| AppError::Internal(e.to_string()))
}

/// Get available video encoders
#[tauri::command]
pub async fn get_video_encoders() -> AppResult<Vec<crate::utils::ffmpeg::EncoderInfo>> {
    crate::utils::ffmpeg::get_video_encoders().map_err(|e| AppError::Internal(e.to_string()))
}
