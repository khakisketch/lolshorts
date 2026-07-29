use super::models::RecordingSettings;
use super::platform_config::{PlatformConfig, PlatformConfigError};
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Write data atomically: write to .tmp, then rename to final path.
/// Prevents corruption if process crashes mid-write.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension().unwrap_or_default().to_string_lossy()
    ));
    fs::write(&tmp_path, data)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("Failed to get config directory")]
    ConfigDirNotFound,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Platform configuration error: {0}")]
    PlatformConfig(#[from] PlatformConfigError),

    #[error("Settings validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, SettingsError>;

impl RecordingSettings {
    /// Load settings from file
    ///
    /// If the settings file doesn't exist, returns default settings.
    /// On parse failure, attempts to load from .bak file before falling back to defaults.
    /// Location: %APPDATA%/Roaming/LoLShorts/settings.json (Windows)
    pub fn load() -> Result<Self> {
        let settings_path = Self::get_settings_path()?;

        if settings_path.exists() {
            match fs::read_to_string(&settings_path)
                .map_err(SettingsError::from)
                .and_then(|json| serde_json::from_str::<Self>(&json).map_err(SettingsError::from))
            {
                Ok(settings) => {
                    tracing::info!("Loaded settings from: {:?}", settings_path);
                    if let Err(e) = settings.validate() {
                        tracing::error!("Settings validation failed: {}. Using defaults.", e);
                        return Ok(Self::default());
                    }
                    return Ok(settings);
                }
                Err(e) => {
                    tracing::warn!("Settings load failed: {}. Trying backup.", e);
                    let backup_path = settings_path.with_extension("json.bak");
                    if backup_path.exists() {
                        match fs::read_to_string(&backup_path)
                            .map_err(SettingsError::from)
                            .and_then(|json| {
                                serde_json::from_str::<Self>(&json).map_err(SettingsError::from)
                            }) {
                            Ok(settings) => {
                                tracing::warn!("Loaded settings from backup: {:?}", backup_path);
                                if let Err(e2) = settings.validate() {
                                    tracing::error!(
                                        "Backup settings validation failed: {}. Using defaults.",
                                        e2
                                    );
                                    return Ok(Self::default());
                                }
                                return Ok(settings);
                            }
                            Err(e2) => {
                                tracing::warn!(
                                    "Backup settings also failed: {}. Using defaults.",
                                    e2
                                );
                            }
                        }
                    } else {
                        tracing::warn!("No backup settings file found. Using defaults.");
                    }
                }
            }
        } else {
            tracing::info!("Settings file not found, using defaults");
        }

        Ok(Self::default())
    }

    /// Save settings to file
    ///
    /// Creates a .bak backup of the existing file before writing.
    /// Creates the config directory if it doesn't exist.
    pub fn save(&self) -> Result<()> {
        let settings_path = Self::get_settings_path()?;

        // Ensure parent directory exists
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Backup existing settings before overwriting
        if settings_path.exists() {
            let backup_path = settings_path.with_extension("json.bak");
            if let Err(e) = fs::copy(&settings_path, &backup_path) {
                tracing::warn!("Failed to create settings backup: {}", e);
            }
        }

        let json = serde_json::to_string_pretty(self)?;
        atomic_write(&settings_path, json.as_bytes())?;

        tracing::info!("Saved settings to: {:?}", settings_path);
        Ok(())
    }

    /// Get the path to the settings file
    ///
    /// Platform-specific:
    /// - Windows: %APPDATA%/Roaming/LoLShorts/settings.json
    /// - macOS: ~/Library/Application Support/LoLShorts/settings.json
    /// - Linux: ~/.config/LoLShorts/settings.json
    ///
    /// NOTE: this is a deliberately separate storage channel from
    /// `storage::Storage` (SQLite + media files under `dirs::data_dir()`).
    /// On Windows both resolve under `%APPDATA%\Roaming`, but on Linux
    /// `dirs::config_dir()` (`~/.config`) and `dirs::data_dir()`
    /// (`~/.local/share`) are genuinely different directories. They are
    /// intentionally NOT merged here to avoid a migration/data-loss risk for
    /// existing users -- see the storage-location note on `storage::Storage`
    /// for the other half of this split.
    fn get_settings_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("settings.json"))
    }

    /// Directory holding `settings.json`.
    ///
    /// `LOLSHORTS_CONFIG_DIR` overrides it. That escape hatch exists because the
    /// tests in this module WRITE AND DELETE the settings file, and they used to
    /// do it against the real one: a plain `cargo test` on a developer's machine
    /// silently destroyed that machine's saved settings. It was observed, not
    /// theorised -- the log went
    ///
    /// ```text
    /// 07:37:46  Loaded settings from: ...\LoLShorts\settings.json
    /// 07:42:29  Settings file not found, using defaults
    /// ```
    ///
    /// with nothing but a test run in between, and only `settings.json.bak`
    /// survived. (The `#[ignore]` on `test_save_and_load`, blaming a "race
    /// condition ... both use same settings file", was the same bug seen from
    /// the other side and worked around instead of fixed.)
    ///
    /// Unit tests get an automatic per-process temp directory so they can never
    /// reach real user data even if someone forgets the variable; integration
    /// tests, which link the library without `cfg(test)`, must set it.
    fn config_dir() -> Result<PathBuf> {
        if let Some(dir) = std::env::var_os("LOLSHORTS_CONFIG_DIR") {
            return Ok(PathBuf::from(dir));
        }

        #[cfg(test)]
        {
            return Ok(test_config_dir());
        }

        #[cfg(not(test))]
        {
            let config_dir = dirs::config_dir().ok_or(SettingsError::ConfigDirNotFound)?;
            Ok(config_dir.join("LoLShorts"))
        }
    }

    /// Reset settings to default and save
    pub fn reset_to_default() -> Result<Self> {
        let settings = Self::default();
        settings.save()?;
        tracing::info!("Settings reset to default");
        Ok(settings)
    }

    /// Load settings with platform-specific optimization
    ///
    /// Detects platform hardware and applies optimizations
    pub async fn load_with_platform_optimization() -> Result<Self> {
        let mut settings = Self::load()?;

        // Detect platform configuration
        let platform_config = PlatformConfig::detect().await?;

        // Apply platform-specific defaults for new settings
        settings.apply_platform_defaults(&platform_config);

        // Optimize settings based on hardware
        platform_config.optimize_settings(&mut settings);

        // Validate settings against platform capabilities
        platform_config.validate_settings(&settings)?;

        // Save optimized settings
        settings.save()?;

        tracing::info!(
            "Settings loaded and optimized for platform: {:?}",
            platform_config.platform
        );
        Ok(settings)
    }

    /// Apply platform-specific defaults
    fn apply_platform_defaults(&mut self, platform_config: &PlatformConfig) {
        // Override with platform-specific defaults
        let defaults = &platform_config.default_overrides;

        // Apply video settings if user hasn't customized
        if !self.is_customized_video() {
            self.video = defaults.video.clone();
        }

        // Apply audio settings if user hasn't customized
        if !self.is_customized_audio() {
            self.audio = defaults.audio.clone();
        }

        // Platform-specific overrides
        match platform_config.platform {
            super::platform_config::Platform::MacOS => {
                self.minimize_to_tray = false; // macOS uses dock instead of tray
            }
            super::platform_config::Platform::Linux => {
                self.minimize_to_tray = false; // Linux tray support varies
            }
            _ => {}
        }
    }

    /// Check if video settings have been customized by user
    fn is_customized_video(&self) -> bool {
        let default = RecordingSettings::default();
        self.video.resolution != default.video.resolution
            || self.video.frame_rate != default.video.frame_rate
            || self.video.bitrate_preset != default.video.bitrate_preset
            || self.video.codec != default.video.codec
            || self.video.encoder != default.video.encoder
    }

    /// Check if audio settings have been customized by user
    fn is_customized_audio(&self) -> bool {
        let default = RecordingSettings::default();
        self.audio.record_microphone != default.audio.record_microphone
            || self.audio.record_system_audio != default.audio.record_system_audio
            || self.audio.sample_rate != default.audio.sample_rate
            || self.audio.bitrate != default.audio.bitrate
    }

    /// Get migration status for settings version upgrades
    pub fn needs_migration() -> Result<bool> {
        let migration_path = Self::get_migration_path()?;
        Ok(!migration_path.exists())
    }

    /// Perform settings migration from previous version
    pub async fn migrate_settings() -> Result<Self> {
        tracing::info!("Starting settings migration...");

        // Load existing settings
        let mut settings = Self::load().unwrap_or_else(|_| {
            tracing::warn!("Could not load existing settings, using defaults");
            Self::default()
        });

        // Perform version-specific migrations
        settings = Self::migrate_from_v1_to_v2(settings);
        settings = Self::migrate_from_v2_to_v3(settings);

        // Apply platform optimization after migration
        let platform_config = PlatformConfig::detect().await?;
        platform_config.optimize_settings(&mut settings);

        // Save migrated settings
        settings.save()?;

        // Mark migration as complete
        Self::mark_migration_complete()?;

        tracing::info!("Settings migration completed successfully");
        Ok(settings)
    }

    /// Migration from version 1 to version 2
    fn migrate_from_v1_to_v2(mut settings: Self) -> Self {
        // Add new fields introduced in v2
        tracing::debug!("Migrating settings from v1 to v2");

        // Example: Add new clip timing settings if they don't exist
        if settings.clip_timing.event_timings.is_empty() {
            settings.clip_timing = RecordingSettings::default().clip_timing;
        }

        settings
    }

    /// Migration from version 2 to version 3
    fn migrate_from_v2_to_v3(mut settings: Self) -> Self {
        // Add new fields introduced in v3
        tracing::debug!("Migrating settings from v2 to v3");

        // Example: Update event filter settings
        if settings.event_filter.min_priority == 0 {
            settings.event_filter.min_priority = 1; // Fix invalid default
        }

        settings
    }

    /// Get migration marker file path
    fn get_migration_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().ok_or(SettingsError::ConfigDirNotFound)?;
        let lolshorts_dir = config_dir.join("LoLShorts");
        Ok(lolshorts_dir.join(".migrated_v3"))
    }

    /// Mark migration as complete
    fn mark_migration_complete() -> Result<()> {
        let migration_path = Self::get_migration_path()?;

        if let Some(parent) = migration_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&migration_path, "migrated")?;
        Ok(())
    }

    /// Export settings to file for backup
    pub fn export_settings(&self, path: &PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        tracing::info!("Settings exported to: {:?}", path);
        Ok(())
    }

    /// Import settings from file
    pub fn import_settings(path: &PathBuf) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let settings: RecordingSettings = serde_json::from_str(&json)?;

        // Validate imported settings
        if settings.event_filter.min_priority == 0 || settings.event_filter.min_priority > 5 {
            return Err(SettingsError::Validation(
                "Invalid event filter priority range".to_string(),
            ));
        }

        tracing::info!("Settings imported from: {:?}", path);
        Ok(settings)
    }

    /// Validate settings integrity
    pub fn validate_integrity(&self) -> Result<()> {
        // Validate event filter settings
        if self.event_filter.min_priority == 0 || self.event_filter.min_priority > 5 {
            return Err(SettingsError::Validation(
                "Event filter priority must be between 1 and 5".to_string(),
            ));
        }

        // Validate audio volume settings
        if self.audio.microphone_volume > 200 {
            return Err(SettingsError::Validation(
                "Microphone volume cannot exceed 200%".to_string(),
            ));
        }

        if self.audio.system_audio_volume > 200 {
            return Err(SettingsError::Validation(
                "System audio volume cannot exceed 200%".to_string(),
            ));
        }

        // Validate clip timing settings
        if self.clip_timing.default_pre_duration > 60 {
            return Err(SettingsError::Validation(
                "Pre-event duration cannot exceed 60 seconds".to_string(),
            ));
        }

        if self.clip_timing.default_post_duration > 60 {
            return Err(SettingsError::Validation(
                "Post-event duration cannot exceed 60 seconds".to_string(),
            ));
        }

        // Validate merge threshold
        if self.clip_timing.merge_time_threshold < 1.0
            || self.clip_timing.merge_time_threshold > 300.0
        {
            return Err(SettingsError::Validation(
                "Event merge threshold must be between 1 and 300 seconds".to_string(),
            ));
        }

        Ok(())
    }

    /// Get settings summary for diagnostics
    pub fn get_diagnostics_summary(&self) -> String {
        format!(
            r#"Settings Diagnostics:
- Event Filter: priority={}, kills={}, multikills={}
- Video: resolution={:?}, framerate={:?}, codec={:?}
- Audio: microphone={}, system_audio={}, bitrate={:?}
- Clip Timing: pre={}s, post={}s, merge={}
- General: auto_start={}, minimize_to_tray={}, notifications={}"#,
            self.event_filter.min_priority,
            self.event_filter.record_kills,
            self.event_filter.record_multikills,
            self.video.resolution,
            self.video.frame_rate,
            self.video.codec,
            self.audio.record_microphone,
            self.audio.record_system_audio,
            self.audio.bitrate,
            self.clip_timing.default_pre_duration,
            self.clip_timing.default_post_duration,
            self.clip_timing.merge_consecutive_events,
            self.auto_start_with_league,
            self.minimize_to_tray,
            self.show_notifications,
        )
    }
}

/// Per-process throwaway config directory for unit tests.
///
/// Kept alive for the whole test binary (leaked on purpose) so the directory is
/// not removed while a later test still reads from it. The `LoLShorts` component
/// is preserved because `test_settings_path` asserts on it -- and because the
/// real layout is what we want to exercise, just somewhere harmless.
#[cfg(test)]
fn test_config_dir() -> PathBuf {
    use std::sync::OnceLock;
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let base = tempfile::Builder::new()
            .prefix("lolshorts-settings-test-")
            .tempdir()
            .expect("temp dir for settings tests");
        let path = base.keep().join("LoLShorts");
        std::fs::create_dir_all(&path).expect("temp config dir");
        path
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn settings_tests_never_touch_the_real_config_directory() {
        // The guard for the data-loss bug documented on `config_dir`. If this
        // ever fails, `cargo test` is again writing where a user's settings live.
        let path = RecordingSettings::get_settings_path().unwrap();
        let real = dirs::config_dir().map(|d| d.join("LoLShorts").join("settings.json"));
        assert_ne!(
            Some(path.clone()),
            real,
            "테스트가 실제 사용자 설정 파일을 가리키고 있다: {}",
            path.display()
        );
    }

    #[test]
    fn test_settings_path() {
        let path = RecordingSettings::get_settings_path().unwrap();
        assert!(path.to_string_lossy().contains("LoLShorts"));
        assert!(path.to_string_lossy().ends_with("settings.json"));
    }

    #[test]
    #[ignore] // Ignored due to race condition with test_reset_to_default (both use same settings file)
    fn test_save_and_load() {
        // Cleanup any existing settings file first
        let path = RecordingSettings::get_settings_path().unwrap();
        if path.exists() {
            fs::remove_file(&path).ok();
        }

        let mut settings = RecordingSettings::default();
        settings.event_filter.min_priority = 3;
        settings.audio.microphone_volume = 150;

        // Save
        settings.save().unwrap();

        // Add delay and retry logic to handle race conditions with parallel tests
        let path = RecordingSettings::get_settings_path().unwrap();
        let mut retries = 5;
        while retries > 0
            && (!path.exists() || fs::read_to_string(&path).unwrap_or_default().is_empty())
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if !path.exists() || fs::read_to_string(&path).unwrap_or_default().is_empty() {
                // Re-save if file was deleted or corrupted by parallel test
                settings.save().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            retries -= 1;
        }

        // Verify file exists and has content
        assert!(path.exists(), "Settings file should exist after save");
        let content = fs::read_to_string(&path).unwrap();
        assert!(
            !content.is_empty(),
            "Settings file should not be empty after save"
        );

        // Load
        let loaded = RecordingSettings::load().unwrap();
        assert_eq!(loaded.event_filter.min_priority, 3);
        assert_eq!(loaded.audio.microphone_volume, 150);

        // Cleanup
        let path = RecordingSettings::get_settings_path().unwrap();
        if path.exists() {
            fs::remove_file(path).ok();
        }
    }

    #[test]
    fn test_reset_to_default() {
        // Cleanup any existing settings file first
        let path = RecordingSettings::get_settings_path().unwrap();
        if path.exists() {
            fs::remove_file(&path).ok();
        }

        // Create modified settings
        let mut settings = RecordingSettings::default();
        settings.event_filter.min_priority = 5;
        settings.save().unwrap();

        // Reset
        let reset_settings = RecordingSettings::reset_to_default().unwrap();
        assert_eq!(reset_settings.event_filter.min_priority, 1); // default value

        // Verify persisted
        let loaded = RecordingSettings::load().unwrap();
        assert_eq!(loaded.event_filter.min_priority, 1);

        // Cleanup
        let path = RecordingSettings::get_settings_path().unwrap();
        if path.exists() {
            fs::remove_file(path).ok();
        }
    }

    #[test]
    fn test_atomic_write_creates_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        atomic_write(&path, b"{}").unwrap();
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn test_atomic_write_no_tmp_file_remains() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        atomic_write(&path, b"{}").unwrap();
        let tmp_path = path.with_extension("json.tmp");
        assert!(!tmp_path.exists());
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }
}
