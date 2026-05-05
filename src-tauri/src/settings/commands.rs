use super::models::{EncoderPreference, RecordingSettings};
use super::platform_config::{PlatformConfig, RecommendedSettings};
use crate::AppState;
use tauri::State;

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
    state: State<'_, AppState>,
    settings: RecordingSettings,
) -> Result<(), String> {
    // Validate settings before saving
    settings
        .validate_integrity()
        .map_err(|e| format!("Settings validation failed: {}", e))?;

    // Save to disk first
    settings.save().map_err(|e| e.to_string())?;

    // Build RecordingConfig from new settings and apply to RecordingManager
    let encoder_pref = match settings.video.encoder {
        EncoderPreference::Auto => "auto",
        EncoderPreference::Nvenc => "nvenc",
        EncoderPreference::Qsv => "qsv",
        EncoderPreference::Amf => "amf",
        EncoderPreference::Software => "software",
    };

    let recordings_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("lolshorts")
        .join("recordings");

    let audio_config = Some(settings.audio.to_audio_config());
    let video = crate::recording::VideoSettingsConfig {
        resolution: settings.video.get_resolution(),
        fps: settings.video.get_fps(),
        bitrate: settings.video.get_bitrate(),
        use_h265: settings.video.is_h265(),
        encoder_preference: encoder_pref.to_string(),
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

    let new_config = crate::recording::RecordingConfig {
        fps: video.fps,
        bitrate: video.bitrate,
        resolution: video.resolution,
        encoder,
        hw_accel,
        output_dir: recordings_dir,
        audio_config,
        monitor_index: if settings.video.monitor_index > 0 {
            Some(settings.video.monitor_index)
        } else {
            None
        },
        ..Default::default()
    };

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

    Ok(())
}

/// Reset settings to default values
#[tauri::command]
pub async fn reset_settings_to_default(
    state: State<'_, AppState>,
) -> Result<RecordingSettings, String> {
    // Reset to defaults and save
    let defaults = RecordingSettings::reset_to_default().map_err(|e| e.to_string())?;

    // Update shared in-memory settings
    let mut current_settings = state.recording_settings.write().await;
    *current_settings = defaults.clone();

    Ok(defaults)
}

// TODO: These tests require Tauri State and should be integration tests
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[tokio::test]
//     async fn test_get_settings_command() {
//         // TODO: Implement as E2E test with Playwright
//         // let result = get_recording_settings().await;
//         // assert!(result.is_ok());
//     }
//
//     #[tokio::test]
//     async fn test_save_settings_command() {
//         // TODO: Implement as E2E test with Playwright
//         // let settings = RecordingSettings::default();
//         // let result = save_recording_settings(settings).await;
//         // assert!(result.is_ok());
//     }
//
//     #[tokio::test]
//     async fn test_reset_command() {
//         // TODO: Implement as E2E test with Playwright
//         // let result = reset_settings_to_default().await;
//         // assert!(result.is_ok());
//         //
//         // let settings = result.unwrap();
//         // assert_eq!(settings.event_filter.min_priority, 2);
//     }
// }

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
