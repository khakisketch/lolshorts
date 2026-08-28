use super::models::{EncoderPreference, RecordingSettings};
use super::platform_config::{PlatformConfig, RecommendedSettings};
use crate::AppState;
use tauri::State;

/// Translate persisted settings into the runtime recording configuration.
/// Keeping this mapping in one place prevents reset/save paths from drifting
/// and ensures a successful settings change is applied to the active recorder
/// with the same encoder, audio, monitor, and output choices.
fn recording_config_from_settings(
    settings: &RecordingSettings,
    recordings_dir: &std::path::Path,
) -> crate::recording::RecordingConfig {
    let encoder_pref = match settings.video.encoder {
        EncoderPreference::Auto => "auto",
        EncoderPreference::Nvenc => "nvenc",
        EncoderPreference::Qsv => "qsv",
        EncoderPreference::Amf => "amf",
        EncoderPreference::Software => "software",
    };

    let video = crate::recording::VideoSettingsConfig {
        resolution: settings.video.get_resolution(),
        fps: settings.video.get_fps(),
        bitrate: settings.video.get_bitrate(),
        use_h265: settings.video.is_h265(),
        encoder_preference: encoder_pref.to_string(),
        monitor_index: (settings.video.monitor_index > 0).then_some(settings.video.monitor_index),
    };
    let encoder = if video.use_h265 {
        crate::recording::VideoEncoder::H265
    } else {
        crate::recording::VideoEncoder::H264
    };
    let hw_accel = match video.encoder_preference.as_str() {
        "nvenc" => crate::recording::HwAccel::Nvenc,
        "qsv" => crate::recording::HwAccel::Qsv,
        "amf" => crate::recording::HwAccel::Amf,
        "software" => crate::recording::HwAccel::Software,
        _ => crate::recording::HwAccel::Auto,
    };

    crate::recording::RecordingConfig {
        fps: video.fps,
        bitrate: video.bitrate,
        resolution: video.resolution,
        encoder,
        hw_accel,
        output_dir: recordings_dir.to_path_buf(),
        audio_config: Some(settings.audio.to_audio_config()),
        monitor_index: (settings.video.monitor_index > 0).then_some(settings.video.monitor_index),
        ..Default::default()
    }
}

/// Get current recording settings
#[tauri::command]
pub async fn get_recording_settings(
    state: State<'_, AppState>,
) -> Result<RecordingSettings, String> {
    // Read from shared in-memory settings
    let settings = state.recording_settings.read().await;
    Ok(settings.clone())
}

/// Save recording settings
#[tauri::command]
pub async fn save_recording_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: RecordingSettings,
) -> Result<(), String> {
    // Validate settings before saving
    settings
        .validate_integrity()
        .map_err(|e| format!("Settings validation failed: {}", e))?;

    let previous_launch_setting = state
        .recording_settings
        .read()
        .await
        .launch_on_windows_startup;
    let launch_setting_changed = previous_launch_setting != settings.launch_on_windows_startup;

    if launch_setting_changed {
        crate::autostart::apply_and_store(
            &app,
            &state.autostart_status,
            settings.launch_on_windows_startup,
        )
        .await?;
    }

    // Save only after the OS accepted the autostart change. If persistence
    // fails, best-effort rollback keeps the toggle and Windows state aligned.
    if let Err(error) = settings.save() {
        if launch_setting_changed {
            let _ = crate::autostart::apply_and_store(
                &app,
                &state.autostart_status,
                previous_launch_setting,
            )
            .await;
        }
        return Err(error.to_string());
    }

    // Build RecordingConfig from new settings and apply to RecordingManager.
    let new_config = recording_config_from_settings(&settings, &state.recordings_dir);

    if let Err(e) = state
        .recording_manager
        .write()
        .await
        .update_config(new_config)
        .await
    {
        tracing::warn!(
            "설정을 녹화 매니저에 즉시 적용하지 못함 (녹화 중일 수 있음): {}",
            e
        );
    }

    // Update shared in-memory settings
    let mut current_settings = state.recording_settings.write().await;
    *current_settings = settings;
    crate::utils::telemetry::set_enabled(
        current_settings.crash_reporting_enabled
            && state.public_service_status.telemetry.configured,
    );

    Ok(())
}

/// Reset settings to default values
#[tauri::command]
pub async fn reset_settings_to_default(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RecordingSettings, String> {
    let previous_settings = state.recording_settings.read().await.clone();
    let defaults = RecordingSettings::default();
    defaults
        .validate_integrity()
        .map_err(|e| format!("Settings validation failed: {e}"))?;
    let launch_setting_changed =
        previous_settings.launch_on_windows_startup != defaults.launch_on_windows_startup;

    // Apply the OS change first. If persistence fails, roll the OS state back
    // so a reset cannot leave Windows startup and settings.json disagreeing.
    if launch_setting_changed {
        crate::autostart::apply_and_store(
            &app,
            &state.autostart_status,
            defaults.launch_on_windows_startup,
        )
        .await?;
    }
    if let Err(error) = defaults.save() {
        if launch_setting_changed {
            let _ = crate::autostart::apply_and_store(
                &app,
                &state.autostart_status,
                previous_settings.launch_on_windows_startup,
            )
            .await;
        }
        return Err(error.to_string());
    }

    // Keep the active recorder in sync with a reset just like a normal save.
    if let Err(error) = state
        .recording_manager
        .write()
        .await
        .update_config(recording_config_from_settings(
            &defaults,
            &state.recordings_dir,
        ))
        .await
    {
        tracing::warn!(
            "설정 초기화를 녹화 매니저에 즉시 적용하지 못함 (녹화 중일 수 있음): {}",
            error
        );
    }

    // Update shared in-memory settings
    let mut current_settings = state.recording_settings.write().await;
    *current_settings = defaults.clone();
    crate::utils::telemetry::set_enabled(
        current_settings.crash_reporting_enabled
            && state.public_service_status.telemetry.configured,
    );

    Ok(defaults)
}

// ============================================================================
// Platform Configuration Commands
// ============================================================================

/// Detect platform and hardware capabilities
#[tauri::command]
pub async fn detect_platform_config() -> Result<PlatformConfig, String> {
    PlatformConfig::detect()
        .await
        .map_err(|e| format!("Failed to detect platform configuration: {}", e))
}

/// Get hardware-based recommended settings
#[tauri::command]
pub async fn get_recommended_settings() -> Result<RecommendedSettings, String> {
    let platform_config = PlatformConfig::detect()
        .await
        .map_err(|e| format!("Failed to detect platform: {}", e))?;

    Ok(platform_config.recommended_settings)
}

/// Validate settings against platform capabilities
#[tauri::command]
pub async fn validate_settings_for_platform(settings: RecordingSettings) -> Result<bool, String> {
    let platform_config = PlatformConfig::detect()
        .await
        .map_err(|e| format!("Failed to detect platform: {}", e))?;

    platform_config
        .validate_settings(&settings)
        .map(|_| true)
        .map_err(|e| format!("Settings validation failed: {}", e))
}

/// Optimize settings for current platform and hardware
#[tauri::command]
pub async fn optimize_settings_for_platform(
    mut settings: RecordingSettings,
) -> Result<RecordingSettings, String> {
    let platform_config = PlatformConfig::detect()
        .await
        .map_err(|e| format!("Failed to detect platform: {}", e))?;

    platform_config.optimize_settings(&mut settings);

    platform_config
        .validate_settings(&settings)
        .map_err(|e| format!("Optimized settings failed validation: {}", e))?;

    Ok(settings)
}

/// Check if settings migration is needed
#[tauri::command]
pub async fn check_settings_migration_needed() -> Result<bool, String> {
    RecordingSettings::needs_migration()
        .map_err(|e| format!("Failed to check migration status: {}", e))
}

/// Perform settings migration
#[tauri::command]
pub async fn migrate_settings() -> Result<RecordingSettings, String> {
    RecordingSettings::migrate_settings()
        .await
        .map_err(|e| format!("Settings migration failed: {}", e))
}

/// Load settings with platform optimization
#[tauri::command]
pub async fn load_settings_optimized() -> Result<RecordingSettings, String> {
    RecordingSettings::load_with_platform_optimization()
        .await
        .map_err(|e| format!("Failed to load optimized settings: {}", e))
}

/// Export settings to backup file
#[tauri::command]
pub async fn export_settings_backup(
    settings: RecordingSettings,
    file_path: String,
) -> Result<(), String> {
    let path = std::path::PathBuf::from(file_path);
    settings
        .export_settings(&path)
        .map_err(|e| format!("Failed to export settings: {}", e))
}

/// Import settings from backup file
#[tauri::command]
pub async fn import_settings_backup(file_path: String) -> Result<RecordingSettings, String> {
    let path = std::path::PathBuf::from(file_path);
    RecordingSettings::import_settings(&path)
        .map_err(|e| format!("Failed to import settings: {}", e))
}

/// Get settings diagnostics summary
#[tauri::command]
pub async fn get_settings_diagnostics() -> Result<String, String> {
    let settings = RecordingSettings::load()
        .map_err(|e| format!("Failed to load settings for diagnostics: {}", e))?;

    Ok(settings.get_diagnostics_summary())
}
