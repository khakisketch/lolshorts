#![allow(dead_code)]

pub mod detection;
pub mod types;

pub use types::{
    AudioDevice, AudioDeviceInfo, AudioRecommendations, CpuInfo, DisplayInfo, GpuInfo, GpuVendor,
    HardwareCapabilities, MemoryInfo, PerformanceRecommendations, Platform, PlatformConfig,
    PlatformConfigError, PlatformFeatures, RecommendedSettings, Result, StorageInfo,
    StorageRecommendations, VideoRecommendations,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::models::{EncoderPreference, RecordingSettings};

    #[test]
    fn test_platform_detection() {
        #[cfg(target_os = "windows")]
        {
            // detect_platform is private; test via the public type
            let config = PlatformConfig {
                platform: Platform::Windows,
                hardware: HardwareCapabilities {
                    cpu: CpuInfo {
                        model: "Test CPU".to_string(),
                        cores: 8,
                        logical_cores: 8,
                        max_frequency: 3600.0,
                        has_avx: true,
                        has_avx2: true,
                        architecture: "x86_64".to_string(),
                    },
                    gpu: vec![],
                    memory: MemoryInfo {
                        total_gb: 16.0,
                        available_gb: 8.0,
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
                        free_space_gb: 250.0,
                        install_drive: "C:".to_string(),
                        temp_directory: "/tmp".to_string(),
                    },
                },
                default_overrides: RecordingSettings::default(),
                features: PlatformFeatures {
                    supports_windows_capture: true,
                    supports_ffmpeg_native: true,
                    supports_core_graphics: false,
                    supports_hardware_acceleration: false,
                    supports_system_tray: true,
                    supports_global_hotkeys: true,
                    supports_file_associations: true,
                    supports_auto_start: true,
                    supports_notifications: true,
                    supports_api_detection: true,
                    supports_window_enumeration: true,
                },
                recommended_settings: RecommendedSettings {
                    video: VideoRecommendations {
                        recommended_encoder: EncoderPreference::Auto,
                        recommended_codec: "h264".to_string(),
                        recommended_bitrate_kbps: 10000,
                        recommended_resolution: "1920x1080".to_string(),
                        recommended_frame_rate: "60".to_string(),
                        maximum_recording_hours: 12.0,
                    },
                    audio: AudioRecommendations {
                        recommended_sample_rate: "48000".to_string(),
                        recommended_bitrate: "192".to_string(),
                        max_channels: 2,
                        enable_microphone_by_default: false,
                        enable_system_audio_by_default: true,
                    },
                    performance: PerformanceRecommendations {
                        enable_hardware_acceleration: true,
                        recommended_buffer_size_mb: 256,
                        recommended_temp_cleanup_interval_minutes: 30,
                        recommended_concurrent_clips: 3,
                        enable_performance_monitoring: true,
                    },
                    storage: StorageRecommendations {
                        recommended_clips_directory: "/test".to_string(),
                        minimum_free_space_gb: 10.0,
                        recommended_cleanup_threshold_gb: 50.0,
                        enable_auto_cleanup: false,
                    },
                },
            };
            assert_eq!(config.platform, Platform::Windows);
            assert!(config.features.supports_windows_capture);
        }
    }

    #[test]
    fn test_gpu_vendor_serialization() {
        let vendor = GpuVendor::Nvidia;
        let serialized = serde_json::to_string(&vendor).unwrap();
        let deserialized: GpuVendor = serde_json::from_str(&serialized).unwrap();
        assert_eq!(vendor, deserialized);
    }
}
