// Integration tests for WindowsCaptureRecorder
#![cfg(test)]

use lolshorts::recording::audio::AudioConfig;
use lolshorts::recording::integration_backend::{
    RecordingConfig, RecordingStatus, SegmentRecorder, VideoEncoder, WindowsCaptureRecorder,
};
use lolshorts::storage::GameMetadata;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Helper to create test recording configuration
fn create_test_config() -> RecordingConfig {
    RecordingConfig {
        fps: 30,
        bitrate: 5_000_000,      // 5 Mbps for testing
        resolution: (1280, 720), // Smaller for testing
        encoder: VideoEncoder::H264,
        output_dir: PathBuf::from("./test_recordings"),
        buffer_duration_secs: 30, // 30 seconds for testing
        audio_config: Some(AudioConfig::default()),
        ..Default::default()
    }
}

/// Helper to create test audio configuration
fn create_test_audio_config() -> AudioConfig {
    AudioConfig {
        record_microphone: false, // Disable for CI testing
        microphone_device: None,
        microphone_volume: 100,
        record_system_audio: false, // Disable for CI testing
        system_audio_device: None,
        system_audio_volume: 100,
        sample_rate: 44100,
        bitrate: 128,
        audio_device_id: None,
    }
}

#[tokio::test]
async fn test_windows_capture_recorder_initialization() {
    // Create temporary directory for test recordings
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    // Test recorder creation
    let recorder = WindowsCaptureRecorder::new(config).await;
    assert!(
        recorder.is_ok(),
        "Failed to create WindowsCaptureRecorder: {:?}",
        recorder.err()
    );

    let recorder = recorder.unwrap();

    // Test initial state
    let status = recorder.get_status().await;
    assert_eq!(status, RecordingStatus::Idle);

    // Test initial stats
    let stats = recorder.get_stats().await;
    assert_eq!(stats.total_frames, 0);
    assert_eq!(stats.uptime_seconds, 0.0);
    // current_fps may reflect configured fps when not recording

    // Test initial game state
    let current_game = recorder.get_current_game().await;
    assert!(current_game.is_none());
}

#[tokio::test]
async fn test_windows_capture_recorder_start_stop() {
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        encoder: VideoEncoder::H264,
        ..create_test_config()
    };

    let mut recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    // Test starting recording
    let start_result = recorder.start_recording().await;

    // Note: This might fail in CI without FFmpeg, which is expected
    if start_result.is_ok() {
        // Give it a moment to initialize
        sleep(Duration::from_millis(100)).await;

        // Check status changed to Recording or Buffering
        let status = recorder.get_status().await;
        assert!(status == RecordingStatus::Recording || status == RecordingStatus::Buffering);

        // Test stopping recording
        if let Ok(output_path) = recorder.stop_recording().await {
            // Verify output path is valid
            assert!(output_path.to_str().is_some());
        }

        // Check status returned to Idle
        let status = recorder.get_status().await;
        assert_eq!(status, RecordingStatus::Idle);
    } else {
        println!(
            "Recording start failed as expected in CI environment: {:?}",
            start_result.err()
        );
    }
}

#[tokio::test]
async fn test_windows_capture_recorder_double_start_protection() {
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let mut recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    // Start first recording (may fail in CI)
    let first_start = recorder.start_recording().await;

    if first_start.is_ok() {
        // Try to start second recording - should fail
        let second_start = recorder.start_recording().await;
        assert!(second_start.is_err(), "Second recording start should fail");

        // Clean up
        let _ = recorder.stop_recording().await;
    }
}

#[tokio::test]
async fn test_windows_capture_recorder_stop_without_start() {
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let mut recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    // Try to stop recording without starting - should fail
    let stop_result = recorder.stop_recording().await;
    assert!(stop_result.is_err(), "Stop without start should fail");
}

#[tokio::test]
async fn test_windows_capture_recorder_game_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    // Create test game metadata (matching actual GameMetadata struct)
    let test_game = GameMetadata {
        game_id: "12345".to_string(),
        champion: "Ahri".to_string(),
        game_mode: "Ranked Solo".to_string(),
        start_time: chrono::Utc::now(),
        end_time: None,
        result: None,
        kda: None,
    };

    // Test setting game metadata
    recorder.set_current_game(Some(test_game.clone())).await;

    // Test getting game metadata
    let retrieved_game = recorder.get_current_game().await;
    assert!(retrieved_game.is_some(), "Game metadata should be set");

    let retrieved_game = retrieved_game.unwrap();
    assert_eq!(retrieved_game.game_id, test_game.game_id);
    assert_eq!(retrieved_game.champion, test_game.champion);
    assert_eq!(retrieved_game.game_mode, test_game.game_mode);

    // Test clearing game metadata
    recorder.set_current_game(None).await;
    let cleared_game = recorder.get_current_game().await;
    assert!(cleared_game.is_none(), "Game metadata should be cleared");
}

#[tokio::test]
async fn test_windows_capture_recorder_different_encoders() {
    let temp_dir = TempDir::new().unwrap();

    for encoder in [VideoEncoder::H264, VideoEncoder::H265] {
        let config = RecordingConfig {
            output_dir: temp_dir.path().to_path_buf(),
            encoder,
            resolution: (160, 120), // Very small for testing
            ..create_test_config()
        };

        let recorder = WindowsCaptureRecorder::new(config).await.unwrap();

        // Test basic functionality with different encoders
        let status = recorder.get_status().await;
        assert_eq!(status, RecordingStatus::Idle);

        println!("Encoder {:?} test completed", encoder);
    }
}

#[tokio::test]
async fn test_windows_capture_recorder_audio_config() {
    let temp_dir = TempDir::new().unwrap();

    // Test with audio disabled (CI safe)
    let audio_config = AudioConfig {
        record_microphone: false,
        record_system_audio: false,
        ..create_test_audio_config()
    };

    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        audio_config: Some(audio_config),
        ..create_test_config()
    };

    let recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    // Test that recorder can be created with audio config
    let status = recorder.get_status().await;
    assert_eq!(status, RecordingStatus::Idle);
}

#[tokio::test]
async fn test_windows_capture_recorder_microphone_enabled_config() {
    // A microphone-enabled config must build a recorder without panicking. Mic
    // capture only starts at start_recording() time (and, per spec, never blocks
    // recording on failure), so construction here is CI-safe with no audio hardware.
    let temp_dir = TempDir::new().unwrap();

    let audio_config = AudioConfig {
        record_microphone: true,
        microphone_device: None,
        microphone_volume: 120,
        record_system_audio: false,
        system_audio_device: None,
        system_audio_volume: 100,
        sample_rate: 48000,
        bitrate: 192,
        audio_device_id: None,
    };

    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        audio_config: Some(audio_config),
        ..create_test_config()
    };

    let recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    let status = recorder.get_status().await;
    assert_eq!(status, RecordingStatus::Idle);

    // Before any recording starts nothing is captured, so system audio is inactive.
    let stats = recorder.get_stats().await;
    assert!(!stats.audio_active);
}

#[tokio::test]
async fn test_windows_capture_recorder_concurrent_access() {
    use tokio::task;

    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let recorder = std::sync::Arc::new(WindowsCaptureRecorder::new(config).await.unwrap());

    // Spawn multiple concurrent tasks to access recorder
    let mut handles = vec![];

    // Task 1: Check status multiple times
    let recorder_clone1 = recorder.clone();
    let handle1 = task::spawn(async move {
        for _ in 0..10 {
            let _status = recorder_clone1.get_status().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    handles.push(handle1);

    // Task 2: Check stats multiple times
    let recorder_clone2 = recorder.clone();
    let handle2 = task::spawn(async move {
        for _ in 0..10 {
            let _stats = recorder_clone2.get_stats().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    handles.push(handle2);

    // Task 3: Check game metadata multiple times
    let recorder_clone3 = recorder.clone();
    let handle3 = task::spawn(async move {
        for _ in 0..10 {
            let _game = recorder_clone3.get_current_game().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    handles.push(handle3);

    // Wait for all tasks to complete
    for handle in handles {
        let result = handle.await;
        assert!(
            result.is_ok(),
            "Concurrent access task should complete successfully"
        );
    }

    // Final status check
    let status = recorder.get_status().await;
    assert_eq!(status, RecordingStatus::Idle);
}

#[tokio::test]
async fn save_event_clip_while_idle_errors_and_writes_no_file() {
    // The auto-clip pipeline treats `Ok(path)` as proof that a playable clip exists and
    // writes a metadata row for it, so a save attempted while nothing is recording must
    // fail loudly and leave nothing behind.
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let recorder = WindowsCaptureRecorder::new(config).await.unwrap();
    assert_eq!(recorder.get_status().await, RecordingStatus::Idle);

    let event_wall_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    let result = recorder
        .save_event_clip(event_wall_secs, 10.0, 3.0, "idle_event")
        .await;

    assert!(
        result.is_err(),
        "idle recorder must not report a saved clip"
    );
    assert!(
        !temp_dir
            .path()
            .join("clips")
            .join("idle_event.mp4")
            .exists(),
        "no clip file may be created for a failed save"
    );
}

#[tokio::test]
async fn clip_extraction_runs_without_holding_the_recorder() {
    // The extraction path works off a snapshot (`ClipExtractionContext`) so the caller can
    // release the recorder lock before the coverage wait / verify / export, which together
    // can run for minutes and used to starve every status poll behind the health monitor's
    // periodic `write()`. Proof by construction: the context carries no handle back to the
    // recorder, so a save can still be driven after the recorder itself is gone.
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let ctx = {
        let recorder = SegmentRecorder::new(config).expect("segment recorder");
        recorder.extraction_context()
    };

    let output_path = temp_dir.path().join("clip.mp4");
    let error = ctx
        // Duration::ZERO skips the coverage wait; there is no footage at all, so the
        // export must bail before creating anything.
        .save_clip_anchored(&output_path, 10.0, None, Duration::ZERO)
        .await
        .expect_err("a save with no segments must fail");

    assert!(
        error.to_string().contains("세그먼트"),
        "unexpected error: {}",
        error
    );
    assert!(!output_path.exists(), "no file may be written on failure");
}

#[tokio::test]
async fn test_recording_config_defaults() {
    // Test default configuration
    let default_config = RecordingConfig::default();

    assert_eq!(default_config.fps, 60);
    assert_eq!(default_config.bitrate, 20_000_000);
    assert_eq!(default_config.resolution, (1920, 1080));
    assert!(matches!(default_config.encoder, VideoEncoder::H264));
    assert_eq!(default_config.buffer_duration_secs, 90);
    assert!(default_config.audio_config.is_some());
}

#[tokio::test]
async fn test_video_encoder_extensions() {
    // Test encoder to extension mapping
    assert_eq!(VideoEncoder::H264.to_extension(), "mp4");
    assert_eq!(VideoEncoder::H265.to_extension(), "mp4");
}

// Performance benchmarks (only run when explicitly enabled)
#[tokio::test]
#[ignore]
async fn benchmark_windows_capture_recorder_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        resolution: (1280, 720),
        fps: 60,
        ..create_test_config()
    };

    let recorder = WindowsCaptureRecorder::new(config).await.unwrap();

    println!("=== WindowsCaptureRecorder Performance Benchmark ===");

    // Benchmark status checks
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _status = recorder.get_status().await;
    }
    let status_duration = start.elapsed();
    println!(
        "1000 status checks: {:?} ({:.2}us per check)",
        status_duration,
        status_duration.as_micros() as f64 / 1000.0
    );

    // Benchmark stats checks
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _stats = recorder.get_stats().await;
    }
    let stats_duration = start.elapsed();
    println!(
        "1000 stats checks: {:?} ({:.2}us per check)",
        stats_duration,
        stats_duration.as_micros() as f64 / 1000.0
    );
}

// Stress test (only run when explicitly enabled)
#[tokio::test]
#[ignore]
async fn stress_test_windows_capture_recorder() {
    let temp_dir = TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..create_test_config()
    };

    let recorder = std::sync::Arc::new(WindowsCaptureRecorder::new(config).await.unwrap());

    println!("=== WindowsCaptureRecorder Stress Test ===");

    // Spawn many concurrent tasks
    let mut handles = vec![];

    for i in 0..10 {
        let recorder_clone = recorder.clone();
        let handle = tokio::task::spawn(async move {
            for j in 0..100 {
                let _status = recorder_clone.get_status().await;
                let _stats = recorder_clone.get_stats().await;

                if j % 10 == 0 {
                    println!("Task {}: iteration {}", i, j);
                }

                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "Stress test task should complete");
    }

    println!("Stress test completed successfully");
}
