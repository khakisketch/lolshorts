#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 녹화 설정
#[derive(Debug, Clone)]
pub struct RecordingConfig {
    pub fps: u32,
    pub bitrate: u32,
    pub resolution: (u32, u32),
    pub encoder: VideoEncoder,
    pub hw_accel: HwAccel,
    pub output_dir: PathBuf,
    pub buffer_duration_secs: u64,
    pub segment_duration_secs: u64,
    pub audio_config: Option<crate::recording::audio::AudioConfig>,
    /// 캡처할 창 제목 (None이면 전체 화면) - fallback only
    pub capture_target: Option<String>,
    /// HWND of the LoL window for precise desktop-region capture.
    /// When set, gdigrab captures `-i desktop` with offset/size from GetWindowRect,
    /// avoiding the alt-tab bug where title-based capture grabs the wrong window.
    pub capture_hwnd: Option<isize>,
    /// Monitor index for multi-monitor capture (0 = primary, None = primary)
    pub monitor_index: Option<u32>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        let hwnd = find_league_hwnd();
        let capture_target = if hwnd.is_some() {
            None // HWND-based capture preferred; no title needed
        } else {
            Some(get_league_capture_title()) // fallback to title-based
        };
        Self {
            fps: 60,
            bitrate: 15_000_000, // 15 Mbps
            resolution: (1920, 1080),
            encoder: VideoEncoder::H264,
            hw_accel: HwAccel::Auto,
            output_dir: PathBuf::from("./recordings"),
            buffer_duration_secs: 60,
            segment_duration_secs: 10,
            audio_config: Some(crate::recording::audio::AudioConfig::default()),
            capture_target,
            capture_hwnd: hwnd,
            monitor_index: None,
        }
    }
}

/// Find the LoL game window HWND by trying known window titles.
/// Returns `Some(hwnd)` if a matching window is found, `None` otherwise.
pub fn find_league_hwnd() -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
            fn IsWindow(hwnd: isize) -> i32;
        }

        fn to_wide_null(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }

        const TITLES: &[&str] = &[
            "League of Legends (TM) Client",
            "League of Legends",
            "\u{B9AC}\u{ADF8} \u{C624}\u{BE0C} \u{B808}\u{C804}\u{B4DC}", // 리그 오브 레전드
            "\u{B9AC}\u{ADF8}\u{C624}\u{BE0C}\u{B808}\u{C804}\u{B4DC}",   // 리그오브레전드
            "\u{82F1}\u{96C4}\u{8054}\u{76DF}",                           // 英雄联盟
            "\u{806F}\u{76DF}\u{722D}\u{9738}",                           // 聯盟爭霸
        ];

        for title in TITLES {
            let wide = to_wide_null(title);
            let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
            if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
                tracing::info!("Found LoL HWND: 0x{:X} (title: {})", hwnd, title);
                return Some(hwnd);
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (); // suppress unused warning
    }

    None
}

/// Get the window rectangle (x, y, width, height) for a given HWND.
/// Returns `None` if the HWND is invalid or the call fails.
#[cfg(target_os = "windows")]
pub fn get_window_rect(hwnd: isize) -> Option<(i32, i32, u32, u32)> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowRect(hwnd: isize, lp_rect: *mut Rect) -> i32;
        fn IsWindow(hwnd: isize) -> i32;
    }

    if unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }

    let mut rect = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    if ok == 0 {
        tracing::warn!("GetWindowRect failed for HWND 0x{:X}", hwnd);
        return None;
    }

    let w = (rect.right - rect.left).unsigned_abs();
    let h = (rect.bottom - rect.top).unsigned_abs();

    if w == 0 || h == 0 {
        tracing::warn!(
            "GetWindowRect returned zero-size for HWND 0x{:X}: {}x{}",
            hwnd,
            w,
            h
        );
        return None;
    }

    tracing::debug!(
        "HWND 0x{:X} rect: ({},{}) {}x{}",
        hwnd,
        rect.left,
        rect.top,
        w,
        h
    );
    Some((rect.left, rect.top, w, h))
}

/// LoL 게임 윈도우 타이틀을 런타임에 탐지하여 반환.
/// FFmpeg gdigrab는 유니코드 타이틀을 처리하지 못하므로 영문 타이틀을 우선 탐지한다.
pub fn get_league_capture_title() -> String {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn FindWindowW(lp_class_name: *const u16, lp_window_name: *const u16) -> isize;
        }

        fn to_wide_null(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }

        // English-first order: FFmpeg gdigrab can't handle Unicode titles
        const TITLES: &[&str] = &[
            "League of Legends (TM) Client",
            "League of Legends",
            "리그 오브 레전드",
            "리그오브레전드",
            "英雄联盟",
            "聯盟爭霸",
        ];

        for title in TITLES {
            let wide = to_wide_null(title);
            let hwnd = unsafe { FindWindowW(std::ptr::null(), wide.as_ptr()) };
            if hwnd != 0 {
                tracing::info!("Detected LoL window title: {}", title);
                return title.to_string();
            }
        }
    }

    // Fallback: use English title (most compatible with FFmpeg)
    "League of Legends (TM) Client".to_string()
}

/// 크로스 플랫폼 지원 비디오 인코더 옵션
#[derive(Debug, Clone, Copy)]
pub enum VideoEncoder {
    H264,
    H265,
}

impl VideoEncoder {
    /// 현재 플랫폼에 최적화된 FFmpeg 인코더 이름 반환 (하드웨어 가속)
    pub fn to_ffmpeg_name(&self) -> &'static str {
        #[cfg(target_os = "windows")]
        {
            match self {
                VideoEncoder::H264 => "h264_nvenc",
                VideoEncoder::H265 => "hevc_nvenc",
            }
        }

        #[cfg(target_os = "macos")]
        {
            match self {
                VideoEncoder::H264 => "h264_videotoolbox",
                VideoEncoder::H265 => "hevc_videotoolbox",
            }
        }

        #[cfg(target_os = "linux")]
        {
            match self {
                VideoEncoder::H264 => "libx264",
                VideoEncoder::H265 => "libx265",
            }
        }
    }

    /// 지정된 하드웨어 가속으로 FFmpeg 인코더 이름 반환
    pub fn to_ffmpeg_name_with_hw(&self, hw: HwAccel) -> &'static str {
        match hw {
            HwAccel::Nvenc => match self {
                VideoEncoder::H264 => "h264_nvenc",
                VideoEncoder::H265 => "hevc_nvenc",
            },
            HwAccel::Qsv => match self {
                VideoEncoder::H264 => "h264_qsv",
                VideoEncoder::H265 => "hevc_qsv",
            },
            HwAccel::Amf => match self {
                VideoEncoder::H264 => "h264_amf",
                VideoEncoder::H265 => "hevc_amf",
            },
            HwAccel::Software => self.to_software_fallback(),
            HwAccel::Auto => self.to_ffmpeg_name(),
        }
    }

    /// 호환성을 위한 소프트웨어 폴백 인코더 반환
    pub fn to_software_fallback(&self) -> &'static str {
        match self {
            VideoEncoder::H264 => "libx264",
            VideoEncoder::H265 => "libx265",
        }
    }

    pub fn to_extension(&self) -> &'static str {
        match self {
            VideoEncoder::H264 => "mp4",
            VideoEncoder::H265 => "mp4",
        }
    }
}

/// 하드웨어 가속 옵션
#[derive(Debug, Clone, Copy, Default)]
pub enum HwAccel {
    #[default]
    Auto,
    Nvenc,
    Qsv,
    Amf,
    Software,
}

/// 녹화 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecordingStatus {
    #[default]
    Idle,
    Buffering,
    Recording,
    Processing,
    Error,
}

/// 녹화 통계
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordingStats {
    pub total_frames: u64,
    pub uptime_seconds: f64,
    pub current_fps: f64,
}

/// 시스템 상태 정보
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct SystemStatus {
    pub recording_status: RecordingStatus,
    pub stats: RecordingStats,
    pub current_game: Option<crate::storage::GameMetadata>,
}

/// Video settings for recording initialization
#[derive(Debug, Clone)]
pub struct VideoSettingsConfig {
    pub resolution: (u32, u32),
    pub fps: u32,
    pub bitrate: u32,
    pub use_h265: bool,
    pub encoder_preference: String,
}

impl Default for VideoSettingsConfig {
    fn default() -> Self {
        Self {
            resolution: (1920, 1080),
            fps: 60,
            bitrate: 15_000_000,
            use_h265: false,
            encoder_preference: "auto".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- RecordingConfig ----

    #[test]
    fn recording_config_default_has_expected_fps() {
        let config = RecordingConfig::default();
        assert_eq!(config.fps, 60);
    }

    #[test]
    fn recording_config_default_has_expected_bitrate() {
        let config = RecordingConfig::default();
        assert_eq!(config.bitrate, 15_000_000);
    }

    #[test]
    fn recording_config_default_has_expected_resolution() {
        let config = RecordingConfig::default();
        assert_eq!(config.resolution, (1920, 1080));
    }

    #[test]
    fn recording_config_default_has_buffer_duration_60s() {
        let config = RecordingConfig::default();
        assert_eq!(config.buffer_duration_secs, 60);
    }

    #[test]
    fn recording_config_default_has_segment_duration_10s() {
        let config = RecordingConfig::default();
        assert_eq!(config.segment_duration_secs, 10);
    }

    #[test]
    fn recording_config_default_has_capture_target_or_hwnd() {
        let config = RecordingConfig::default();
        // Either HWND-based or title-based capture must be configured
        assert!(
            config.capture_hwnd.is_some() || config.capture_target.is_some(),
            "Expected either capture_hwnd or capture_target to be set"
        );
    }

    #[test]
    fn recording_config_default_has_audio_config() {
        let config = RecordingConfig::default();
        assert!(config.audio_config.is_some());
    }

    // ---- find_league_hwnd ----

    #[test]
    fn find_league_hwnd_returns_option() {
        // LoL is unlikely to be running during tests, so None is expected
        let hwnd = find_league_hwnd();
        // Just verify it doesn't panic; value depends on runtime state
        let _ = hwnd;
    }

    // ---- get_league_capture_title ----

    #[test]
    fn get_league_capture_title_returns_non_empty_string() {
        let title = get_league_capture_title();
        assert!(!title.is_empty());
    }

    // ---- VideoEncoder::to_ffmpeg_name ----

    #[test]
    fn video_encoder_h264_to_ffmpeg_name_returns_non_empty_string() {
        let name = VideoEncoder::H264.to_ffmpeg_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn video_encoder_h265_to_ffmpeg_name_returns_non_empty_string() {
        let name = VideoEncoder::H265.to_ffmpeg_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn video_encoder_h264_h265_produce_different_ffmpeg_names() {
        let h264 = VideoEncoder::H264.to_ffmpeg_name();
        let h265 = VideoEncoder::H265.to_ffmpeg_name();
        assert_ne!(h264, h265);
    }

    // ---- VideoEncoder::to_ffmpeg_name_with_hw ----

    #[test]
    fn video_encoder_nvenc_h264_returns_h264_nvenc() {
        let name = VideoEncoder::H264.to_ffmpeg_name_with_hw(HwAccel::Nvenc);
        assert_eq!(name, "h264_nvenc");
    }

    #[test]
    fn video_encoder_nvenc_h265_returns_hevc_nvenc() {
        let name = VideoEncoder::H265.to_ffmpeg_name_with_hw(HwAccel::Nvenc);
        assert_eq!(name, "hevc_nvenc");
    }

    #[test]
    fn video_encoder_qsv_h264_returns_h264_qsv() {
        let name = VideoEncoder::H264.to_ffmpeg_name_with_hw(HwAccel::Qsv);
        assert_eq!(name, "h264_qsv");
    }

    #[test]
    fn video_encoder_qsv_h265_returns_hevc_qsv() {
        let name = VideoEncoder::H265.to_ffmpeg_name_with_hw(HwAccel::Qsv);
        assert_eq!(name, "hevc_qsv");
    }

    #[test]
    fn video_encoder_amf_h264_returns_h264_amf() {
        let name = VideoEncoder::H264.to_ffmpeg_name_with_hw(HwAccel::Amf);
        assert_eq!(name, "h264_amf");
    }

    #[test]
    fn video_encoder_amf_h265_returns_hevc_amf() {
        let name = VideoEncoder::H265.to_ffmpeg_name_with_hw(HwAccel::Amf);
        assert_eq!(name, "hevc_amf");
    }

    #[test]
    fn video_encoder_software_h264_returns_libx264() {
        let name = VideoEncoder::H264.to_ffmpeg_name_with_hw(HwAccel::Software);
        assert_eq!(name, "libx264");
    }

    #[test]
    fn video_encoder_software_h265_returns_libx265() {
        let name = VideoEncoder::H265.to_ffmpeg_name_with_hw(HwAccel::Software);
        assert_eq!(name, "libx265");
    }

    // ---- VideoEncoder::to_software_fallback ----

    #[test]
    fn video_encoder_h264_software_fallback_is_libx264() {
        assert_eq!(VideoEncoder::H264.to_software_fallback(), "libx264");
    }

    #[test]
    fn video_encoder_h265_software_fallback_is_libx265() {
        assert_eq!(VideoEncoder::H265.to_software_fallback(), "libx265");
    }

    // ---- VideoEncoder::to_extension ----

    #[test]
    fn video_encoder_h264_extension_is_mp4() {
        assert_eq!(VideoEncoder::H264.to_extension(), "mp4");
    }

    #[test]
    fn video_encoder_h265_extension_is_mp4() {
        assert_eq!(VideoEncoder::H265.to_extension(), "mp4");
    }

    // ---- HwAccel ----

    #[test]
    fn hw_accel_default_is_auto() {
        let hw: HwAccel = HwAccel::default();
        assert!(matches!(hw, HwAccel::Auto));
    }

    // ---- RecordingStatus ----

    #[test]
    fn recording_status_default_is_idle() {
        let status = RecordingStatus::default();
        assert_eq!(status, RecordingStatus::Idle);
    }

    #[test]
    fn recording_status_equality_same_variants() {
        assert_eq!(RecordingStatus::Recording, RecordingStatus::Recording);
        assert_eq!(RecordingStatus::Error, RecordingStatus::Error);
    }

    #[test]
    fn recording_status_inequality_different_variants() {
        assert_ne!(RecordingStatus::Idle, RecordingStatus::Recording);
        assert_ne!(RecordingStatus::Buffering, RecordingStatus::Processing);
    }

    #[test]
    fn recording_status_serializes_to_lowercase() {
        let idle = serde_json::to_string(&RecordingStatus::Idle).unwrap();
        assert_eq!(idle, "\"idle\"");

        let recording = serde_json::to_string(&RecordingStatus::Recording).unwrap();
        assert_eq!(recording, "\"recording\"");
    }

    #[test]
    fn recording_status_deserializes_from_lowercase() {
        let status: RecordingStatus = serde_json::from_str("\"buffering\"").unwrap();
        assert_eq!(status, RecordingStatus::Buffering);
    }

    // ---- VideoSettingsConfig ----

    #[test]
    fn video_settings_config_default_resolution_is_1080p() {
        let config = VideoSettingsConfig::default();
        assert_eq!(config.resolution, (1920, 1080));
    }

    #[test]
    fn video_settings_config_default_fps_is_60() {
        let config = VideoSettingsConfig::default();
        assert_eq!(config.fps, 60);
    }

    #[test]
    fn video_settings_config_default_bitrate_is_15mbps() {
        let config = VideoSettingsConfig::default();
        assert_eq!(config.bitrate, 15_000_000);
    }

    #[test]
    fn video_settings_config_default_encoder_preference_is_auto() {
        let config = VideoSettingsConfig::default();
        assert_eq!(config.encoder_preference, "auto");
    }

    #[test]
    fn video_settings_config_default_use_h265_is_false() {
        let config = VideoSettingsConfig::default();
        assert!(!config.use_h265);
    }
}
