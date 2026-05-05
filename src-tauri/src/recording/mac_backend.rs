//! macOS recording backend implementation
//!
//! Uses AVFoundation and ScreenCaptureKit for screen capture on macOS
//! Provides cross-platform compatibility with the Windows backend

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock as TokioRwLock};
use tokio::time::Instant;

use super::lol_window_detector::{LoLWindow, LoLWindowDetector};
use super::mac_audio_core::{CoreAudioDevice, CoreAudioDeviceType, CoreAudioManager};
use super::mac_ffmpeg::{
    detect_available_macos_encoders, FFmpegUseCase, MacFFmpegCommandBuilder, MacFFmpegConfig,
};
use super::mac_screen_capture::{MacDisplayInfo, MacScreenCaptureConfig, MacScreenCaptureManager};
use crate::storage::GameMetadata;
use crate::utils::ffmpeg::get_ffmpeg_path;

/// macOS screen capture configuration
#[derive(Debug, Clone)]
pub struct MacRecordingConfig {
    pub fps: u32,
    pub bitrate: u32,
    pub resolution: (u32, u32),
    pub display_id: u32,
    pub output_dir: PathBuf,
    pub audio_enabled: bool,
    pub use_hardware_encoding: bool,
}

impl Default for MacRecordingConfig {
    fn default() -> Self {
        Self {
            fps: 60,
            bitrate: 15_000_000, // 15 Mbps
            resolution: (1920, 1080),
            display_id: 0, // Main display
            output_dir: PathBuf::from("./recordings"),
            audio_enabled: false,
            use_hardware_encoding: true,
        }
    }
}

/// Recording status for macOS backend
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacRecordingStatus {
    Idle,
    Preparing,
    Capturing,
    Encoding,
    Error,
}

impl Default for MacRecordingStatus {
    fn default() -> Self {
        MacRecordingStatus::Idle
    }
}

/// macOS recording manager using AVFoundation
pub struct MacRecordingManager {
    config: MacRecordingConfig,
    status: Arc<TokioRwLock<MacRecordingStatus>>,
    current_game: Arc<TokioRwLock<Option<GameMetadata>>>,
    start_time: Arc<TokioRwLock<Option<Instant>>>,
    total_frames: Arc<TokioRwLock<u64>>,
    capture_pid: Arc<Mutex<Option<u32>>>,
    screen_capture: Arc<Mutex<MacScreenCaptureManager>>,
    audio_manager: Arc<Mutex<CoreAudioManager>>,
    selected_input_device: Arc<Mutex<Option<CoreAudioDevice>>>,
    selected_output_device: Arc<Mutex<Option<CoreAudioDevice>>>,
    lol_window_detector: Arc<LoLWindowDetector>,
    current_lol_window: Arc<TokioRwLock<Option<LoLWindow>>>,
}

impl MacRecordingManager {
    pub async fn new(config: MacRecordingConfig) -> Result<Self> {
        // Ensure output directory exists
        std::fs::create_dir_all(&config.output_dir)?;

        // Initialize screen capture manager
        let capture_config = MacScreenCaptureConfig {
            display_id: config.display_id,
            width: config.resolution.0,
            height: config.resolution.1,
            fps: config.fps as f64,
            pixel_format: super::mac_screen_capture::MacPixelFormat::BGRA,
            capture_audio: config.audio_enabled,
            show_cursor: true,
        };
        let screen_capture = MacScreenCaptureManager::new(capture_config);

        // Initialize audio manager
        let mut audio_manager = CoreAudioManager::new();
        audio_manager.refresh_devices().await?;

        // Initialize LoL window detector
        let lol_window_detector = Arc::new(LoLWindowDetector::new());

        Ok(Self {
            config,
            status: Arc::new(TokioRwLock::new(MacRecordingStatus::Idle)),
            current_game: Arc::new(TokioRwLock::new(None)),
            start_time: Arc::new(TokioRwLock::new(None)),
            total_frames: Arc::new(TokioRwLock::new(0)),
            capture_pid: Arc::new(Mutex::new(None)),
            screen_capture: Arc::new(Mutex::new(screen_capture)),
            audio_manager: Arc::new(Mutex::new(audio_manager)),
            selected_input_device: Arc::new(Mutex::new(None)),
            selected_output_device: Arc::new(Mutex::new(None)),
            lol_window_detector,
            current_lol_window: Arc::new(TokioRwLock::new(None)),
        })
    }

    /// Get current recording status
    pub async fn get_status(&self) -> MacRecordingStatus {
        *self.status.read().await
    }

    /// Get available displays
    pub async fn get_available_displays(&self) -> Result<Vec<MacDisplayInfo>> {
        MacScreenCaptureManager::get_available_displays()
    }

    /// Get available audio devices
    pub async fn get_available_audio_devices(&self) -> Result<Vec<CoreAudioDevice>> {
        let mut manager = self.audio_manager.lock().await;
        manager.refresh_devices().await?;
        Ok(manager.get_devices().to_vec())
    }

    /// Select input audio device
    pub async fn select_input_device(&self, device: CoreAudioDevice) -> Result<()> {
        let mut selected = self.selected_input_device.lock().await;
        *selected = Some(device);
        Ok(())
    }

    /// Select output audio device
    pub async fn select_output_device(&self, device: CoreAudioDevice) -> Result<()> {
        let mut selected = self.selected_output_device.lock().await;
        *selected = Some(device);
        Ok(())
    }

    /// Detect available encoders
    pub async fn detect_available_encoders(
        &self,
    ) -> Result<Vec<super::mac_ffmpeg::MacFFmpegEncoder>> {
        detect_available_macos_encoders()
    }

    /// Start macOS screen capture
    pub async fn start_recording(&self) -> Result<PathBuf> {
        let mut status = self.status.write().await;

        if *status != MacRecordingStatus::Idle {
            anyhow::bail!("Recording already in progress");
        }

        *status = MacRecordingStatus::Preparing;
        drop(status);

        // Create output file with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("recording_{}.mp4", timestamp);
        let output_path = self.config.output_dir.join(filename);

        // Start screen capture
        {
            let mut capture = self.screen_capture.lock().await;
            capture
                .start_capture()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start screen capture: {}", e))?;
        }

        // Start FFmpeg with macOS-specific configuration
        let capture_pid = self.start_ffmpeg_capture(&output_path).await?;

        {
            let mut pid_guard = self.capture_pid.lock().await;
            *pid_guard = Some(capture_pid);
        }

        *self.status.write().await = MacRecordingStatus::Capturing;
        *self.start_time.write().await = Some(Instant::now());
        *self.total_frames.write().await = 0;

        tracing::info!("macOS recording started successfully");
        Ok(output_path)
    }

    /// Stop macOS screen capture
    pub async fn stop_recording(&self) -> Result<PathBuf> {
        let mut status = self.status.write().await;

        if *status == MacRecordingStatus::Idle {
            anyhow::bail!("No recording in progress");
        }

        *status = MacRecordingStatus::Encoding;
        drop(status);

        // Stop screen capture
        {
            let mut capture = self.screen_capture.lock().await;
            capture
                .stop_capture()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to stop screen capture: {}", e))?;
        }

        // Stop FFmpeg process
        let output_path = self.stop_ffmpeg_capture().await?;

        // Reset state
        {
            let mut pid_guard = self.capture_pid.lock().await;
            *pid_guard = None;
        }

        *self.status.write().await = MacRecordingStatus::Idle;
        *self.start_time.write().await = None;

        tracing::info!("macOS recording stopped: {}", output_path.display());
        Ok(output_path)
    }

    /// Start FFmpeg capture with macOS configuration
    async fn start_ffmpeg_capture(&self, output_path: &PathBuf) -> Result<u32> {
        let ffmpeg_path = get_ffmpeg_path().context("Failed to find FFmpeg for macOS recording")?;

        // Detect available encoders
        let encoders = self.detect_available_encoders().await?;
        let best_encoder = encoders
            .iter()
            .find(|e| e.is_hardware && self.config.use_hardware_encoding)
            .or_else(|| encoders.iter().find(|e| e.is_hardware))
            .or_else(|| encoders.first())
            .ok_or_else(|| anyhow::anyhow!("No suitable encoder found"))?;

        // Configure FFmpeg for macOS
        let ffmpeg_config = if self.config.use_hardware_encoding {
            super::mac_ffmpeg::optimize_ffmpeg_params(
                best_encoder,
                FFmpegUseCase::RealTimeRecording,
            )
        } else {
            super::mac_ffmpeg::optimize_ffmpeg_params(
                best_encoder,
                FFmpegUseCase::HighQualityEncoding,
            )
        };

        // Build FFmpeg command
        let mut builder = MacFFmpegCommandBuilder::new(ffmpeg_config, output_path.clone())
            .add_video_input(self.config.display_id as i32)
            .with_video_params(super::mac_ffmpeg::VideoParameters {
                width: self.config.resolution.0,
                height: self.config.resolution.1,
                fps: self.config.fps as f64,
                bitrate: self.config.bitrate,
                gop_size: self.config.fps, // 1 second GOP
                max_b_frames: if self.config.use_hardware_encoding {
                    0
                } else {
                    2
                },
            });

        // Add audio if enabled
        if self.config.audio_enabled {
            builder =
                builder
                    .add_audio_input(0)
                    .with_audio_params(super::mac_ffmpeg::AudioParameters {
                        sample_rate: 44100,
                        channels: 2,
                        bitrate: 128_000,
                        enabled: true,
                    });
        }

        let mut cmd = builder.build()?;

        // Set up process for macOS
        #[cfg(target_os = "macos")]
        {
            // macOS-specific process configuration
            cmd.stdin(Stdio::null());
            cmd.stderr(Stdio::piped());
            cmd.stdout(Stdio::null());
        }

        let mut child = cmd
            .spawn()
            .context("Failed to start FFmpeg process for macOS")?;

        // Return PID (simplified - in production you'd want proper process management)
        Ok(child.id().unwrap_or(0))
    }

    /// Stop FFmpeg capture
    async fn stop_ffmpeg_capture(&self) -> Result<PathBuf> {
        let pid_guard = self.capture_pid.lock().await;

        if let Some(pid) = *pid_guard {
            // Send SIGTERM to FFmpeg process on macOS
            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                let _ = Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .spawn();

                // Wait a moment for graceful shutdown
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                // Force kill if still running
                let _ = Command::new("kill")
                    .arg("-KILL")
                    .arg(pid.to_string())
                    .spawn();
            }
        }

        // Return the most recent recording path
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("recording_{}.mp4", timestamp);
        Ok(self.config.output_dir.join(filename))
    }

    /// Set current game metadata
    pub async fn set_current_game(&self, game: Option<GameMetadata>) {
        *self.current_game.write().await = game;
    }

    /// Get current game metadata
    pub async fn get_current_game(&self) -> Option<GameMetadata> {
        self.current_game.read().await.clone()
    }

    /// Get recording statistics
    pub async fn get_stats(&self) -> MacRecordingStats {
        let total_frames = *self.total_frames.read().await;
        let start_time = *self.start_time.read().await;
        let uptime = start_time.map(|t| t.elapsed().as_secs_f64());

        let fps = if uptime.is_some() {
            total_frames as f64 / uptime.unwrap_or(1.0)
        } else {
            0.0
        };

        let capture_stats = {
            let capture = self.screen_capture.lock().await;
            capture.get_capture_stats()
        };

        MacRecordingStats {
            total_frames,
            uptime_seconds: uptime.unwrap_or(0.0),
            current_fps: fps,
            is_capturing: capture_stats.is_capturing,
            display_id: capture_stats.display_id,
            resolution: capture_stats.resolution,
            hardware_encoding: self.config.use_hardware_encoding,
            audio_enabled: self.config.audio_enabled,
        }
    }

    /// Get system performance metrics
    pub async fn get_system_metrics(&self) -> Result<MacSystemMetrics> {
        use sysinfo::{CpuExt, DiskExt, ProcessExt, System, SystemExt};

        let mut system = System::new_all();
        system.refresh_all();

        // CPU usage
        let cpu_usage = system.global_cpu_info().cpu_usage();

        // Memory usage
        let total_memory = system.total_memory();
        let used_memory = system.used_memory();
        let memory_usage_percent = (used_memory as f64 / total_memory as f64) * 100.0;

        // Disk usage for output directory
        let disk_usage = if let Ok(metadata) = std::fs::metadata(&self.config.output_dir) {
            if let Some(disk) = system
                .disks()
                .iter()
                .find(|d| self.config.output_dir.starts_with(d.mount_point()))
            {
                (disk.total_space(), disk.available_space())
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        Ok(MacSystemMetrics {
            cpu_usage_percent: cpu_usage as f64,
            memory_usage_percent,
            total_disk_space_gb: disk_usage.0 / (1024 * 1024 * 1024),
            available_disk_space_gb: disk_usage.1 / (1024 * 1024 * 1024),
            is_recording: *self.status.read().await == MacRecordingStatus::Capturing,
        })
    }

    /// Detect League of Legends windows
    pub async fn detect_lol_windows(&self) -> Result<Vec<LoLWindow>> {
        self.lol_window_detector.detect_lol_windows().await
    }

    /// Get primary LoL window for recording
    pub async fn get_primary_lol_window(&self) -> Result<Option<LoLWindow>> {
        let window = self.lol_window_detector.get_primary_lol_window().await?;

        // Update current window cache
        *self.current_lol_window.write().await = window.clone();

        Ok(window)
    }

    /// Check if League of Legends is running
    pub async fn is_lol_running(&self) -> bool {
        self.lol_window_detector.is_lol_running().await
    }

    /// Wait for LoL window to appear
    pub async fn wait_for_lol_window(&self, timeout_seconds: u64) -> Result<Option<LoLWindow>> {
        let timeout = std::time::Duration::from_secs(timeout_seconds);
        self.lol_window_detector.wait_for_lol_window(timeout).await
    }

    /// Start LoL window monitoring
    pub async fn start_lol_monitoring(&self) -> Result<()> {
        self.lol_window_detector.start_monitoring().await
    }

    /// Get current LoL window for capture
    pub async fn get_capture_window(&self) -> Option<LoLWindow> {
        // Try to get cached window first
        let cached_window = self.current_lol_window.read().await.clone();

        if let Some(window) = cached_window {
            // Check if window is still valid
            if self
                .lol_window_detector
                .is_window_valid(window.window_id)
                .await
            {
                return Some(window);
            }
        }

        // Detect new window
        if let Ok(Some(window)) = self.get_primary_lol_window().await {
            return Some(window);
        }

        None
    }

    /// Configure capture region for LoL game content
    pub async fn configure_lol_capture(
        &self,
    ) -> Result<Option<super::mac_screen_capture::MacDisplayInfo>> {
        if let Some(window) = self.get_capture_window().await {
            // Get capture region from LoL window
            let capture_region = self.lol_window_detector.get_capture_region(&window).await?;

            // Update screen capture configuration for LoL window
            let mut capture_manager = self.screen_capture.lock().await;

            // Configure to capture specific window region
            capture_manager
                .configure_window_capture(window.window_id, capture_region, self.config.fps as f64)
                .await?;

            // Return display info
            Ok(Some(super::mac_screen_capture::MacDisplayInfo {
                display_id: window.display_id,
                width: window.bounds.size.width as u32,
                height: window.bounds.size.height as u32,
                scale_factor: capture_manager
                    .get_display_scale_factor(window.display_id)
                    .await
                    .unwrap_or(2.0),
                is_main: window.display_id
                    == capture_manager.get_main_display_id().await.unwrap_or(0),
            }))
        } else {
            // No LoL window detected, use full display
            Ok(None)
        }
    }

    /// Update recording status based on LoL window detection
    pub async fn update_status_from_lol_detection(&self) {
        if self.is_lol_running().await {
            tracing::info!("League of Legends detected");

            // If we're idle and LoL is running, consider preparing
            let current_status = *self.status.read().await;
            if current_status == MacRecordingStatus::Idle {
                tracing::info!("LoL detected while idle, ready for recording");
            }
        } else {
            // If LoL is not running and we're capturing, we might want to stop
            let current_status = *self.status.read().await;
            if current_status == MacRecordingStatus::Capturing {
                tracing::warn!("LoL window disappeared during recording");
            }
        }
    }
}

/// macOS recording statistics
#[derive(Debug, Clone)]
pub struct MacRecordingStats {
    pub total_frames: u64,
    pub uptime_seconds: f64,
    pub current_fps: f64,
    pub is_capturing: bool,
    pub display_id: u32,
    pub resolution: (u32, u32),
    pub hardware_encoding: bool,
    pub audio_enabled: bool,
}

/// macOS system metrics
#[derive(Debug, Clone)]
pub struct MacSystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub total_disk_space_gb: u64,
    pub available_disk_space_gb: u64,
    pub is_recording: bool,
}

/// Initialize macOS recording backend
pub async fn initialize_recording_backend(output_dir: PathBuf) -> Result<MacRecordingManager> {
    let config = MacRecordingConfig {
        output_dir,
        ..Default::default()
    };

    tracing::info!("Initializing macOS recording backend with AVFoundation");

    // Check if we're actually on macOS
    #[cfg(not(target_os = "macos"))]
    {
        tracing::warn!("Attempting to initialize macOS backend on non-macOS platform");
        anyhow::bail!("macOS backend can only be initialized on macOS");
    }

    #[cfg(target_os = "macos")]
    {
        // Check if FFmpeg is available
        let result = std::process::Command::new("which").arg("ffmpeg").output();

        match result {
            Ok(output) if output.status.success() => {
                tracing::info!(
                    "FFmpeg found for macOS recording: {:?}",
                    String::from_utf8_lossy(&output.stdout).trim()
                );
            }
            _ => {
                tracing::warn!("FFmpeg not found - macOS recording may not work");
            }
        }

        // Check for hardware encoder availability
        match detect_available_macos_encoders().await {
            Ok(encoders) => {
                let hardware_encoders: Vec<_> = encoders.iter().filter(|e| e.is_hardware).collect();
                if !hardware_encoders.is_empty() {
                    tracing::info!(
                        "Found {} hardware encoders: {}",
                        hardware_encoders.len(),
                        hardware_encoders
                            .iter()
                            .map(|e| &e.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                } else {
                    tracing::info!("No hardware encoders found, will use software encoding");
                }
            }
            Err(e) => {
                tracing::warn!("Failed to detect encoders: {}", e);
            }
        }
    }

    MacRecordingManager::new(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_recording_config_default() {
        let config = MacRecordingConfig::default();
        assert_eq!(config.fps, 60);
        assert_eq!(config.bitrate, 15_000_000);
        assert_eq!(config.resolution, (1920, 1080));
        assert_eq!(config.display_id, 0);
        assert!(config.use_hardware_encoding);
    }

    #[test]
    fn test_mac_recording_status_transitions() {
        let status = MacRecordingStatus::default();
        assert_eq!(status, MacRecordingStatus::Idle);

        // Test status transitions
        assert_ne!(status, MacRecordingStatus::Capturing);
        assert_ne!(status, MacRecordingStatus::Error);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_macos_backend_initialization() {
        // This test would require actual macOS environment
        // In CI, we'd mock the system calls
        let temp_dir = tempfile::tempdir().unwrap();
        let result = initialize_recording_backend(temp_dir.path().to_path_buf()).await;

        // On non-macOS platforms in CI, this should fail
        #[cfg(not(target_os = "macos"))]
        {
            assert!(result.is_err());
        }
    }
}
