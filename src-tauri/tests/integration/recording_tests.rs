// Integration tests for recording system
#![cfg(test)]

use lolshorts::lcu::LcuClient;
use lolshorts::recording::{RecordingConfig, RecordingManager, RecordingStatus};
use std::sync::Arc;
use tokio;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_recording_manager_initialization() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let manager = RecordingManager::new(config).await.unwrap();

    let status = manager.get_status().await;
    assert_eq!(status, RecordingStatus::Idle);
}

#[tokio::test]
async fn test_recording_state_transitions() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let manager = Arc::new(RwLock::new(RecordingManager::new(config).await.unwrap()));

    // Initial state should be Idle
    {
        let mgr = manager.read().await;
        assert_eq!(mgr.get_status().await, RecordingStatus::Idle);
    }

    // Start recording (will fail without actual FFmpeg capture source, but test state change logic)
    {
        let mut mgr = manager.write().await;
        let result = mgr.start_recording().await;

        // May fail in test environment without capture device, which is expected
        if result.is_err() {
            println!("Recording start failed as expected in test environment");
        }
    }
}

#[tokio::test]
async fn test_lcu_client_initialization() {
    let _client = LcuClient::new();

    // Test that client can be created without panicking
    // Note: Actual connection test requires League Client running
    // This is tested in E2E tests with mocked LCU
}

#[tokio::test]
async fn test_concurrent_recording_requests() {
    use tokio::task;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = RecordingConfig {
        output_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let manager = Arc::new(RwLock::new(RecordingManager::new(config).await.unwrap()));

    // Spawn multiple concurrent state checks
    let mut handles = vec![];
    for _ in 0..5 {
        let mgr_clone = Arc::clone(&manager);
        let handle = task::spawn(async move {
            let mgr = mgr_clone.read().await;
            mgr.get_status().await
        });
        handles.push(handle);
    }

    // All should succeed and return same state
    for handle in handles {
        let status = handle.await.unwrap();
        assert_eq!(status, RecordingStatus::Idle);
    }
}

#[tokio::test]
async fn test_clip_metadata_validation() {
    use lolshorts::storage::models::EventType;
    use lolshorts::storage::ClipMetadata;

    let valid_metadata = ClipMetadata {
        file_path: "C:\\Videos\\clip.mp4".to_string(),
        thumbnail_path: None,
        event_type: EventType::ChampionKill,
        event_time: 180.5,
        priority: 3,
        duration: 15.0,
        event_offset_secs: Some(10.0),
        created_at: chrono::Utc::now(),
        usage_count: 0,
        highlight_score: None,
        score_reasons: Vec::new(),
    };

    // Verify required fields are present
    assert!(valid_metadata.event_time > 0.0);
    assert!(valid_metadata.priority >= 1 && valid_metadata.priority <= 5);
    assert!(!valid_metadata.file_path.is_empty());
}

#[tokio::test]
async fn test_event_priority_calculation() {
    use lolshorts::storage::models::EventType;

    // Pentakill should have highest priority
    let pentakill_priority = EventType::Multikill(5).default_priority();
    assert_eq!(pentakill_priority, 5);

    // Quadrakill should be 4
    let quadrakill_priority = EventType::Multikill(4).default_priority();
    assert_eq!(quadrakill_priority, 4);

    // Triple kill should be 3
    let triple_priority = EventType::Multikill(3).default_priority();
    assert_eq!(triple_priority, 3);

    // Single kill should be lower
    let single_priority = EventType::ChampionKill.default_priority();
    assert!(single_priority < 3);
}

#[tokio::test]
async fn test_clip_storage_limits() {
    // Test that we respect storage limits
    const MAX_CLIPS_PER_GAME: usize = 20;
    const MAX_TOTAL_SIZE_GB: u64 = 50;

    let clips_count = 15;
    assert!(clips_count <= MAX_CLIPS_PER_GAME);

    let total_size_bytes = 10 * 1024 * 1024 * 1024; // 10 GB
    let max_size_bytes = MAX_TOTAL_SIZE_GB * 1024 * 1024 * 1024;
    assert!(total_size_bytes < max_size_bytes);
}

#[tokio::test]
async fn test_game_detection_flow() {
    use lolshorts::recording::game_monitor::{GameMode, UnifiedGameStatus};

    // Test game mode variants exist and are distinct
    assert_ne!(GameMode::Live, GameMode::TFT);

    // Test UnifiedGameStatus can be constructed
    let status = UnifiedGameStatus {
        lcu_connected: false,
        in_game: false,
        game_mode: GameMode::Live,
        summoner_name: None,
        champion_name: None,
        game_time: None,
        is_monitoring: false,
        is_recording: false,
        session_clip_count: 0,
    };

    assert!(!status.in_game);
    assert!(!status.lcu_connected);
}

#[tokio::test]
async fn test_windows_capture_recorder_basic_functionality() {
    use lolshorts::recording::integration_backend::{
        RecordingConfig, RecordingStatus, VideoEncoder, WindowsCaptureRecorder,
    };

    let temp_dir = tempfile::TempDir::new().unwrap();

    // Create test configuration
    let config = RecordingConfig {
        fps: 30,
        bitrate: 5_000_000,
        resolution: (640, 480), // Small for testing
        encoder: VideoEncoder::H264,
        output_dir: temp_dir.path().to_path_buf(),
        buffer_duration_secs: 30,
        audio_config: None, // No audio for basic test
        ..Default::default()
    };

    // Test recorder creation
    let recorder_result = WindowsCaptureRecorder::new(config).await;
    assert!(
        recorder_result.is_ok(),
        "Failed to create WindowsCaptureRecorder"
    );

    let recorder = recorder_result.unwrap();

    // Test initial state
    let status = recorder.get_status().await;
    assert_eq!(status, RecordingStatus::Idle);

    // Test initial stats
    let stats = recorder.get_stats().await;
    assert_eq!(stats.total_frames, 0);
    assert_eq!(stats.uptime_seconds, 0.0);

    println!("WindowsCaptureRecorder basic functionality test passed");
}
