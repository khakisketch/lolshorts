#![allow(dead_code)]

pub mod caption;
pub mod composer;
pub mod processing;
pub mod types;

pub use composer::AutoComposer;
pub use types::{
    AudioLevels, AutoEditConfig, AutoEditFramingMode, AutoEditJobReceipt, AutoEditOutput,
    AutoEditOutputIntent, AutoEditOutputKind, AutoEditPlan, AutoEditPlanClip, AutoEditProgress,
    AutoEditResult, AutoEditStatus, BackgroundLayer, BackgroundMusic, CanvasElement,
    CanvasTemplate, PlatformPreset, Position, PublishMetadata, StoryboardClip,
};

#[cfg(test)]
mod tests {
    use super::super::{ClipInfo, VideoProcessor};
    use super::caption::CaptionLocale;
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
            // 이 헬퍼의 테스트들은 `priority` 로 순서를 검증한다. 점수를 비워 두면
            // 선택기가 priority 폴백 경로를 타므로 그 검증이 그대로 유효하다.
            highlight_score: None,
            event_offset_secs: None,
            score_reasons: Vec::new(),
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
            selected_clip_paths: None,
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
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
            selected_clip_paths: None,
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
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
            selected_clip_paths: None,
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };

        let selected = composer.select_clips(&clips, &config).await.unwrap();

        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|c| c.id == 1));
        assert!(selected.iter().any(|c| c.id == 3));
    }

    /// 화면에서 고른 선택은 **경로**로 온다.
    ///
    /// `selected_clip_ids` 의 `id` 는 로딩 순서로 매기는 위치 카운터라 프론트가
    /// 지목할 수 없다. 홈의 체크박스가 오랫동안 아무 일도 하지 않던 이유다.
    #[tokio::test]
    async fn clips_picked_on_screen_are_matched_by_path() {
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
            selected_clip_ids: None,
            selected_clip_paths: Some(vec![
                "/tmp/clip_1.mp4".to_string(),
                "/tmp/clip_3.mp4".to_string(),
            ]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };

        let selected = composer.select_clips(&clips, &config).await.unwrap();

        assert_eq!(selected.len(), 2);
        // 점수순 — 홈이 1위에 「최고의 순간」을 달아 둔 그 클립이 첫 장면이다.
        assert_eq!(selected[0].id, 3);
        assert_eq!(selected[1].id, 1);
    }

    /// 고른 것은 목표 길이로 자르지 않는다.
    ///
    /// 자동 선택일 때만 길이가 예산이다. 직접 고른 클립을 말없이 빼면 "왜 내가
    /// 고른 게 안 들어갔지" 가 된다.
    #[tokio::test]
    async fn picked_clips_are_not_trimmed_to_the_target_length() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let clips = vec![
            create_test_clip(1, 5, 40.0, "Pentakill"),
            create_test_clip(2, 4, 40.0, "Quadrakill"),
        ];

        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: None,
            selected_clip_paths: Some(vec![
                "/tmp/clip_1.mp4".to_string(),
                "/tmp/clip_2.mp4".to_string(),
            ]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };

        let selected = composer.select_clips(&clips, &config).await.unwrap();

        // 자동 선택이었으면 80초 > 54초 예산이라 하나만 남았을 것이다.
        assert_eq!(selected.len(), 2);
    }

    /// 경로가 하나도 안 맞으면 조용히 자동 선택으로 흘러가지 않는다.
    ///
    /// 파일이 지워졌거나 옮겨진 경우다. 자동 선택으로 떨어지면 사용자는 고른 적
    /// 없는 클립으로 만들어진 영상을 받는다 — 실패가 낫다.
    #[tokio::test]
    async fn unmatched_paths_fail_instead_of_falling_back_to_auto() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let clips = vec![create_test_clip(1, 5, 10.0, "Pentakill")];

        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: None,
            selected_clip_paths: Some(vec!["/tmp/gone.mp4".to_string()]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };

        assert!(composer.select_clips(&clips, &config).await.is_err());
    }

    #[tokio::test]
    async fn multi_game_picks_all_survive_and_sort_globally() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let mut first_game = create_test_clip(1, 2, 12.0, "Kill");
        first_game.game_id = "game1".to_string();
        let mut second_game = create_test_clip(2, 5, 15.0, "Pentakill");
        second_game.game_id = "game2".to_string();

        let config = AutoEditConfig {
            target_duration: 10,
            game_ids: vec!["game1".to_string(), "game2".to_string()],
            selected_clip_ids: None,
            selected_clip_paths: Some(vec![
                first_game.file_path.clone(),
                second_game.file_path.clone(),
            ]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };

        let selected = composer
            .select_clips(&[first_game, second_game], &config)
            .await
            .unwrap();
        assert_eq!(
            selected.len(),
            2,
            "target duration must not drop manual picks"
        );
        assert_eq!(selected[0].game_id, "game2", "best score leads globally");
        assert_eq!(selected[1].game_id, "game1");
    }

    #[tokio::test]
    async fn a_partially_missing_manual_selection_fails() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);
        let clips = vec![create_test_clip(1, 5, 10.0, "Pentakill")];
        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: None,
            selected_clip_paths: Some(vec![
                "/tmp/clip_1.mp4".to_string(),
                "/tmp/missing.mp4".to_string(),
            ]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };
        assert!(composer.select_clips(&clips, &config).await.is_err());
    }

    #[tokio::test]
    async fn storyboard_preserves_reviewed_cross_game_order_and_exact_trims() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);
        let mut first = create_test_clip(1, 5, 30.0, "Pentakill");
        first.game_id = "game1".to_string();
        let mut second = create_test_clip(2, 1, 20.0, "Kill");
        second.game_id = "game2".to_string();

        let config = AutoEditConfig {
            game_ids: vec!["game1".to_string(), "game2".to_string()],
            target_duration: 10,
            storyboard: Some(vec![
                StoryboardClip {
                    game_id: "game2".to_string(),
                    file_path: second.file_path.clone(),
                    order: 0,
                    trim_start_secs: 3.0,
                    trim_end_secs: 9.5,
                },
                StoryboardClip {
                    game_id: "game1".to_string(),
                    file_path: first.file_path.clone(),
                    order: 1,
                    trim_start_secs: 2.0,
                    trim_end_secs: 18.0,
                },
            ]),
            ..Default::default()
        };

        let (selected, timeline) = composer
            .resolve_timeline(&[first, second], &config)
            .await
            .unwrap();

        assert_eq!(
            selected.iter().map(|clip| clip.id).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(timeline[0].trim_start, Some(3.0));
        assert_eq!(timeline[0].trim_duration, Some(6.5));
        assert_eq!(timeline[1].trim_start, Some(2.0));
        assert_eq!(timeline[1].trim_duration, Some(16.0));
    }

    #[tokio::test]
    async fn storyboard_and_legacy_path_selection_are_mutually_exclusive() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);
        let clip = create_test_clip(1, 5, 30.0, "Pentakill");
        let config = AutoEditConfig {
            game_ids: vec!["test_game".to_string()],
            selected_clip_paths: Some(vec![clip.file_path.clone()]),
            storyboard: Some(vec![StoryboardClip {
                game_id: clip.game_id.clone(),
                file_path: clip.file_path.clone(),
                order: 0,
                trim_start_secs: 0.0,
                trim_end_secs: 10.0,
            }]),
            ..Default::default()
        };

        assert!(composer.resolve_timeline(&[clip], &config).await.is_err());
    }

    /// 여러 번 쓴 클립이라도 **직접 고르면** 감쇠로 순서가 뒤집히지 않는다.
    ///
    /// 재사용 감쇠는 "다음에 무엇을 쓸까" 를 정하는 자동 선택의 관심사다.
    /// 사용자가 직접 고른 목록에까지 적용되면, 이미 한 번 쓴 펜타킬이 처음 쓰는
    /// 평범한 킬 뒤로 밀려 훅이 무너진다.
    #[tokio::test]
    async fn picking_by_hand_ignores_the_reuse_decay() {
        let processor = Arc::new(VideoProcessor::new_with_fallback());
        let storage = create_test_storage();
        let composer = AutoComposer::new(processor, storage);

        let mut penta = create_test_clip(1, 5, 10.0, "Pentakill");
        penta.usage_count = 4; // 자동 선택이었으면 100 * 0.6^4 = 12.96 으로 밀린다
        let kill = create_test_clip(2, 2, 10.0, "Kill"); // 40

        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game1".to_string()],
            selected_clip_ids: None,
            selected_clip_paths: Some(vec![
                "/tmp/clip_1.mp4".to_string(),
                "/tmp/clip_2.mp4".to_string(),
            ]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
            enable_hook_captions: false,
            caption_locale: CaptionLocale::default(),
            ..Default::default()
        };

        let selected = composer
            .select_clips(&[penta, kill], &config)
            .await
            .unwrap();

        assert_eq!(selected[0].id, 1);
    }

    /// 줌(과 훅 자막)이 걸리는 시각이 **하이라이트 위**에 오는가.
    ///
    /// 예전에는 구간 중앙을 썼다. 클립마다 pre/post 가 다른데 하나의 규칙으로
    /// 뭉갠 것이라, 게임 종료 클립(pre 30 / post 10)에서는 줌이 승리 순간이
    /// 아니라 그 20초 전 아무 일 없는 지점에서 걸렸다.
    #[test]
    fn event_timeline_lands_on_the_highlight_not_the_middle() {
        use crate::video::processor::types::ClipSpec;
        use std::path::PathBuf;

        let mut kill = create_test_clip(1, 1, 13.0, "Kill");
        kill.event_offset_secs = Some(10.0); // pre 10 / post 3
        let mut game_end = create_test_clip(2, 3, 40.0, "GameEnd");
        game_end.event_offset_secs = Some(30.0); // pre 30 / post 10

        let specs = vec![
            ClipSpec {
                path: PathBuf::from("a.mp4"),
                trim_start: None,
                trim_duration: None,
            },
            ClipSpec {
                path: PathBuf::from("b.mp4"),
                trim_start: Some(21.0),
                trim_duration: Some(12.0),
            },
        ];

        let times = AutoComposer::event_timeline(&specs, &[kill, game_end]);

        // 첫 클립은 트림 없음 -> 그대로 10초.
        assert!((times[0] - 10.0).abs() < 0.01, "{:?}", times);
        // 둘째는 13초(첫 클립 길이) + (30 - 21) = 22초. 중앙 규칙이면 19초였다.
        assert!((times[1] - 22.0).abs() < 0.01, "{:?}", times);
    }

    #[test]
    fn event_timeline_falls_back_to_the_middle_for_older_clips() {
        use crate::video::processor::types::ClipSpec;
        use std::path::PathBuf;

        // `event_offset_secs` 가 없는 예전 클립 — 예전 동작(구간 중앙)을 유지한다.
        let clip = create_test_clip(1, 1, 12.0, "Kill");
        let specs = vec![ClipSpec {
            path: PathBuf::from("a.mp4"),
            trim_start: None,
            trim_duration: None,
        }];

        assert_eq!(AutoComposer::event_timeline(&specs, &[clip]), vec![6.0]);
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
