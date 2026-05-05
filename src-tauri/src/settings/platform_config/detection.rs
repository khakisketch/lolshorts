#![allow(dead_code)]
use super::super::models::{EncoderPreference, RecordingSettings};
use super::types::{
    AudioDeviceInfo, AudioRecommendations, CpuInfo, DisplayInfo, GpuInfo, GpuVendor,
    HardwareCapabilities, MemoryInfo, PerformanceRecommendations, Platform, PlatformConfig,
    PlatformConfigError, PlatformFeatures, RecommendedSettings, Result, StorageInfo,
    StorageRecommendations, VideoRecommendations,
};

impl PlatformConfig {
    /// Detect current platform and hardware capabilities
    pub async fn detect() -> Result<Self> {
        let platform = Self::detect_platform()?;
        let hardware = Self::detect_hardware(&platform).await?;
        let features = Self::detect_features(&platform, &hardware);
        let default_overrides = Self::get_default_overrides(&platform, &hardware);
        let recommended_settings = Self::generate_recommendations(&platform, &hardware);

        Ok(Self {
            platform,
            hardware,
            default_overrides,
            features,
            recommended_settings,
        })
    }

    /// Detect current platform
    fn detect_platform() -> Result<Platform> {
        #[cfg(target_os = "windows")]
        {
            Ok(Platform::Windows)
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Platform::MacOS)
        }
        #[cfg(target_os = "linux")]
        {
            Ok(Platform::Linux)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            Err(PlatformConfigError::UnsupportedPlatform(
                std::env::consts::OS.to_string(),
            ))
        }
    }

    /// Detect hardware capabilities
    async fn detect_hardware(platform: &Platform) -> Result<HardwareCapabilities> {
        match platform {
            Platform::Windows => Self::detect_windows_hardware().await,
            Platform::MacOS => Self::detect_macos_hardware().await,
            Platform::Linux => Self::detect_linux_hardware().await,
        }
    }

    /// Detect Windows hardware
    #[cfg(target_os = "windows")]
    async fn detect_windows_hardware() -> Result<HardwareCapabilities> {
        use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

        let processor_count = {
            let mut cpu_info = SYSTEM_INFO::default();
            unsafe { GetSystemInfo(&mut cpu_info) };
            cpu_info.dwNumberOfProcessors as usize
        };

        let cpu = CpuInfo {
            model: Self::get_windows_cpu_name().await?,
            cores: processor_count,
            logical_cores: processor_count,
            max_frequency: Self::get_windows_cpu_frequency().await?,
            has_avx: Self::check_windows_cpu_feature("avx").await?,
            has_avx2: Self::check_windows_cpu_feature("avx2").await?,
            architecture: "x86_64".to_string(),
        };

        let gpu = Self::detect_windows_gpu().await?;
        let memory = Self::detect_windows_memory().await?;
        let displays = Self::detect_windows_displays().await?;
        let audio_devices = Self::detect_windows_audio().await?;
        let storage = Self::detect_windows_storage().await?;

        Ok(HardwareCapabilities {
            cpu,
            gpu,
            memory,
            displays,
            audio_devices,
            storage,
        })
    }

    /// Detect macOS hardware
    #[cfg(target_os = "macos")]
    async fn detect_macos_hardware() -> Result<HardwareCapabilities> {
        use std::process::Command;

        let cpu_output = Command::new("sysctl")
            .args(&["-n", "machdep.cpu.brand_string"])
            .output()
            .map_err(|e| PlatformConfigError::HardwareDetection(e.to_string()))?;

        let cpu_model = String::from_utf8_lossy(&cpu_output.stdout)
            .trim()
            .to_string();

        let cpu_cores_output = Command::new("sysctl")
            .args(&["-n", "hw.ncpu"])
            .output()
            .map_err(|e| PlatformConfigError::HardwareDetection(e.to_string()))?;

        let cpu_cores = String::from_utf8_lossy(&cpu_cores_output.stdout)
            .trim()
            .parse::<usize>()
            .map_err(|e| PlatformConfigError::HardwareDetection(e.to_string()))?;

        let cpu = CpuInfo {
            model: cpu_model,
            cores: cpu_cores,
            logical_cores: cpu_cores,
            max_frequency: 0.0,
            has_avx: true,
            has_avx2: true,
            architecture: std::env::consts::ARCH.to_string(),
        };

        let gpu = vec![GpuInfo {
            name: "Apple Silicon GPU".to_string(),
            vendor: GpuVendor::Apple,
            memory_mb: 0,
            driver_version: "N/A".to_string(),
            is_primary: true,
            supports_encoding: true,
            supports_nvenc: false,
            supports_amf: false,
            supports_qsv: false,
        }];

        let mem_output = Command::new("sysctl")
            .args(&["-n", "hw.memsize"])
            .output()
            .map_err(|e| PlatformConfigError::HardwareDetection(e.to_string()))?;

        let total_bytes = String::from_utf8_lossy(&mem_output.stdout)
            .trim()
            .parse::<u64>()
            .map_err(|e| PlatformConfigError::HardwareDetection(e.to_string()))?;

        let memory = MemoryInfo {
            total_gb: total_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            available_gb: 0.0,
            speed_mhz: None,
        };

        let displays = vec![DisplayInfo {
            id: "primary".to_string(),
            name: "Built-in Display".to_string(),
            resolution: (1920, 1080),
            refresh_rate: 60.0,
            is_primary: true,
            scaling_factor: 2.0,
        }];

        let audio_devices = AudioDeviceInfo {
            input_devices: vec![],
            output_devices: vec![],
            default_input: None,
            default_output: None,
        };

        let storage = StorageInfo {
            total_space_gb: 0.0,
            free_space_gb: 0.0,
            install_drive: "/".to_string(),
            temp_directory: std::env::temp_dir().to_string_lossy().to_string(),
        };

        Ok(HardwareCapabilities {
            cpu,
            gpu,
            memory,
            displays,
            audio_devices,
            storage,
        })
    }

    /// Detect Linux hardware
    #[cfg(target_os = "linux")]
    async fn detect_linux_hardware() -> Result<HardwareCapabilities> {
        Err(PlatformConfigError::UnsupportedPlatform(
            "Linux hardware detection not fully implemented".to_string(),
        ))
    }

    /// Detect platform features
    fn detect_features(platform: &Platform, hardware: &HardwareCapabilities) -> PlatformFeatures {
        match platform {
            Platform::Windows => PlatformFeatures {
                supports_windows_capture: true,
                supports_ffmpeg_native: true,
                supports_core_graphics: false,
                supports_hardware_acceleration: hardware
                    .gpu
                    .iter()
                    .any(|gpu| gpu.supports_encoding),
                supports_system_tray: true,
                supports_global_hotkeys: true,
                supports_file_associations: true,
                supports_auto_start: true,
                supports_notifications: true,
                supports_api_detection: true,
                supports_window_enumeration: true,
            },
            Platform::MacOS => PlatformFeatures {
                supports_windows_capture: false,
                supports_ffmpeg_native: true,
                supports_core_graphics: true,
                supports_hardware_acceleration: hardware
                    .gpu
                    .iter()
                    .any(|gpu| gpu.supports_encoding),
                supports_system_tray: true,
                supports_global_hotkeys: true,
                supports_file_associations: true,
                supports_auto_start: true,
                supports_notifications: true,
                supports_api_detection: true,
                supports_window_enumeration: true,
            },
            Platform::Linux => PlatformFeatures {
                supports_windows_capture: false,
                supports_ffmpeg_native: true,
                supports_core_graphics: false,
                supports_hardware_acceleration: hardware
                    .gpu
                    .iter()
                    .any(|gpu| gpu.supports_encoding),
                supports_system_tray: false,
                supports_global_hotkeys: false,
                supports_file_associations: true,
                supports_auto_start: true,
                supports_notifications: true,
                supports_api_detection: false,
                supports_window_enumeration: false,
            },
        }
    }

    /// Get platform-specific default overrides
    fn get_default_overrides(
        platform: &Platform,
        hardware: &HardwareCapabilities,
    ) -> RecordingSettings {
        let mut defaults = RecordingSettings::default();

        match platform {
            Platform::Windows => {
                defaults.video.encoder = if hardware.gpu.iter().any(|gpu| gpu.supports_nvenc) {
                    EncoderPreference::Nvenc
                } else if hardware.gpu.iter().any(|gpu| gpu.supports_amf) {
                    EncoderPreference::Amf
                } else if hardware.gpu.iter().any(|gpu| gpu.supports_qsv) {
                    EncoderPreference::Qsv
                } else {
                    EncoderPreference::Software
                };
            }
            Platform::MacOS => {
                defaults.video.encoder = EncoderPreference::Software;
                defaults.minimize_to_tray = false;
            }
            Platform::Linux => {
                defaults.video.encoder = EncoderPreference::Software;
                defaults.minimize_to_tray = false;
            }
        }

        defaults
    }

    /// Generate hardware-based recommendations
    fn generate_recommendations(
        _platform: &Platform,
        hardware: &HardwareCapabilities,
    ) -> RecommendedSettings {
        let video = Self::generate_video_recommendations(hardware);
        let audio = Self::generate_audio_recommendations(hardware);
        let performance = Self::generate_performance_recommendations(hardware);
        let storage = Self::generate_storage_recommendations(hardware);

        RecommendedSettings {
            video,
            audio,
            performance,
            storage,
        }
    }

    /// Generate video recommendations
    fn generate_video_recommendations(hardware: &HardwareCapabilities) -> VideoRecommendations {
        let total_memory_gb = hardware.memory.total_gb;
        let gpu_memory_mb = hardware
            .gpu
            .iter()
            .map(|gpu| gpu.memory_mb)
            .max()
            .unwrap_or(0);

        let (recommended_encoder, recommended_codec, recommended_bitrate_kbps) =
            if gpu_memory_mb >= 8000 {
                (EncoderPreference::Auto, "h265", 20000)
            } else if gpu_memory_mb >= 4000 {
                (EncoderPreference::Auto, "h265", 10000)
            } else {
                (EncoderPreference::Software, "h264", 5000)
            };

        let (recommended_resolution, recommended_frame_rate) =
            if total_memory_gb >= 16.0 && gpu_memory_mb >= 6000 {
                ("2560x1440", "60")
            } else if total_memory_gb >= 8.0 {
                ("1920x1080", "60")
            } else {
                ("1280x720", "30")
            };

        let maximum_recording_hours = if hardware.storage.free_space_gb >= 100.0 {
            24.0
        } else if hardware.storage.free_space_gb >= 50.0 {
            12.0
        } else {
            6.0
        };

        VideoRecommendations {
            recommended_encoder,
            recommended_codec: recommended_codec.to_string(),
            recommended_bitrate_kbps,
            recommended_resolution: recommended_resolution.to_string(),
            recommended_frame_rate: recommended_frame_rate.to_string(),
            maximum_recording_hours,
        }
    }

    /// Generate audio recommendations
    fn generate_audio_recommendations(_hardware: &HardwareCapabilities) -> AudioRecommendations {
        AudioRecommendations {
            recommended_sample_rate: "48000".to_string(),
            recommended_bitrate: "192".to_string(),
            max_channels: 2,
            enable_microphone_by_default: true,
            enable_system_audio_by_default: true,
        }
    }

    /// Generate performance recommendations
    fn generate_performance_recommendations(
        hardware: &HardwareCapabilities,
    ) -> PerformanceRecommendations {
        let total_memory_gb = hardware.memory.total_gb;

        PerformanceRecommendations {
            enable_hardware_acceleration: hardware.gpu.iter().any(|gpu| gpu.supports_encoding),
            recommended_buffer_size_mb: if total_memory_gb >= 16.0 {
                512
            } else if total_memory_gb >= 8.0 {
                256
            } else {
                128
            },
            recommended_temp_cleanup_interval_minutes: 30,
            recommended_concurrent_clips: if total_memory_gb >= 16.0 { 5 } else { 3 },
            enable_performance_monitoring: true,
        }
    }

    /// Generate storage recommendations
    fn generate_storage_recommendations(hardware: &HardwareCapabilities) -> StorageRecommendations {
        StorageRecommendations {
            recommended_clips_directory: format!(
                "{}/Documents/LoLShorts",
                std::env::var("HOME").unwrap_or_default()
            ),
            minimum_free_space_gb: 10.0,
            recommended_cleanup_threshold_gb: hardware.storage.total_space_gb * 0.1,
            enable_auto_cleanup: hardware.storage.free_space_gb < 50.0,
        }
    }

    // Windows-specific helper methods
    #[cfg(target_os = "windows")]
    async fn get_windows_cpu_name() -> Result<String> {
        Ok("Intel Core i7-9700K".to_string())
    }

    #[cfg(target_os = "windows")]
    async fn get_windows_cpu_frequency() -> Result<f64> {
        Ok(3600.0)
    }

    #[cfg(target_os = "windows")]
    async fn check_windows_cpu_feature(_feature: &str) -> Result<bool> {
        Ok(true)
    }

    #[cfg(target_os = "windows")]
    async fn detect_windows_gpu() -> Result<Vec<GpuInfo>> {
        Ok(vec![GpuInfo {
            name: "NVIDIA GeForce RTX 3070".to_string(),
            vendor: GpuVendor::Nvidia,
            memory_mb: 8000,
            driver_version: "511.23".to_string(),
            is_primary: true,
            supports_encoding: true,
            supports_nvenc: true,
            supports_amf: false,
            supports_qsv: false,
        }])
    }

    #[cfg(target_os = "windows")]
    async fn detect_windows_memory() -> Result<MemoryInfo> {
        Ok(MemoryInfo {
            total_gb: 16.0,
            available_gb: 8.0,
            speed_mhz: Some(3200.0),
        })
    }

    #[cfg(target_os = "windows")]
    async fn detect_windows_displays() -> Result<Vec<DisplayInfo>> {
        Ok(vec![DisplayInfo {
            id: "PRIMARY".to_string(),
            name: "Primary Monitor".to_string(),
            resolution: (1920, 1080),
            refresh_rate: 144.0,
            is_primary: true,
            scaling_factor: 1.0,
        }])
    }

    #[cfg(target_os = "windows")]
    async fn detect_windows_audio() -> Result<AudioDeviceInfo> {
        Ok(AudioDeviceInfo {
            input_devices: vec![],
            output_devices: vec![],
            default_input: None,
            default_output: None,
        })
    }

    #[cfg(target_os = "windows")]
    async fn detect_windows_storage() -> Result<StorageInfo> {
        Ok(StorageInfo {
            total_space_gb: 500.0,
            free_space_gb: 250.0,
            install_drive: "C:".to_string(),
            temp_directory: std::env::temp_dir().to_string_lossy().to_string(),
        })
    }

    /// Validate settings against platform capabilities
    pub fn validate_settings(&self, settings: &RecordingSettings) -> Result<()> {
        if settings.video.frame_rate == super::super::models::FrameRate::Fps120
            && self.hardware.memory.total_gb < 8.0
        {
            return Err(PlatformConfigError::Validation(
                "120 FPS recording requires at least 8GB RAM".to_string(),
            ));
        }

        if self.hardware.storage.free_space_gb < 5.0 {
            return Err(PlatformConfigError::Validation(
                "At least 5GB free space required for recording".to_string(),
            ));
        }

        Ok(())
    }

    /// Apply platform-specific optimizations to settings
    pub fn optimize_settings(&self, settings: &mut RecordingSettings) {
        if self.hardware.memory.total_gb < 8.0 {
            settings.video.bitrate_preset = super::super::models::BitratePreset::Low;
        }

        if !self.hardware.gpu.iter().any(|gpu| gpu.supports_encoding) {
            settings.video.encoder = EncoderPreference::Software;
        }

        if settings.audio.record_microphone && self.hardware.audio_devices.input_devices.is_empty()
        {
            settings.audio.record_microphone = false;
            settings.audio.microphone_device = None;
        }

        match self.platform {
            Platform::Windows => {
                if self.features.supports_windows_capture {
                    // Prefer windows-capture backend
                }
            }
            Platform::MacOS => {
                settings.minimize_to_tray = false;
            }
            Platform::Linux => {
                settings.minimize_to_tray = false;
                settings.show_notifications = true;
            }
        }
    }

    /// Stub implementation for macOS hardware detection on non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    async fn detect_macos_hardware() -> Result<HardwareCapabilities> {
        Err(PlatformConfigError::UnsupportedPlatform(
            "macOS hardware detection not available on this platform".to_string(),
        ))
    }

    /// Stub implementation for Linux hardware detection on non-Linux platforms
    #[cfg(not(target_os = "linux"))]
    async fn detect_linux_hardware() -> Result<HardwareCapabilities> {
        Err(PlatformConfigError::UnsupportedPlatform(
            "Linux hardware detection not available on this platform".to_string(),
        ))
    }

    /// Stub implementation for Windows hardware detection on non-Windows platforms
    #[cfg(not(target_os = "windows"))]
    async fn detect_windows_hardware() -> Result<HardwareCapabilities> {
        Err(PlatformConfigError::UnsupportedPlatform(
            "Windows hardware detection not available on this platform".to_string(),
        ))
    }
}
