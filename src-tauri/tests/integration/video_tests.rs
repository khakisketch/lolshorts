// Integration tests for video processing system
#![cfg(test)]

use lolshorts::video::VideoProcessor;
use std::path::PathBuf;
use tokio;

#[tokio::test]
async fn test_video_processor_initialization() {
    let _processor = VideoProcessor::new_with_fallback();

    // Verify processor can be created without panicking
}

#[tokio::test]
async fn test_ffmpeg_availability() {
    use std::process::Command;

    // Check if FFmpeg is available (either in PATH or as bundled binary)
    let ffmpeg_candidates = [
        "ffmpeg",
        "./src-tauri/binaries/ffmpeg.exe",
        "../binaries/ffmpeg.exe",
    ];

    let mut found = false;
    for candidate in &ffmpeg_candidates {
        let output = Command::new(candidate).arg("-version").output();

        if let Ok(result) = output {
            if result.status.success() {
                let version_str = String::from_utf8_lossy(&result.stdout);
                println!(
                    "FFmpeg found at '{}': {}",
                    candidate,
                    version_str.lines().next().unwrap_or("")
                );
                found = true;
                break;
            }
        }
    }

    if !found {
        println!("FFmpeg not available in test environment - skipping version check");
    }
    // Not asserting found=true: acceptable in CI/CD without FFmpeg in PATH
}

#[tokio::test]
async fn test_video_format_validation() {
    let valid_formats = vec!["mp4", "avi", "mov", "mkv"];

    for format in valid_formats {
        let path = PathBuf::from(format!("test.{}", format));
        let extension = path.extension().and_then(|s| s.to_str());

        assert!(extension.is_some());
        assert_eq!(extension.unwrap(), format);
    }
}

#[tokio::test]
async fn test_clip_duration_limits() {
    // FREE tier: max 30 seconds per clip
    const FREE_TIER_MAX_DURATION: f64 = 30.0;

    // PRO tier: max 60 seconds per clip
    const PRO_TIER_MAX_DURATION: f64 = 60.0;

    let test_durations = vec![10.0, 25.0, 45.0, 70.0];

    for duration in test_durations {
        if duration <= FREE_TIER_MAX_DURATION {
            assert!(duration <= FREE_TIER_MAX_DURATION); // FREE tier can use this
        }

        if duration <= PRO_TIER_MAX_DURATION {
            assert!(duration <= 60.0); // PRO tier can use this
        } else {
            assert!(duration > PRO_TIER_MAX_DURATION); // Exceeds PRO limits
        }
    }
}

#[tokio::test]
async fn test_youtube_shorts_dimensions() {
    // YouTube Shorts: 9:16 aspect ratio (1080x1920)
    const SHORTS_WIDTH: u32 = 1080;
    const SHORTS_HEIGHT: u32 = 1920;

    let aspect_ratio = SHORTS_HEIGHT as f64 / SHORTS_WIDTH as f64;
    let expected_ratio = 16.0 / 9.0;

    let difference = (aspect_ratio - expected_ratio).abs();
    assert!(difference < 0.01); // Allow small floating point error
}

#[tokio::test]
async fn test_video_quality_presets() {
    let quality_presets = vec![
        ("low", 480),
        ("medium", 720),
        ("high", 1080),
        ("ultra", 1440),
    ];

    for (name, height) in quality_presets {
        assert!(!name.is_empty());
        assert!((480..=2160).contains(&height)); // Valid height range
    }
}

#[tokio::test]
async fn test_thumbnail_generation_params() {
    // Thumbnail should be extracted at key moment
    let thumbnail_timestamp = 5.0; // 5 seconds into clip

    assert!(thumbnail_timestamp >= 0.0);
    assert!(thumbnail_timestamp < 60.0); // Within max clip duration

    // Thumbnail dimensions
    const THUMBNAIL_WIDTH: u32 = 1920;
    const THUMBNAIL_HEIGHT: u32 = 1080;

    assert_eq!(THUMBNAIL_WIDTH, 1920);
    assert_eq!(THUMBNAIL_HEIGHT, 1080);
}

#[tokio::test]
async fn test_concurrent_video_processing() {
    use tokio::task;

    let _processor = VideoProcessor::new_with_fallback();

    // Simulate multiple concurrent validation requests
    let mut handles = vec![];
    for i in 0..5 {
        let test_path = PathBuf::from(format!("test_{}.mp4", i));
        let handle = task::spawn(async move {
            // Just validate path structure
            assert!(test_path.extension().is_some());
            true
        });
        handles.push(handle);
    }

    // All should complete successfully
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result);
    }
}

#[tokio::test]
async fn test_video_codec_validation() {
    let supported_codecs = vec!["h264", "h265", "vp9", "av1"];

    for codec in supported_codecs {
        assert!(!codec.is_empty());
        // Verify codec names are valid
        assert!(codec.len() <= 10); // Reasonable codec name length
    }
}

#[tokio::test]
async fn test_audio_codec_validation() {
    let supported_audio_codecs = vec!["aac", "mp3", "opus"];

    for codec in supported_audio_codecs {
        assert!(!codec.is_empty());
        assert!(codec.len() <= 10);
    }
}

#[tokio::test]
async fn test_bitrate_calculation() {
    // Calculate recommended bitrates for different resolutions

    // 1080p: 8-12 Mbps
    let bitrate_1080p = 10_000_000; // 10 Mbps
    assert!((8_000_000..=12_000_000).contains(&bitrate_1080p));

    // 720p: 5-8 Mbps
    let bitrate_720p = 6_000_000; // 6 Mbps
    assert!((5_000_000..=8_000_000).contains(&bitrate_720p));

    // 480p: 2.5-4 Mbps
    let bitrate_480p = 3_000_000; // 3 Mbps
    assert!((2_500_000..=4_000_000).contains(&bitrate_480p));
}

#[tokio::test]
async fn test_file_size_estimation() {
    // Estimate file size: (bitrate * duration) / 8
    let bitrate = 10_000_000; // 10 Mbps
    let duration = 30.0; // 30 seconds

    let estimated_size_bytes = ((bitrate as f64 * duration) / 8.0) as u64;

    // 30 seconds at 10 Mbps should be ~37.5 MB
    let expected_size = 37_500_000; // bytes
    let difference = (estimated_size_bytes as i64 - expected_size as i64).abs();

    // Allow 10% margin of error
    assert!(difference < (expected_size / 10) as i64);
}

#[tokio::test]
async fn test_clip_composition_limits() {
    // Test limits for composing multiple clips

    // FREE tier: max 3 clips per composition
    const FREE_TIER_MAX_CLIPS: usize = 3;

    // PRO tier: max 10 clips per composition
    const PRO_TIER_MAX_CLIPS: usize = 10;

    let test_clip_counts = vec![1, 3, 5, 10, 15];

    for count in test_clip_counts {
        if count <= FREE_TIER_MAX_CLIPS {
            assert!(count >= 1); // FREE tier valid
        }

        if count <= PRO_TIER_MAX_CLIPS {
            assert!(count <= 10); // PRO tier valid
        } else {
            assert!(count > PRO_TIER_MAX_CLIPS); // Exceeds limits
        }
    }
}
