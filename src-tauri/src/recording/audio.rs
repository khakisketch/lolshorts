use crate::utils::ffmpeg::get_ffmpeg_path;
use crate::utils::process::command_output_with_timeout;
/// Audio capture utilities for Windows using DirectShow
///
/// This module provides:
/// - Audio device enumeration via FFmpeg/DirectShow
/// - Audio input configuration for microphone and system audio
/// - Volume control and mixing parameters
/// - FFmpeg command builder for audio capture
use anyhow::{Context as AnyhowContext, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

const AUDIO_DEVICE_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Audio device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub device_type: AudioDeviceType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDeviceType {
    Microphone,
    SystemAudio,
}

/// Audio capture configuration
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Enable microphone recording
    pub record_microphone: bool,
    /// Microphone device name (None = default device)
    pub microphone_device: Option<String>,
    /// Microphone volume (0-200%)
    pub microphone_volume: u8,

    /// Enable system audio recording
    pub record_system_audio: bool,
    /// System audio device name (None = default device)
    pub system_audio_device: Option<String>,
    /// System audio volume (0-200%)
    pub system_audio_volume: u8,

    /// Audio sample rate
    pub sample_rate: u32,
    /// Audio bitrate in kbps
    pub bitrate: u32,

    /// Explicit WASAPI loopback device ID (from `AudioSettings::audio_device_id`).
    /// Takes priority over `system_audio_device` when selecting the WASAPI
    /// capture device (None = fall back to `system_audio_device`, then the
    /// system default output device).
    pub audio_device_id: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            // Off by default: microphone capture is not wired into the
            // actual Windows recording pipeline (`integration_backend::
            // segment_recorder` never reads `record_microphone`), so
            // defaulting this to `true` misled users into believing they
            // were being recorded when they weren't.
            record_microphone: false,
            microphone_device: None,
            microphone_volume: 120,
            record_system_audio: true,
            system_audio_device: None,
            system_audio_volume: 100,
            sample_rate: 48000,
            bitrate: 192,
            audio_device_id: None,
        }
    }
}

impl AudioConfig {
    /// Check if any audio capture is enabled
    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.record_microphone || self.record_system_audio
    }

    // NOTE: `build_ffmpeg_args` (a DirectShow-based mic+system amix builder) was
    // removed. Microphone and system-audio muxing now lives in
    // `integration_backend::segment_recorder::save_clip`, which mixes the separately
    // captured WASAPI/mic wavs (per-input `-ss` anchoring + `amix`) at clip-export
    // time. This DirectShow builder was never wired into that pipeline and had no
    // callers, so it was dead code.
}

/// Cached audio device manager for memory efficiency
pub struct AudioDeviceManager {
    devices: Vec<AudioDevice>,
    /// Last refresh timestamp (for cache management)
    pub last_refresh: std::time::Instant,
    /// Cache time-to-live (for cache management)
    pub cache_ttl: std::time::Duration,
    /// True once at least one refresh attempt has completed successfully.
    /// `last_refresh` is initialized to "now" at construction time, which
    /// would otherwise look "fresh" under the TTL check and make
    /// `refresh_if_needed` skip enumeration entirely on its very first
    /// call — silently reporting the still-empty `devices` cache as
    /// authoritative before any real enumeration ever ran.
    initialized: bool,
}

impl Default for AudioDeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioDeviceManager {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            last_refresh: std::time::Instant::now(),
            cache_ttl: std::time::Duration::from_secs(60), // Cache for 60 seconds
            initialized: false,
        }
    }

    /// Get cached audio devices as a slice (no copying)
    #[allow(dead_code)]
    pub fn get_devices(&self) -> &[AudioDevice] {
        &self.devices
    }

    /// Whether `refresh_if_needed` may skip enumeration and reuse the
    /// cached device list. Split out (pure, no I/O) so the TTL/first-call
    /// logic is unit-testable without shelling out to FFmpeg.
    fn should_skip_refresh(&self) -> bool {
        self.initialized && self.last_refresh.elapsed() < self.cache_ttl
    }

    /// Refresh devices if cache expired (or has never been populated).
    ///
    /// This used to try a "Windows Core Audio API" step first by calling
    /// the free `list_audio_devices()` function below — but that function
    /// only ever *reads* this same manager's cache through
    /// `AUDIO_DEVICE_MANAGER.try_lock()`. Called from here, while the
    /// caller already holds this exact mutex via `.lock().await` to get
    /// `&mut self`, that `try_lock()` always fails (tokio's `Mutex` is not
    /// reentrant), so the "Core Audio" branch was permanently dead and
    /// every refresh silently fell through to the FFmpeg DirectShow
    /// fallback anyway. Enumerate via DirectShow directly instead — no
    /// nested lock, so no reentrancy, and the fallback fills the cache
    /// immediately in the same call rather than depending on a first
    /// branch that could never succeed.
    #[allow(dead_code)]
    pub async fn refresh_if_needed(&mut self) -> Result<()> {
        if self.should_skip_refresh() {
            return Ok(()); // Use cached devices
        }

        tracing::debug!("Refreshing cached audio devices via FFmpeg DirectShow...");

        match list_audio_devices_ffmpeg() {
            Ok(devices) => {
                self.devices = devices;
                self.last_refresh = std::time::Instant::now();
                self.initialized = true;
                tracing::info!(
                    "Found {} audio devices via FFmpeg DirectShow",
                    self.devices.len()
                );
                Ok(())
            }
            Err(e) => {
                // Leave `initialized` false and `last_refresh` untouched so
                // the *next* call retries immediately instead of treating
                // this failed attempt as a "fresh" (but empty) cache.
                tracing::warn!("FFmpeg DirectShow audio device enumeration failed: {}", e);
                Err(e)
            }
        }
    }

    /// Force refresh regardless of cache TTL
    #[allow(dead_code)]
    pub async fn force_refresh(&mut self) -> Result<()> {
        self.last_refresh = std::time::Instant::now() - self.cache_ttl;
        self.refresh_if_needed().await
    }
}

/// Global audio device manager instance (async thread-safe)
static AUDIO_DEVICE_MANAGER: std::sync::OnceLock<tokio::sync::Mutex<AudioDeviceManager>> =
    std::sync::OnceLock::new();

/// Get global audio device manager
pub fn get_audio_device_manager() -> &'static tokio::sync::Mutex<AudioDeviceManager> {
    AUDIO_DEVICE_MANAGER.get_or_init(|| tokio::sync::Mutex::new(AudioDeviceManager::new()))
}

/// Get audio devices, refreshing the cache first if it is stale or has
/// never been populated.
///
/// Previously this only read whatever was already cached (via
/// `try_lock` + clone, without ever refreshing), so the very first call
/// after app launch — before anything else happened to trigger a
/// refresh — returned an empty `Ok(vec![])` instead of populating the
/// list. It now performs the refresh itself under a single lock
/// acquisition, so callers always get a populated (or honestly failed)
/// result on the first call.
#[allow(dead_code)]
pub async fn list_audio_devices() -> Result<Vec<AudioDevice>> {
    let manager = get_audio_device_manager();
    let mut manager_guard = manager.lock().await;
    manager_guard.refresh_if_needed().await?;
    Ok(manager_guard.get_devices().to_vec())
}

/// Get audio devices as slice (memory efficient - no copying)
/// Get a clone of audio devices - safe alternative to transmute
/// Returns owned Vec instead of static slice to avoid undefined behavior
#[allow(dead_code)]
pub fn get_audio_devices_clone() -> Result<Vec<AudioDevice>> {
    let manager = get_audio_device_manager();

    let manager_guard = manager
        .try_lock()
        .map_err(|_| anyhow::anyhow!("Audio device manager is locked"))?;

    // Clone the devices to avoid unsafe transmute
    // The performance cost is minimal for the small number of audio devices
    Ok(manager_guard.devices.clone())
}

/// Fallback method using FFmpeg DirectShow (original implementation)
#[allow(dead_code)]
pub fn list_audio_devices_ffmpeg() -> Result<Vec<AudioDevice>> {
    tracing::debug!("Listing DirectShow audio devices...");

    let ffmpeg_path =
        get_ffmpeg_path().context("Failed to find FFmpeg for audio device listing")?;

    let mut command = Command::new(ffmpeg_path);
    command.args(["-list_devices", "true", "-f", "dshow", "-i", "dummy"]);

    let output = command_output_with_timeout(
        command,
        AUDIO_DEVICE_PROBE_TIMEOUT,
        "FFmpeg audio device listing",
    )
    .context("Failed to execute ffmpeg for device listing")?;

    // FFmpeg outputs device list to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut devices = Vec::new();
    let mut in_audio_section = false;

    for line in stderr.lines() {
        if line.contains("DirectShow audio devices") {
            in_audio_section = true;
            continue;
        }

        if line.contains("DirectShow video devices") {
            break;
        }

        if in_audio_section && line.contains('"') {
            // Extract device name from format: [dshow @ ...] "Device Name"
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    let name = line[start + 1..start + 1 + end].to_string();

                    // Categorize by common device name patterns
                    let device_type = if name.to_lowercase().contains("mic")
                        || name.to_lowercase().contains("microphone")
                        || name.to_lowercase().contains("input")
                    {
                        AudioDeviceType::Microphone
                    } else {
                        AudioDeviceType::SystemAudio
                    };

                    devices.push(AudioDevice { name, device_type });
                }
            }
        }
    }

    tracing::info!(
        "Found {} audio devices via FFmpeg DirectShow",
        devices.len()
    );
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_config_default() {
        let config = AudioConfig::default();
        // Off by default: microphone capture isn't wired into the actual
        // recording pipeline yet, so it must not silently default to "on"
        // and mislead users into thinking they're being recorded.
        assert!(!config.record_microphone);
        assert!(config.record_system_audio);
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.bitrate, 192);
    }

    #[test]
    fn test_audio_config_disabled_is_not_enabled() {
        let config = AudioConfig {
            record_microphone: false,
            record_system_audio: false,
            ..Default::default()
        };
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_audio_config_enabled_when_only_microphone() {
        let config = AudioConfig {
            record_microphone: true,
            microphone_volume: 150,
            record_system_audio: false,
            ..Default::default()
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn test_audio_config_enabled_when_only_system_audio() {
        let config = AudioConfig {
            record_microphone: false,
            record_system_audio: true,
            ..Default::default()
        };
        assert!(config.is_enabled());
    }

    // ---- AudioDeviceManager: refresh-skip decision (regression for the
    // reentrant try_lock / cold-cache bug) ----

    #[test]
    fn audio_device_manager_new_is_not_initialized() {
        // `last_refresh` is set to "now" at construction, which would look
        // "fresh" under a naive TTL check even though no enumeration has
        // ever run — `initialized` must gate that.
        let manager = AudioDeviceManager::new();
        assert!(!manager.initialized);
    }

    #[test]
    fn audio_device_manager_never_skips_refresh_before_first_successful_refresh() {
        let manager = AudioDeviceManager::new();
        // Fresh construction: `last_refresh.elapsed() < cache_ttl` would be
        // true here, so only the `initialized` gate prevents a skip.
        assert!(!manager.should_skip_refresh());
    }

    #[test]
    fn audio_device_manager_skips_refresh_when_initialized_and_within_ttl() {
        let mut manager = AudioDeviceManager::new();
        manager.initialized = true;
        manager.last_refresh = std::time::Instant::now();
        manager.cache_ttl = std::time::Duration::from_secs(60);
        assert!(manager.should_skip_refresh());
    }

    #[test]
    fn audio_device_manager_does_not_skip_refresh_once_ttl_expires() {
        let mut manager = AudioDeviceManager::new();
        manager.initialized = true;
        manager.cache_ttl = std::time::Duration::from_millis(1);
        manager.last_refresh = std::time::Instant::now() - std::time::Duration::from_secs(1);
        assert!(!manager.should_skip_refresh());
    }
}
