#![allow(dead_code)]
use super::super::models::{EncoderPreference, RecordingSettings};
#[cfg(target_os = "windows")]
use super::types::AudioDevice;
use super::types::{
    AudioDeviceInfo, AudioRecommendations, CpuInfo, DisplayInfo, GpuInfo, GpuVendor,
    HardwareCapabilities, MemoryInfo, PerformanceRecommendations, Platform, PlatformConfig,
    PlatformConfigError, PlatformFeatures, RecommendedSettings, Result, StorageInfo,
    StorageRecommendations, VideoRecommendations,
};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const PLATFORM_CONFIG_CACHE_TTL: Duration = Duration::from_secs(60);
const WINDOWS_HARDWARE_DETECTION_TIMEOUT: Duration = Duration::from_secs(20);

type PlatformConfigCache = tokio::sync::Mutex<Option<(Instant, PlatformConfig)>>;

fn platform_config_cache() -> &'static PlatformConfigCache {
    static CACHE: OnceLock<PlatformConfigCache> = OnceLock::new();
    CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

fn bytes_to_gib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

fn gpu_vendor(name: &str, compatibility: &str) -> GpuVendor {
    let identity = format!("{name} {compatibility}").to_ascii_lowercase();
    if identity.contains("nvidia") {
        GpuVendor::Nvidia
    } else if identity.contains("amd") || identity.contains("advanced micro devices") {
        GpuVendor::Amd
    } else if identity.contains("intel") {
        GpuVendor::Intel
    } else if identity.contains("apple") {
        GpuVendor::Apple
    } else {
        GpuVendor::Unknown
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, serde::Deserialize)]
struct WindowsVideoController {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "AdapterCompatibility")]
    adapter_compatibility: Option<String>,
    #[serde(rename = "AdapterRAM")]
    adapter_ram: Option<u64>,
    #[serde(rename = "DriverVersion")]
    driver_version: Option<String>,
    #[serde(rename = "CurrentHorizontalResolution")]
    current_horizontal_resolution: Option<u32>,
    #[serde(rename = "CurrentVerticalResolution")]
    current_vertical_resolution: Option<u32>,
    #[serde(rename = "CurrentRefreshRate")]
    current_refresh_rate: Option<u32>,
}

impl PlatformConfig {
    #[cfg(test)]
    pub(crate) fn test_fixture(default_overrides: RecordingSettings) -> Self {
        let platform = Platform::Windows;
        let hardware = HardwareCapabilities {
            cpu: CpuInfo {
                model: "Test CPU".to_string(),
                cores: 8,
                logical_cores: 16,
                max_frequency: 4_000.0,
                has_avx: true,
                has_avx2: true,
                architecture: "x86_64".to_string(),
            },
            gpu: vec![GpuInfo {
                name: "Test NVIDIA GPU".to_string(),
                vendor: GpuVendor::Nvidia,
                memory_mb: 8_192,
                driver_version: "test".to_string(),
                is_primary: true,
                supports_encoding: true,
                supports_nvenc: true,
                supports_amf: false,
                supports_qsv: false,
            }],
            memory: MemoryInfo {
                total_gb: 16.0,
                available_gb: 8.0,
                speed_mhz: None,
            },
            displays: Vec::new(),
            audio_devices: AudioDeviceInfo {
                input_devices: Vec::new(),
                output_devices: Vec::new(),
                default_input: None,
                default_output: None,
            },
            storage: StorageInfo {
                total_space_gb: 500.0,
                free_space_gb: 100.0,
                install_drive: "C:\\".to_string(),
                temp_directory: "C:\\Temp".to_string(),
            },
        };
        let features = Self::detect_features(&platform, &hardware);
        let recommended_settings = Self::generate_recommendations(&platform, &hardware);
        Self {
            platform,
            hardware,
            default_overrides,
            features,
            recommended_settings,
        }
    }

    /// Detect current platform and hardware capabilities
    pub async fn detect() -> Result<Self> {
        // Settings, onboarding and autostart reconciliation can request this
        // information at nearly the same time during startup. WMI, CPAL and
        // FFmpeg probing are comparatively expensive, so keep one short-lived
        // process-wide snapshot and serialize refreshes.
        let mut cache = platform_config_cache().lock().await;
        if let Some((measured_at, config)) = cache.as_ref() {
            if measured_at.elapsed() < PLATFORM_CONFIG_CACHE_TTL {
                return Ok(config.clone());
            }
        }

        let config = Self::detect_uncached().await?;
        *cache = Some((Instant::now(), config.clone()));
        Ok(config)
    }

    async fn detect_uncached() -> Result<Self> {
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
        tokio::time::timeout(
            WINDOWS_HARDWARE_DETECTION_TIMEOUT,
            tokio::task::spawn_blocking(Self::detect_windows_hardware_blocking),
        )
        .await
        .map_err(|_| {
            PlatformConfigError::HardwareDetection(format!(
                "Windows hardware detection timed out after {} seconds",
                WINDOWS_HARDWARE_DETECTION_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|error| {
            PlatformConfigError::HardwareDetection(format!(
                "Windows hardware detection task failed: {error}"
            ))
        })?
    }

    #[cfg(target_os = "windows")]
    fn detect_windows_hardware_blocking() -> Result<HardwareCapabilities> {
        use sysinfo::System;

        let mut system = System::new_all();
        system.refresh_cpu_all();
        system.refresh_memory();

        let logical_cores = system.cpus().len().max(1);
        let cores = system.physical_core_count().unwrap_or(logical_cores).max(1);
        let model = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim())
            .filter(|brand| !brand.is_empty())
            .unwrap_or("Unknown CPU")
            .to_string();
        let max_frequency = system
            .cpus()
            .iter()
            .map(|cpu| cpu.frequency())
            .max()
            .unwrap_or(0) as f64;
        let (has_avx, has_avx2) = Self::windows_cpu_features();
        let cpu = CpuInfo {
            model,
            cores,
            logical_cores,
            max_frequency,
            has_avx,
            has_avx2,
            architecture: std::env::consts::ARCH.to_string(),
        };

        let controllers = match Self::query_windows_video_controllers() {
            Ok(controllers) => controllers,
            Err(error) => {
                tracing::warn!(%error, "WMI video-controller query failed");
                Vec::new()
            }
        };
        let gpu = Self::windows_gpu_info(&controllers);
        let displays = Self::windows_display_info(&controllers);
        let audio_devices = Self::windows_audio_info();
        let storage = Self::windows_storage_info();
        let memory = MemoryInfo {
            total_gb: bytes_to_gib(system.total_memory()),
            available_gb: bytes_to_gib(system.available_memory()),
            speed_mhz: None,
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

        let (recommended_encoder, recommended_bitrate_kbps) = if gpu_memory_mb >= 8000 {
            (EncoderPreference::Auto, 20000)
        } else if gpu_memory_mb >= 4000 {
            (EncoderPreference::Auto, 10000)
        } else {
            (EncoderPreference::Software, 5000)
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
            // H.264 is the public compatibility contract. H.265 previews can
            // be black in WebView2 when the optional Windows HEVC extension is
            // absent, so hardware capability alone must not recommend it.
            recommended_codec: "h264".to_string(),
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
            // Microphone capture is opt-in. This matches RecordingSettings and
            // avoids turning on an additional privacy-sensitive audio source.
            enable_microphone_by_default: false,
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
        let recommended_root = dirs::video_dir()
            .or_else(dirs::document_dir)
            .or_else(dirs::data_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join("LoLShorts");
        StorageRecommendations {
            recommended_clips_directory: recommended_root.to_string_lossy().to_string(),
            minimum_free_space_gb: 10.0,
            recommended_cleanup_threshold_gb: hardware.storage.total_space_gb * 0.1,
            // User media is never deleted implicitly on a fresh profile. Low
            // space is surfaced through readiness and storage diagnostics so
            // the user can make an explicit retention choice.
            enable_auto_cleanup: false,
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_cpu_features() -> (bool, bool) {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            (
                std::is_x86_feature_detected!("avx"),
                std::is_x86_feature_detected!("avx2"),
            )
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            (false, false)
        }
    }

    #[cfg(target_os = "windows")]
    fn query_windows_video_controllers() -> std::result::Result<Vec<WindowsVideoController>, String>
    {
        use wmi::{COMLibrary, WMIConnection};

        let com = COMLibrary::new().map_err(|error| error.to_string())?;
        let connection = WMIConnection::new(com).map_err(|error| error.to_string())?;
        connection
            .raw_query(
                "SELECT Name, AdapterCompatibility, AdapterRAM, DriverVersion, \
                 CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate \
                 FROM Win32_VideoController",
            )
            .map_err(|error| error.to_string())
    }

    #[cfg(target_os = "windows")]
    fn hardware_encoder_support() -> (bool, bool, bool) {
        let Ok(ffmpeg) = crate::utils::ffmpeg::get_ffmpeg_path() else {
            return (false, false, false);
        };
        let mut command = std::process::Command::new(ffmpeg);
        command.args(["-hide_banner", "-encoders"]);
        let Ok(output) = crate::utils::process::command_output_with_timeout(
            command,
            Duration::from_secs(8),
            "platform encoder detection",
        ) else {
            return (false, false, false);
        };
        let encoders = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        (
            encoders.contains("h264_nvenc"),
            encoders.contains("h264_amf"),
            encoders.contains("h264_qsv"),
        )
    }

    #[cfg(target_os = "windows")]
    fn query_nvidia_gpus() -> Vec<GpuInfo> {
        let mut command = std::process::Command::new("nvidia-smi");
        command.args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ]);
        let Ok(output) = crate::utils::process::command_output_with_timeout(
            command,
            Duration::from_secs(5),
            "NVIDIA adapter detection",
        ) else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let mut values = line.split(',').map(str::trim);
                let name = values.next()?.to_string();
                let memory_mb = values.next()?.parse::<u64>().ok()?;
                let driver_version = values.next().unwrap_or("unknown").to_string();
                Some(GpuInfo {
                    name,
                    vendor: GpuVendor::Nvidia,
                    memory_mb,
                    driver_version,
                    is_primary: false,
                    supports_encoding: false,
                    supports_nvenc: false,
                    supports_amf: false,
                    supports_qsv: false,
                })
            })
            .collect()
    }

    #[cfg(target_os = "windows")]
    fn windows_gpu_info(controllers: &[WindowsVideoController]) -> Vec<GpuInfo> {
        let (nvenc, amf, qsv) = Self::hardware_encoder_support();
        let mut gpus = Self::query_nvidia_gpus();

        for controller in controllers {
            let name = controller
                .name
                .as_deref()
                .unwrap_or("Unknown graphics adapter")
                .trim()
                .to_string();
            if gpus.iter().any(|gpu| gpu.name.eq_ignore_ascii_case(&name)) {
                continue;
            }
            let vendor = gpu_vendor(
                &name,
                controller
                    .adapter_compatibility
                    .as_deref()
                    .unwrap_or_default(),
            );
            let supports_nvenc = vendor == GpuVendor::Nvidia && nvenc;
            let supports_amf = vendor == GpuVendor::Amd && amf;
            let supports_qsv = vendor == GpuVendor::Intel && qsv;
            gpus.push(GpuInfo {
                name,
                vendor,
                memory_mb: controller.adapter_ram.unwrap_or(0) / (1024 * 1024),
                driver_version: controller
                    .driver_version
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                is_primary: false,
                supports_encoding: supports_nvenc || supports_amf || supports_qsv,
                supports_nvenc,
                supports_amf,
                supports_qsv,
            });
        }

        // nvidia-smi provides more reliable VRAM than Win32_VideoController,
        // while the FFmpeg encoder probe establishes whether this bundled
        // runtime can actually address the hardware encoder.
        for gpu in &mut gpus {
            match gpu.vendor {
                GpuVendor::Nvidia => {
                    gpu.supports_nvenc = nvenc;
                    gpu.supports_encoding = nvenc;
                }
                GpuVendor::Amd => {
                    gpu.supports_amf = amf;
                    gpu.supports_encoding = amf;
                }
                GpuVendor::Intel => {
                    gpu.supports_qsv = qsv;
                    gpu.supports_encoding = qsv;
                }
                _ => {}
            }
        }

        if let Some(primary) = gpus
            .iter()
            .position(|gpu| gpu.supports_nvenc)
            .or_else(|| gpus.iter().position(|gpu| gpu.supports_encoding))
            .or_else(|| gpus.iter().position(|gpu| gpu.vendor != GpuVendor::Unknown))
            .or_else(|| (!gpus.is_empty()).then_some(0))
        {
            gpus[primary].is_primary = true;
        }
        gpus
    }

    #[cfg(target_os = "windows")]
    fn windows_display_info(controllers: &[WindowsVideoController]) -> Vec<DisplayInfo> {
        let mut displays = Vec::new();
        for controller in controllers {
            let (Some(width), Some(height)) = (
                controller.current_horizontal_resolution,
                controller.current_vertical_resolution,
            ) else {
                continue;
            };
            if width == 0
                || height == 0
                || displays
                    .iter()
                    .any(|display: &DisplayInfo| display.resolution == (width, height))
            {
                continue;
            }
            displays.push(DisplayInfo {
                id: format!("DISPLAY{}", displays.len()),
                name: controller
                    .name
                    .clone()
                    .unwrap_or_else(|| "Windows display".to_string()),
                resolution: (width, height),
                refresh_rate: controller.current_refresh_rate.unwrap_or(60) as f64,
                is_primary: displays.is_empty(),
                scaling_factor: 1.0,
            });
        }

        if displays.is_empty() {
            use windows::Win32::UI::WindowsAndMessaging::{
                GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
            };
            let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(0) as u32;
            let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(0) as u32;
            if width > 0 && height > 0 {
                displays.push(DisplayInfo {
                    id: "DISPLAY0".to_string(),
                    name: "Primary Windows display".to_string(),
                    resolution: (width, height),
                    refresh_rate: 60.0,
                    is_primary: true,
                    scaling_factor: 1.0,
                });
            }
        }
        displays
    }

    #[cfg(target_os = "windows")]
    fn windows_audio_info() -> AudioDeviceInfo {
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = cpal::default_host();
        let default_input = host
            .default_input_device()
            .and_then(|device| device.name().ok());
        let default_output = host
            .default_output_device()
            .and_then(|device| device.name().ok());
        let input_devices = host
            .input_devices()
            .map(|devices| {
                devices
                    .enumerate()
                    .filter_map(|(index, device)| {
                        let name = device.name().ok()?;
                        let config = device.default_input_config().ok();
                        Some(AudioDevice {
                            id: format!("input-{index}"),
                            channels: config
                                .as_ref()
                                .map(|value| value.channels() as u32)
                                .unwrap_or(0),
                            sample_rate: config
                                .as_ref()
                                .map(|value| value.sample_rate().0)
                                .unwrap_or(0),
                            is_default: default_input.as_deref() == Some(name.as_str()),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let output_devices = host
            .output_devices()
            .map(|devices| {
                devices
                    .enumerate()
                    .filter_map(|(index, device)| {
                        let name = device.name().ok()?;
                        let config = device.default_output_config().ok();
                        Some(AudioDevice {
                            id: format!("output-{index}"),
                            channels: config
                                .as_ref()
                                .map(|value| value.channels() as u32)
                                .unwrap_or(0),
                            sample_rate: config
                                .as_ref()
                                .map(|value| value.sample_rate().0)
                                .unwrap_or(0),
                            is_default: default_output.as_deref() == Some(name.as_str()),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        AudioDeviceInfo {
            input_devices,
            output_devices,
            default_input,
            default_output,
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_storage_info() -> StorageInfo {
        let target = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        let snapshot = crate::utils::disk::query_disk_space(&target).ok();

        StorageInfo {
            total_space_gb: snapshot
                .map(|value| bytes_to_gib(value.total_bytes))
                .unwrap_or(0.0),
            free_space_gb: snapshot
                .map(|value| bytes_to_gib(value.available_bytes))
                .unwrap_or(0.0),
            install_drive: target.display().to_string(),
            temp_directory: std::env::temp_dir().to_string_lossy().to_string(),
        }
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

        if self.hardware.storage.total_space_gb > 0.0 && self.hardware.storage.free_space_gb < 5.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hardware() -> HardwareCapabilities {
        HardwareCapabilities {
            cpu: CpuInfo {
                model: "Test CPU".to_string(),
                cores: 8,
                logical_cores: 16,
                max_frequency: 4_000.0,
                has_avx: true,
                has_avx2: true,
                architecture: "x86_64".to_string(),
            },
            gpu: vec![GpuInfo {
                name: "NVIDIA Test GPU".to_string(),
                vendor: GpuVendor::Nvidia,
                memory_mb: 8_192,
                driver_version: "test".to_string(),
                is_primary: true,
                supports_encoding: true,
                supports_nvenc: true,
                supports_amf: false,
                supports_qsv: false,
            }],
            memory: MemoryInfo {
                total_gb: 16.0,
                available_gb: 8.0,
                speed_mhz: None,
            },
            displays: Vec::new(),
            audio_devices: AudioDeviceInfo {
                input_devices: Vec::new(),
                output_devices: Vec::new(),
                default_input: None,
                default_output: None,
            },
            storage: StorageInfo {
                total_space_gb: 500.0,
                free_space_gb: 100.0,
                install_drive: "C:\\".to_string(),
                temp_directory: "C:\\Temp".to_string(),
            },
        }
    }

    #[test]
    fn gpu_vendor_classification_uses_name_and_compatibility() {
        assert_eq!(gpu_vendor("GeForce RTX", "NVIDIA"), GpuVendor::Nvidia);
        assert_eq!(gpu_vendor("Radeon RX", "AMD"), GpuVendor::Amd);
        assert_eq!(
            gpu_vendor("Arc A770", "Intel Corporation"),
            GpuVendor::Intel
        );
        assert_eq!(gpu_vendor("Virtual Display", "Unknown"), GpuVendor::Unknown);
    }

    #[test]
    fn recommendations_preserve_public_compatibility_and_microphone_privacy() {
        let recommendations =
            PlatformConfig::generate_recommendations(&Platform::Windows, &test_hardware());

        assert_eq!(recommendations.video.recommended_codec, "h264");
        assert!(!recommendations.audio.enable_microphone_by_default);
        assert!(recommendations.audio.enable_system_audio_by_default);
        assert!(!recommendations.storage.enable_auto_cleanup);
        assert!(!recommendations
            .storage
            .recommended_clips_directory
            .is_empty());
    }

    #[test]
    fn byte_conversion_uses_binary_gibibytes() {
        assert_eq!(bytes_to_gib(1024 * 1024 * 1024), 1.0);
    }
}
