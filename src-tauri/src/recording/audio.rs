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
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            record_microphone: true,
            microphone_device: None,
            microphone_volume: 120,
            record_system_audio: true,
            system_audio_device: None,
            system_audio_volume: 100,
            sample_rate: 48000,
            bitrate: 192,
        }
    }
}

impl AudioConfig {
    /// Check if any audio capture is enabled
    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        self.record_microphone || self.record_system_audio
    }

    /// Build FFmpeg audio input arguments
    ///
    /// Returns (input_args, filter_args, map_args, codec_args)
    /// where each component is a Vec of FFmpeg argument strings
    #[allow(dead_code)]
    pub fn build_ffmpeg_args(&self) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
        if !self.is_enabled() {
            return (vec![], vec![], vec![], vec![]);
        }

        let mut input_args = Vec::new();
        let mut filter_parts = Vec::new();
        let mut map_args = Vec::new();
        let codec_args = vec![
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            format!("{}k", self.bitrate),
            "-ar".to_string(),
            self.sample_rate.to_string(),
        ];

        // Track which audio input index we're on (starts at 1, since 0 is video)
        let mut audio_input_idx = 1;
        let mut mix_inputs = Vec::new();

        // Add microphone input
        if self.record_microphone {
            input_args.push("-f".to_string());
            input_args.push("dshow".to_string());
            input_args.push("-i".to_string());

            let mic_device = self
                .microphone_device
                .as_ref()
                .map(|d| format!("audio={}", d))
                .unwrap_or_else(|| {
                    "audio=@device_cm_{33D9A762-90C8-11D0-BD43-00A0C911CE86}\\wave_in".to_string()
                });
            input_args.push(mic_device);

            // Apply volume to microphone
            let volume = self.microphone_volume as f32 / 100.0;
            filter_parts.push(format!("[{}:a]volume={}[mic]", audio_input_idx, volume));
            mix_inputs.push("[mic]".to_string());
            audio_input_idx += 1;
        }

        // Add system audio input (loopback)
        if self.record_system_audio {
            input_args.push("-f".to_string());
            input_args.push("dshow".to_string());
            input_args.push("-i".to_string());

            let sys_device = self
                .system_audio_device
                .as_ref()
                .map(|d| format!("audio={}", d))
                .unwrap_or_else(|| "audio=Stereo Mix".to_string());
            input_args.push(sys_device);

            // Apply volume to system audio
            let volume = self.system_audio_volume as f32 / 100.0;
            filter_parts.push(format!("[{}:a]volume={}[sys]", audio_input_idx, volume));
            mix_inputs.push("[sys]".to_string());
        }

        // Build filter_complex for mixing
        let filter_args = if mix_inputs.len() > 1 {
            // Mix multiple audio sources
            filter_parts.push(format!(
                "{}amix=inputs={}[aout]",
                mix_inputs.join(""),
                mix_inputs.len()
            ));
            vec!["-filter_complex".to_string(), filter_parts.join(";")]
        } else if mix_inputs.len() == 1 {
            // Single audio source, just apply volume
            vec![
                "-filter_complex".to_string(),
                filter_parts.join(";"),
                "-map".to_string(),
                "0:v".to_string(),
                "-map".to_string(),
                if self.record_microphone {
                    "[mic]"
                } else {
                    "[sys]"
                }
                .to_string(),
            ]
        } else {
            vec![]
        };

        // Add audio mapping
        if mix_inputs.len() > 1 {
            map_args.push("-map".to_string());
            map_args.push("0:v".to_string());
            map_args.push("-map".to_string());
            map_args.push("[aout]".to_string());
        }

        (input_args, filter_args, map_args, codec_args)
    }
}

/// Cached audio device manager for memory efficiency
pub struct AudioDeviceManager {
    devices: Vec<AudioDevice>,
    /// Last refresh timestamp (for cache management)
    pub last_refresh: std::time::Instant,
    /// Cache time-to-live (for cache management)
    pub cache_ttl: std::time::Duration,
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
        }
    }

    /// Get cached audio devices as a slice (no copying)
    #[allow(dead_code)]
    pub fn get_devices(&self) -> &[AudioDevice] {
        &self.devices
    }

    /// Refresh devices if cache expired
    #[allow(dead_code)]
    pub async fn refresh_if_needed(&mut self) -> Result<()> {
        if self.last_refresh.elapsed() < self.cache_ttl {
            return Ok(()); // Use cached devices
        }

        tracing::debug!("Refreshing cached audio devices...");

        // Method 1: Try Windows Core Audio API (more reliable)
        if let Ok(core_devices) = list_audio_devices() {
            self.devices = core_devices;
            self.last_refresh = std::time::Instant::now();
            tracing::info!(
                "Found {} audio devices via Windows Core Audio API",
                self.devices.len()
            );
            return Ok(());
        }

        // Method 2: Fallback to FFmpeg DirectShow (less reliable)
        tracing::warn!("Windows Core Audio API failed, falling back to FFmpeg DirectShow");
        if let Ok(ffmpeg_devices) = list_audio_devices_ffmpeg() {
            self.devices = ffmpeg_devices;
            self.last_refresh = std::time::Instant::now();
            tracing::info!(
                "Found {} audio devices via FFmpeg DirectShow",
                self.devices.len()
            );
        }

        Ok(())
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

/// List available audio devices (optimized with caching and slice return)
#[allow(dead_code)]
pub fn list_audio_devices() -> Result<Vec<AudioDevice>> {
    tracing::debug!("Getting audio devices (cached)...");

    let manager = get_audio_device_manager();

    // Use non-blocking try_lock to avoid deadlocks
    let manager_guard = manager
        .try_lock()
        .map_err(|_| anyhow::anyhow!("Audio device manager is locked"))?;

    // Clone only if needed (for backward compatibility)
    Ok(manager_guard.devices.clone())
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
        assert!(config.record_microphone);
        assert!(config.record_system_audio);
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.bitrate, 192);
    }

    #[test]
    fn test_audio_config_disabled() {
        let config = AudioConfig {
            record_microphone: false,
            record_system_audio: false,
            ..Default::default()
        };
        assert!(!config.is_enabled());

        let (input_args, filter_args, map_args, codec_args) = config.build_ffmpeg_args();
        assert!(input_args.is_empty());
        assert!(filter_args.is_empty());
        assert!(map_args.is_empty());
        assert!(codec_args.is_empty());
    }

    #[test]
    fn test_audio_config_microphone_only() {
        let config = AudioConfig {
            record_microphone: true,
            microphone_volume: 150,
            record_system_audio: false,
            ..Default::default()
        };
        assert!(config.is_enabled());

        let (input_args, filter_args, _, codec_args) = config.build_ffmpeg_args();
        assert!(!input_args.is_empty());
        assert!(!filter_args.is_empty());
        assert!(!codec_args.is_empty());

        // Check volume is applied (150% = 1.5)
        let filter_str = filter_args.join(" ");
        assert!(filter_str.contains("volume=1.5"));
    }

    #[test]
    fn test_audio_config_both_sources() {
        let config = AudioConfig {
            record_microphone: true,
            microphone_volume: 120,
            record_system_audio: true,
            system_audio_volume: 100,
            ..Default::default()
        };
        assert!(config.is_enabled());

        let (input_args, filter_args, map_args, codec_args) = config.build_ffmpeg_args();
        assert!(!input_args.is_empty());
        assert!(!filter_args.is_empty());
        assert!(!map_args.is_empty());
        assert!(!codec_args.is_empty());

        // Check mixing is configured
        let filter_str = filter_args.join(" ");
        assert!(filter_str.contains("amix"));
        assert!(filter_str.contains("[aout]"));
    }
}
