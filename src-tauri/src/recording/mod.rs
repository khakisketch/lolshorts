// Complete windows-capture v2 + FFmpeg integration
pub mod integration_backend;

// macOS-specific recording backend
#[cfg(target_os = "macos")]
mod mac_audio;
#[cfg(target_os = "macos")]
mod mac_audio_core;
#[cfg(target_os = "macos")]
mod mac_backend;
#[cfg(target_os = "macos")]
mod mac_ffmpeg;
#[cfg(target_os = "macos")]
mod mac_screen_capture;

// WASAPI loopback audio capture (replaces unreliable DirectShow Stereo Mix)
#[cfg(target_os = "windows")]
pub mod wasapi_audio;

// Common types and interfaces
pub mod audio;
pub mod auto_clip_manager;
pub mod commands;
pub mod game_lifecycle;
pub mod game_monitor;
pub mod highlight_score;
pub mod live_client;

// Complete integration exports
pub use integration_backend::RecordingManager;

// Platform-specific exports
#[cfg(target_os = "macos")]
pub use mac_audio::{MacAudioDevice, MacAudioDeviceType, MacAudioManager};
#[cfg(target_os = "macos")]
pub use mac_audio_core::{CoreAudioDevice, CoreAudioDeviceType, CoreAudioManager};
#[cfg(target_os = "macos")]
pub use mac_backend::{MacRecordingConfig, MacRecordingManager, MacRecordingStatus};
#[cfg(target_os = "macos")]
pub use mac_ffmpeg::{detect_available_macos_encoders, MacFFmpegCommandBuilder, MacFFmpegConfig};
#[cfg(target_os = "macos")]
pub use mac_screen_capture::{MacDisplayInfo, MacScreenCaptureConfig, MacScreenCaptureManager};

/// Platform detection utilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    #[allow(dead_code)]
    MacOS,
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        return Platform::Windows;

        #[cfg(target_os = "macos")]
        return Platform::MacOS;
    }

    pub fn name(&self) -> &'static str {
        match self {
            Platform::Windows => "Windows",
            Platform::MacOS => "macOS",
        }
    }
}

// Platform-specific initialization
// Note: Use initialize_recording_backend_full for complete initialization with all options
pub use integration_backend::initialize_recording_backend_full;
pub use integration_backend::VideoSettingsConfig;
pub use integration_backend::{
    CaptureBackend, CaptureMode, HwAccel, RecordingConfig, RecordingStatus, VideoEncoder,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::RecordingStatus;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::current();

        #[cfg(target_os = "windows")]
        assert_eq!(platform, Platform::Windows);

        #[cfg(target_os = "macos")]
        assert_eq!(platform, Platform::MacOS);

        // Test platform matching
        match platform {
            Platform::Windows | Platform::MacOS => {}
        }
    }

    #[test]
    fn test_platform_names() {
        assert_eq!(Platform::Windows.name(), "Windows");
        assert_eq!(Platform::MacOS.name(), "macOS");
    }

    #[test]
    fn test_recording_status_default() {
        let status = RecordingStatus::default();
        assert_eq!(status, RecordingStatus::Idle);
    }

    #[test]
    fn test_recording_status_equality() {
        assert_eq!(RecordingStatus::Idle, RecordingStatus::Idle);
        assert_ne!(RecordingStatus::Idle, RecordingStatus::Recording);
    }
}
