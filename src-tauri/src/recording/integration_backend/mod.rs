//! FFmpeg gdigrab 기반 화면 캡처 모듈
//!
//! Windows: gdigrab (FFmpeg 내장 화면 캡처)
//! macOS: avfoundation
//! Linux: x11grab
//!
//! 세그먼트 기반 순환 버퍼로 60초 리플레이 윈도우 유지

#![allow(dead_code)]

pub mod segment_recorder;
pub mod types;
pub mod windows_capture;

pub use segment_recorder::SegmentRecorder;
pub use types::{
    get_league_capture_title, HwAccel, RecordingConfig, RecordingStats, RecordingStatus,
    SystemStatus, VideoEncoder, VideoSettingsConfig,
};
pub use windows_capture::WindowsCaptureRecorder;

/// 메인 녹화 매니저 타입 정의
pub type RecordingManager = WindowsCaptureRecorder;

/// 녹화 백엔드 초기화 (전체 옵션 지원)
///
/// # Arguments
/// * `output_dir` - 출력 디렉토리
/// * `audio_config` - 오디오 설정 (선택)
/// * `video_config` - 비디오 설정 (선택, 기본값: 1920x1080 60fps)
pub async fn initialize_recording_backend_full(
    output_dir: std::path::PathBuf,
    audio_config: Option<crate::recording::audio::AudioConfig>,
    video_config: Option<VideoSettingsConfig>,
) -> anyhow::Result<RecordingManager> {
    use tracing::info;

    let video = video_config.unwrap_or_default();

    let encoder = if video.use_h265 {
        VideoEncoder::H265
    } else {
        VideoEncoder::H264
    };

    let hw_accel = match video.encoder_preference.as_str() {
        "nvenc" => HwAccel::Nvenc,
        "qsv" => HwAccel::Qsv,
        "amf" => HwAccel::Amf,
        "software" => HwAccel::Software,
        _ => HwAccel::Auto,
    };

    let config = RecordingConfig {
        fps: video.fps,
        bitrate: video.bitrate,
        resolution: video.resolution,
        encoder,
        hw_accel,
        output_dir,
        audio_config,
        ..Default::default()
    };

    info!(
        "Initializing recording: {}x{} @ {}fps, {} Mbps, {:?}",
        config.resolution.0,
        config.resolution.1,
        config.fps,
        config.bitrate / 1_000_000,
        config.encoder
    );

    WindowsCaptureRecorder::new(config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_recorder_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecordingConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let recorder = WindowsCaptureRecorder::new(config).await;
        assert!(recorder.is_ok());

        let recorder = recorder.unwrap();
        assert_eq!(recorder.get_status().await, RecordingStatus::Idle);
    }

    #[tokio::test]
    async fn test_video_encoder() {
        #[cfg(target_os = "windows")]
        {
            assert_eq!(VideoEncoder::H264.to_ffmpeg_name(), "h264_nvenc");
            assert_eq!(VideoEncoder::H265.to_ffmpeg_name(), "hevc_nvenc");
        }
        assert_eq!(VideoEncoder::H264.to_extension(), "mp4");
    }

    #[tokio::test]
    async fn test_config_default() {
        let config = RecordingConfig::default();
        assert_eq!(config.fps, 60);
        assert_eq!(config.bitrate, 15_000_000);
        assert_eq!(config.resolution, (1920, 1080));
        assert_eq!(config.buffer_duration_secs, 60);
        assert_eq!(config.segment_duration_secs, 10);
    }

    #[test]
    fn test_segment_recorder_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecordingConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let recorder = SegmentRecorder::new(config);
        assert!(recorder.is_ok());
    }
}
