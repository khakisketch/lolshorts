#![allow(dead_code)]

pub mod composer;
pub mod processing;
pub mod types;

pub use composer::AutoComposer;
pub use types::{
    AudioLevels, AutoEditConfig, AutoEditProgress, AutoEditResult, AutoEditStatus, BackgroundLayer,
    BackgroundMusic, CanvasElement, CanvasTemplate, Position,
};

#[cfg(test)]
mod tests {
    use super::super::{ClipInfo, VideoProcessor};
    use super::*;
    use crate::storage::Storage;
    use std::sync::Arc;

    fn create_test_storage() -> Arc<Storage> {
        let temp_dir = std::env::temp_dir().join(format!(
            "lolshorts_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        Arc::new(Storage::new(&temp_dir).expect("테스트 저장소 생성 실패"))
    }

    fn create_test_clip(id: i64, priority: i32, duration: f64, event_type: &str) -> ClipInfo {
        ClipInfo {
            id,
            game_id: "test_game".to_string(),
            event_type: event_type.to_string(),
            event_time: 100.0,
            priority,
            file_path: format!("/tmp/clip_{}.mp4", id),
            thumbnail_path: None,
            duration: Some(duration),
            usage_count: 0,
        }
    }

    #[tokio::test]
    async fn test_clip_selection_by_priority() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let clips = vec![
            create_test_clip(1, 1, 10.0, "Kill"),
            create_test_clip(2, 3, 15.0, "Triple Kill"),
            create_test_clip(3, 5, 12.0, "Pentakill"),
            create_test_clip(4, 2, 8.0, "Double Kill"),
            create_test_clip(5, 4, 10.0, "Quadrakill"),
        ];

        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: None,
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
        };

        let selected = composer.select_clips(&clips, &config).await.unwrap();

        assert!(!selected.is_empty());
        assert_eq!(selected[0].priority, 5);
        assert!(selected.iter().all(|c| c.priority >= 2));

        let total_duration: f64 = selected.iter().map(|c| c.duration.unwrap()).sum();
        assert!(total_duration <= 54.0);
    }

    #[tokio::test]
    async fn test_clip_selection_fits_duration() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let clips = vec![
            create_test_clip(1, 5, 20.0, "Pentakill"),
            create_test_clip(2, 4, 25.0, "Quadrakill"),
            create_test_clip(3, 3, 30.0, "Triple Kill"),
        ];

        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: None,
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
        };

        let selected = composer.select_clips(&clips, &config).await.unwrap();

        let total_duration: f64 = selected.iter().map(|c| c.duration.unwrap()).sum();
        assert!(total_duration <= 54.0);
        assert_eq!(selected.len(), 2);
    }

    #[tokio::test]
    async fn test_manual_clip_selection() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let clips = vec![
            create_test_clip(1, 1, 10.0, "Kill"),
            create_test_clip(2, 3, 15.0, "Triple Kill"),
            create_test_clip(3, 5, 12.0, "Pentakill"),
        ];

        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: Some(vec![1, 3]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
        };

        let selected = composer.select_clips(&clips, &config).await.unwrap();

        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|c| c.id == 1));
        assert!(selected.iter().any(|c| c.id == 3));
    }

    #[test]
    fn test_audio_levels_default() {
        let levels = AudioLevels::default();
        assert_eq!(levels.game_audio, 60);
        assert_eq!(levels.background_music, 80);
    }

    #[test]
    fn test_canvas_element_serialization() {
        let text_element = CanvasElement::Text {
            id: "title".to_string(),
            content: "PENTAKILL!".to_string(),
            font: "Bebas Neue".to_string(),
            size: 48,
            color: "#FFD700".to_string(),
            outline: Some("#000000".to_string()),
            position: Position { x: 50.0, y: 10.0 },
        };

        let json = serde_json::to_string(&text_element).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("PENTAKILL"));
    }
}
