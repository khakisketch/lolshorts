#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::super::models::{EncoderPreference, RecordingSettings};

#[derive(Debug, Error)]
pub enum PlatformConfigError {
    #[error("Platform not supported: {0}")]
    UnsupportedPlatform(String),

    #[error("Hardware detection failed: {0}")]
    HardwareDetection(String),

    #[error("Configuration validation failed: {0}")]
    Validation(String),

    #[error("Settings migration failed: {0}")]
    Migration(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, PlatformConfigError>;

/// Platform-specific configuration overrides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// Platform identifier
    pub platform: Platform,

    /// Hardware capabilities
    pub hardware: HardwareCapabilities,

    /// Default settings overrides
    pub default_overrides: RecordingSettings,

    /// Platform-specific feature flags
    pub features: PlatformFeatures,

    /// Recommended settings based on hardware
    pub recommended_settings: RecommendedSettings,
}

/// Platform enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
}

/// Hardware capabilities detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareCapabilities {
    /// CPU information
    pub cpu: CpuInfo,

    /// GPU information
    pub gpu: Vec<GpuInfo>,

    /// Memory information
    pub memory: MemoryInfo,

    /// Display information
    pub displays: Vec<DisplayInfo>,

    /// Audio devices
    pub audio_devices: AudioDeviceInfo,

    /// Storage information
    pub storage: StorageInfo,
}

/// CPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub model: String,
    pub cores: usize,
    pub logical_cores: usize,
    pub max_frequency: f64,
    pub has_avx: bool,
    pub has_avx2: bool,
    pub architecture: String,
}

/// GPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: GpuVendor,
    pub memory_mb: u64,
    pub driver_version: String,
    pub is_primary: bool,
    pub supports_encoding: bool,
    pub supports_nvenc: bool,
    pub supports_amf: bool,
    pub supports_qsv: bool,
}

/// GPU vendor
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Unknown,
}

/// Memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_gb: f64,
    pub available_gb: f64,
    pub speed_mhz: Option<f64>,
}

/// Display information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub resolution: (u32, u32),
    pub refresh_rate: f64,
    pub is_primary: bool,
    pub scaling_factor: f64,
}

/// Audio device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub input_devices: Vec<AudioDevice>,
    pub output_devices: Vec<AudioDevice>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

/// Audio device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub channels: u32,
    pub sample_rate: u32,
    pub is_default: bool,
}

/// Storage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    pub total_space_gb: f64,
    pub free_space_gb: f64,
    pub install_drive: String,
    pub temp_directory: String,
}

/// Platform-specific feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformFeatures {
    pub supports_windows_capture: bool,
    pub supports_ffmpeg_native: bool,
    pub supports_core_graphics: bool,
    pub supports_hardware_acceleration: bool,
    pub supports_system_tray: bool,
    pub supports_global_hotkeys: bool,
    pub supports_file_associations: bool,
    pub supports_auto_start: bool,
    pub supports_notifications: bool,
    pub supports_api_detection: bool,
    pub supports_window_enumeration: bool,
}

/// Recommended settings based on hardware
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedSettings {
    pub video: VideoRecommendations,
    pub audio: AudioRecommendations,
    pub performance: PerformanceRecommendations,
    pub storage: StorageRecommendations,
}

/// Video encoding recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoRecommendations {
    pub recommended_encoder: EncoderPreference,
    pub recommended_codec: String,
    pub recommended_bitrate_kbps: u32,
    pub recommended_resolution: String,
    pub recommended_frame_rate: String,
    pub maximum_recording_hours: f64,
}

/// Audio recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRecommendations {
    pub recommended_sample_rate: String,
    pub recommended_bitrate: String,
    pub max_channels: u32,
    pub enable_microphone_by_default: bool,
    pub enable_system_audio_by_default: bool,
}

/// Performance recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecommendations {
    pub enable_hardware_acceleration: bool,
    pub recommended_buffer_size_mb: u32,
    pub recommended_temp_cleanup_interval_minutes: u32,
    pub recommended_concurrent_clips: u32,
    pub enable_performance_monitoring: bool,
}

/// Storage recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageRecommendations {
    pub recommended_clips_directory: String,
    pub minimum_free_space_gb: f64,
    pub recommended_cleanup_threshold_gb: f64,
    pub enable_auto_cleanup: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Platform ----

    #[test]
    fn platform_windows_equals_windows() {
        assert_eq!(Platform::Windows, Platform::Windows);
    }

    #[test]
    fn platform_macos_equals_macos() {
        assert_eq!(Platform::MacOS, Platform::MacOS);
    }

    #[test]
    fn platform_linux_equals_linux() {
        assert_eq!(Platform::Linux, Platform::Linux);
    }

    #[test]
    fn platform_windows_not_equal_to_linux() {
        assert_ne!(Platform::Windows, Platform::Linux);
    }

    #[test]
    fn platform_macos_not_equal_to_windows() {
        assert_ne!(Platform::MacOS, Platform::Windows);
    }

    // ---- GpuVendor ----

    #[test]
    fn gpu_vendor_nvidia_equals_nvidia() {
        assert_eq!(GpuVendor::Nvidia, GpuVendor::Nvidia);
    }

    #[test]
    fn gpu_vendor_amd_equals_amd() {
        assert_eq!(GpuVendor::Amd, GpuVendor::Amd);
    }

    #[test]
    fn gpu_vendor_intel_equals_intel() {
        assert_eq!(GpuVendor::Intel, GpuVendor::Intel);
    }

    #[test]
    fn gpu_vendor_nvidia_not_equal_to_amd() {
        assert_ne!(GpuVendor::Nvidia, GpuVendor::Amd);
    }

    #[test]
    fn gpu_vendor_unknown_equals_unknown() {
        assert_eq!(GpuVendor::Unknown, GpuVendor::Unknown);
    }

    // ---- PlatformConfigError Display ----

    #[test]
    fn platform_config_error_unsupported_platform_display_contains_platform_name() {
        let err = PlatformConfigError::UnsupportedPlatform("FreeBSD".to_string());
        let msg = err.to_string();
        assert!(
            msg.contains("FreeBSD"),
            "display should include platform name, got: {}",
            msg
        );
    }

    #[test]
    fn platform_config_error_hardware_detection_display_contains_reason() {
        let err = PlatformConfigError::HardwareDetection("GPU not found".to_string());
        let msg = err.to_string();
        assert!(
            msg.contains("GPU not found"),
            "display should include reason, got: {}",
            msg
        );
    }

    #[test]
    fn platform_config_error_validation_display_contains_reason() {
        let err = PlatformConfigError::Validation("invalid bitrate".to_string());
        let msg = err.to_string();
        assert!(
            msg.contains("invalid bitrate"),
            "display should include reason, got: {}",
            msg
        );
    }

    #[test]
    fn platform_config_error_migration_display_contains_reason() {
        let err = PlatformConfigError::Migration("schema mismatch".to_string());
        let msg = err.to_string();
        assert!(
            msg.contains("schema mismatch"),
            "display should include reason, got: {}",
            msg
        );
    }

    #[test]
    fn platform_config_error_io_display_non_empty() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = PlatformConfigError::Io(io_err);
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }

    #[test]
    fn platform_config_error_json_display_non_empty() {
        let json_err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>("bad json").unwrap_err();
        let err = PlatformConfigError::Json(json_err);
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }

    // ---- HardwareCapabilities construction ----

    #[test]
    fn hardware_capabilities_can_be_constructed_with_empty_gpu_list() {
        let caps = HardwareCapabilities {
            cpu: CpuInfo {
                model: "Test CPU".to_string(),
                cores: 8,
                logical_cores: 16,
                max_frequency: 3.6,
                has_avx: true,
                has_avx2: true,
                architecture: "x86_64".to_string(),
            },
            gpu: vec![],
            memory: MemoryInfo {
                total_gb: 32.0,
                available_gb: 16.0,
                speed_mhz: Some(3200.0),
            },
            displays: vec![],
            audio_devices: AudioDeviceInfo {
                input_devices: vec![],
                output_devices: vec![],
                default_input: None,
                default_output: None,
            },
            storage: StorageInfo {
                total_space_gb: 500.0,
                free_space_gb: 200.0,
                install_drive: "C:".to_string(),
                temp_directory: "C:\\Temp".to_string(),
            },
        };

        assert_eq!(caps.cpu.model, "Test CPU");
        assert_eq!(caps.cpu.cores, 8);
        assert!(caps.gpu.is_empty());
        assert_eq!(caps.memory.total_gb, 32.0);
    }

    #[test]
    fn hardware_capabilities_can_be_constructed_with_gpu_info() {
        let gpu = GpuInfo {
            name: "NVIDIA RTX 4080".to_string(),
            vendor: GpuVendor::Nvidia,
            memory_mb: 16384,
            driver_version: "535.0".to_string(),
            is_primary: true,
            supports_encoding: true,
            supports_nvenc: true,
            supports_amf: false,
            supports_qsv: false,
        };

        assert_eq!(gpu.vendor, GpuVendor::Nvidia);
        assert!(gpu.supports_nvenc);
        assert!(!gpu.supports_amf);
        assert_eq!(gpu.memory_mb, 16384);
    }

    // ---- GpuVendor serialization ----

    #[test]
    fn gpu_vendor_serializes_and_deserializes_correctly() {
        let vendor = GpuVendor::Nvidia;
        let json = serde_json::to_string(&vendor).expect("serialization should succeed");
        let restored: GpuVendor =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(restored, GpuVendor::Nvidia);
    }

    // ---- Platform serialization ----

    #[test]
    fn platform_serializes_and_deserializes_correctly() {
        let platform = Platform::Windows;
        let json = serde_json::to_string(&platform).expect("serialization should succeed");
        let restored: Platform =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(restored, Platform::Windows);
    }
}
