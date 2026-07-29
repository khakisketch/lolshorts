#![allow(dead_code)]
use serde::{Deserialize, Serialize};

use super::super::ClipInfo;

/// 자동 편집(Auto-Edit) 구성 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEditConfig {
    /// 목표 길이 (초 단위: 60, 120, 180 등)
    pub target_duration: u32,

    /// 클립을 가져올 게임 ID 목록
    pub game_ids: Vec<String>,

    /// 수동으로 선택된 클립 ID 목록 (자동 선택 무시)
    pub selected_clip_ids: Option<Vec<i64>>,

    /// 캔버스 템플릿 설정
    pub canvas_template: Option<CanvasTemplate>,

    /// 배경 음악 설정
    pub background_music: Option<BackgroundMusic>,

    /// 오디오 믹싱 레벨 설정
    ///
    /// 배경 음악이 없어도 게임 오디오 볼륨을 결정하므로 항상 의미가 있다.
    /// `#[serde(default)]` 가 없으면 프론트가 이 필드를 생략했을 때
    /// `missing field audio_levels` 로 auto-edit 이 통째로 실패한다.
    #[serde(default)]
    pub audio_levels: AudioLevels,

    /// 중복 클립 사용 허용 여부 (기본값: false)
    #[serde(default)]
    pub allow_duplicates: bool,

    /// 이벤트(킬 등) 시점 자동 줌인 효과 활성화 여부 (기본값: false)
    ///
    /// true면 선택된 클립들의 누적 타임라인 오프셋으로 event_times를 계산해
    /// compose_with_options 로 전달, 소스 fps 60 기준 zoompan 을 적용한다.
    #[serde(default)]
    pub enable_event_zoom: bool,
}

/// 오버레이용 캔버스 템플릿
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasTemplate {
    pub id: String,
    pub name: String,
    pub background: BackgroundLayer,
    pub elements: Vec<CanvasElement>,
}

// NOTE: the frontend (src/types/autoEdit.ts, src/components/editor/canvas/
// CanvasControlsPanel.tsx, CanvasEditor.tsx) sends the `type` tag in
// PascalCase ("Color"/"Gradient"/"Image"/"Text"), while these enums
// serialize it in lowercase via `rename_all = "lowercase"`. These structs
// are deserialized directly from Tauri command JSON args (see
// `save_canvas_template` / `AutoEditConfig::canvas_template`), so a tag
// case mismatch is a hard deserialization error, not a silently-skipped
// branch. The `alias` below accepts the frontend's PascalCase tag in
// addition to the canonical lowercase one; serialization is unaffected
// (still emits lowercase, matching the tests below).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackgroundLayer {
    #[serde(alias = "Color")]
    Color { value: String },
    #[serde(alias = "Gradient")]
    Gradient { value: String },
    #[serde(alias = "Image")]
    Image { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CanvasElement {
    #[serde(alias = "Text")]
    Text {
        // `#[serde(default)]`: the frontend's CanvasElement type
        // (src/types/autoEdit.ts) never sends an `id` field when
        // constructing new elements (CanvasControlsPanel.tsx
        // addTextElement/addImageElement) — `id` is unused by any Rust
        // processing logic (grep confirms only `.id` reads elsewhere are
        // unrelated ClipInfo/job ids), so defaulting to an empty string
        // keeps deserialization from hard-failing on a frontend-omitted
        // field instead of silently killing the whole canvas template.
        #[serde(default)]
        id: String,
        content: String,
        font: String,
        size: u32,
        color: String,
        outline: Option<String>,
        position: Position,
    },
    #[serde(alias = "Image")]
    Image {
        #[serde(default)]
        id: String,
        path: String,
        width: u32,
        height: u32,
        position: Position,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// X 위치 (백분율 0-100)
    pub x: f32,
    /// Y 위치 (백분율 0-100)
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundMusic {
    /// MP3 파일 경로
    pub file_path: String,
    /// 영상보다 짧을 경우 반복 재생 여부
    pub loop_music: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioLevels {
    /// 게임 오디오 볼륨 (0-100)
    pub game_audio: u32,
    /// 배경 음악 볼륨 (0-100)
    pub background_music: u32,
}

impl Default for AudioLevels {
    fn default() -> Self {
        Self {
            game_audio: 60,
            background_music: 80,
        }
    }
}

/// 자동 편집 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEditResult {
    /// 최종 합성된 영상 경로
    pub output_path: String,

    /// 사용된 클립 목록
    pub selected_clips: Vec<ClipInfo>,

    /// 최종 영상 길이
    pub total_duration: f64,

    /// 사용된 클립 개수
    pub clip_count: usize,
}

/// 자동 편집 진행 상황 추적
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEditProgress {
    /// 고유 작업 ID
    pub job_id: String,

    /// 현재 상태
    pub status: AutoEditStatus,

    /// 진행률 (0-100)
    pub progress: f64,

    /// 현재 단계 설명
    pub current_step: String,

    /// 경과 시간 (초)
    pub elapsed_seconds: f64,

    /// 예상 소요 시간 (초)
    pub estimated_seconds: f64,

    /// 출력 경로 (완료 시 제공)
    pub output_path: Option<String>,

    /// 오류 메시지 (실패 시 제공)
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AutoEditStatus {
    Queued,
    // Granular processing stages. These serialize in PascalCase (not the
    // enum-wide lowercase) so that the frontend stepper's pass-through switch
    // (src/api/video.ts `normalizeAutoEditStatus`) can highlight the matching
    // stage row (SelectingClips/PreparingClips/Concatenating/ApplyingCanvas/
    // MixingAudio). The generic `Processing` below is kept for backward compat.
    #[serde(rename = "SelectingClips")]
    SelectingClips,
    #[serde(rename = "PreparingClips")]
    PreparingClips,
    #[serde(rename = "Concatenating")]
    Concatenating,
    #[serde(rename = "ApplyingCanvas")]
    ApplyingCanvas,
    #[serde(rename = "MixingAudio")]
    MixingAudio,
    Processing,
    Completed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- AudioLevels ----

    #[test]
    fn audio_levels_default_game_audio_is_60() {
        let levels = AudioLevels::default();
        assert_eq!(levels.game_audio, 60);
    }

    #[test]
    fn audio_levels_default_background_music_is_80() {
        let levels = AudioLevels::default();
        assert_eq!(levels.background_music, 80);
    }

    #[test]
    fn audio_levels_can_be_constructed_with_custom_values() {
        let levels = AudioLevels {
            game_audio: 100,
            background_music: 50,
        };
        assert_eq!(levels.game_audio, 100);
        assert_eq!(levels.background_music, 50);
    }

    // ---- AutoEditConfig serialization / deserialization ----

    #[test]
    fn auto_edit_config_serializes_to_json_with_expected_fields() {
        let config = AutoEditConfig {
            target_duration: 60,
            game_ids: vec!["game-1".to_string()],
            selected_clip_ids: None,
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels::default(),
            allow_duplicates: false,
            enable_event_zoom: false,
        };

        let json = serde_json::to_string(&config).expect("serialization should succeed");
        assert!(json.contains("target_duration"));
        assert!(json.contains("game_ids"));
        assert!(json.contains("audio_levels"));
    }

    #[test]
    fn auto_edit_config_round_trips_through_json() {
        let original = AutoEditConfig {
            target_duration: 120,
            game_ids: vec!["g1".to_string(), "g2".to_string()],
            selected_clip_ids: Some(vec![1, 2, 3]),
            canvas_template: None,
            background_music: None,
            audio_levels: AudioLevels {
                game_audio: 70,
                background_music: 40,
            },
            allow_duplicates: true,
            enable_event_zoom: true,
        };

        let json = serde_json::to_string(&original).expect("serialization should succeed");
        let restored: AutoEditConfig =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(restored.target_duration, 120);
        assert_eq!(restored.game_ids, vec!["g1", "g2"]);
        assert_eq!(restored.selected_clip_ids, Some(vec![1, 2, 3]));
        assert_eq!(restored.audio_levels.game_audio, 70);
        assert_eq!(restored.audio_levels.background_music, 40);
        assert!(restored.allow_duplicates);
        assert!(restored.enable_event_zoom);
    }

    #[test]
    fn auto_edit_config_enable_event_zoom_defaults_to_false_when_missing_from_json() {
        // The field has #[serde(default)], so omitting it should deserialize as false.
        let json = r#"{
            "target_duration": 60,
            "game_ids": [],
            "selected_clip_ids": null,
            "canvas_template": null,
            "background_music": null,
            "audio_levels": { "game_audio": 60, "background_music": 80 }
        }"#;

        let config: AutoEditConfig =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(!config.enable_event_zoom);
    }

    #[test]
    fn auto_edit_config_allow_duplicates_defaults_to_false_when_missing_from_json() {
        // The field has #[serde(default)], so omitting it should deserialize as false.
        let json = r#"{
            "target_duration": 60,
            "game_ids": [],
            "selected_clip_ids": null,
            "canvas_template": null,
            "background_music": null,
            "audio_levels": { "game_audio": 60, "background_music": 80 }
        }"#;

        let config: AutoEditConfig =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(!config.allow_duplicates);
    }

    // ---- BackgroundLayer serialization ----

    #[test]
    fn background_layer_color_serializes_with_type_tag() {
        let layer = BackgroundLayer::Color {
            value: "#000000".to_string(),
        };
        let json = serde_json::to_string(&layer).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"color\""), "json: {}", json);
        assert!(json.contains("#000000"));
    }

    #[test]
    fn background_layer_gradient_serializes_with_type_tag() {
        let layer = BackgroundLayer::Gradient {
            value: "linear-gradient(...)".to_string(),
        };
        let json = serde_json::to_string(&layer).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"gradient\""), "json: {}", json);
    }

    #[test]
    fn background_layer_image_serializes_with_type_tag() {
        let layer = BackgroundLayer::Image {
            path: "/path/to/img.png".to_string(),
        };
        let json = serde_json::to_string(&layer).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"image\""), "json: {}", json);
        assert!(json.contains("/path/to/img.png"));
    }

    #[test]
    fn background_layer_round_trips_through_json() {
        let original = BackgroundLayer::Color {
            value: "#ffffff".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: BackgroundLayer = serde_json::from_str(&json).unwrap();
        if let BackgroundLayer::Color { value } = restored {
            assert_eq!(value, "#ffffff");
        } else {
            panic!("expected BackgroundLayer::Color");
        }
    }

    // ---- BackgroundLayer accepts the frontend's PascalCase `type` tag ----
    // (src/types/autoEdit.ts / CanvasControlsPanel.tsx send "Color"/
    // "Gradient"/"Image", not the canonical lowercase serialized form.)

    #[test]
    fn background_layer_deserializes_frontend_pascal_case_color_tag() {
        let json = r##"{"type":"Color","value":"#112233"}"##;
        let layer: BackgroundLayer =
            serde_json::from_str(json).expect("should accept PascalCase tag");
        match layer {
            BackgroundLayer::Color { value } => assert_eq!(value, "#112233"),
            _ => panic!("expected BackgroundLayer::Color"),
        }
    }

    #[test]
    fn background_layer_deserializes_frontend_pascal_case_gradient_tag() {
        let json = r##"{"type":"Gradient","value":"#000:#fff"}"##;
        let layer: BackgroundLayer =
            serde_json::from_str(json).expect("should accept PascalCase tag");
        assert!(matches!(layer, BackgroundLayer::Gradient { .. }));
    }

    #[test]
    fn background_layer_deserializes_frontend_pascal_case_image_tag() {
        let json = r#"{"type":"Image","path":"/bg.png"}"#;
        let layer: BackgroundLayer =
            serde_json::from_str(json).expect("should accept PascalCase tag");
        match layer {
            BackgroundLayer::Image { path } => assert_eq!(path, "/bg.png"),
            _ => panic!("expected BackgroundLayer::Image"),
        }
    }

    // ---- CanvasElement serialization ----

    #[test]
    fn canvas_element_text_serializes_with_type_tag() {
        let element = CanvasElement::Text {
            id: "t1".to_string(),
            content: "Hello".to_string(),
            font: "Arial".to_string(),
            size: 24,
            color: "#fff".to_string(),
            outline: None,
            position: Position { x: 50.0, y: 10.0 },
        };
        let json = serde_json::to_string(&element).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"text\""), "json: {}", json);
        assert!(json.contains("Hello"));
    }

    #[test]
    fn canvas_element_image_serializes_with_type_tag() {
        let element = CanvasElement::Image {
            id: "img1".to_string(),
            path: "/img.png".to_string(),
            width: 100,
            height: 80,
            position: Position { x: 0.0, y: 0.0 },
        };
        let json = serde_json::to_string(&element).expect("serialization should succeed");
        assert!(json.contains("\"type\":\"image\""), "json: {}", json);
    }

    // ---- CanvasElement accepts the frontend's PascalCase `type` tag and
    // its omitted `id` field (CanvasControlsPanel.tsx never sends `id`) ----

    #[test]
    fn canvas_element_deserializes_frontend_pascal_case_text_without_id() {
        let json = r##"{"type":"Text","content":"Hello","font":"Arial","size":24,"color":"#fff","outline":null,"position":{"x":50.0,"y":10.0}}"##;
        let element: CanvasElement =
            serde_json::from_str(json).expect("should accept PascalCase tag with missing id");
        match element {
            CanvasElement::Text { id, content, .. } => {
                assert_eq!(id, "");
                assert_eq!(content, "Hello");
            }
            _ => panic!("expected CanvasElement::Text"),
        }
    }

    #[test]
    fn canvas_element_deserializes_frontend_pascal_case_image_without_id() {
        let json = r#"{"type":"Image","path":"/img.png","width":100,"height":80,"position":{"x":0.0,"y":0.0}}"#;
        let element: CanvasElement =
            serde_json::from_str(json).expect("should accept PascalCase tag with missing id");
        match element {
            CanvasElement::Image { id, path, .. } => {
                assert_eq!(id, "");
                assert_eq!(path, "/img.png");
            }
            _ => panic!("expected CanvasElement::Image"),
        }
    }

    // ---- AutoEditStatus ----

    #[test]
    fn auto_edit_status_equality_same_variants() {
        assert_eq!(AutoEditStatus::Queued, AutoEditStatus::Queued);
        assert_eq!(AutoEditStatus::Completed, AutoEditStatus::Completed);
    }

    #[test]
    fn auto_edit_status_inequality_different_variants() {
        assert_ne!(AutoEditStatus::Queued, AutoEditStatus::Processing);
        assert_ne!(AutoEditStatus::Completed, AutoEditStatus::Failed);
    }

    #[test]
    fn auto_edit_status_queued_serializes_to_lowercase() {
        let json = serde_json::to_string(&AutoEditStatus::Queued).unwrap();
        assert_eq!(json, "\"queued\"");
    }

    #[test]
    fn auto_edit_status_completed_serializes_to_lowercase() {
        let json = serde_json::to_string(&AutoEditStatus::Completed).unwrap();
        assert_eq!(json, "\"completed\"");
    }

    #[test]
    fn auto_edit_status_failed_serializes_to_lowercase() {
        let json = serde_json::to_string(&AutoEditStatus::Failed).unwrap();
        assert_eq!(json, "\"failed\"");
    }

    #[test]
    fn auto_edit_status_deserializes_from_lowercase_processing() {
        let status: AutoEditStatus = serde_json::from_str("\"processing\"").unwrap();
        assert_eq!(status, AutoEditStatus::Processing);
    }

    #[test]
    fn auto_edit_status_deserializes_from_lowercase_failed() {
        let status: AutoEditStatus = serde_json::from_str("\"failed\"").unwrap();
        assert_eq!(status, AutoEditStatus::Failed);
    }

    // ---- Granular stage variants serialize in PascalCase for the frontend stepper ----

    #[test]
    fn auto_edit_status_stage_variants_serialize_in_pascal_case() {
        // These must match the pass-through cases in src/api/video.ts exactly.
        assert_eq!(
            serde_json::to_string(&AutoEditStatus::SelectingClips).unwrap(),
            "\"SelectingClips\""
        );
        assert_eq!(
            serde_json::to_string(&AutoEditStatus::PreparingClips).unwrap(),
            "\"PreparingClips\""
        );
        assert_eq!(
            serde_json::to_string(&AutoEditStatus::Concatenating).unwrap(),
            "\"Concatenating\""
        );
        assert_eq!(
            serde_json::to_string(&AutoEditStatus::ApplyingCanvas).unwrap(),
            "\"ApplyingCanvas\""
        );
        assert_eq!(
            serde_json::to_string(&AutoEditStatus::MixingAudio).unwrap(),
            "\"MixingAudio\""
        );
    }

    #[test]
    fn auto_edit_status_processing_still_serializes_to_lowercase() {
        // Backward-compat: the legacy generic status is unchanged.
        let json = serde_json::to_string(&AutoEditStatus::Processing).unwrap();
        assert_eq!(json, "\"processing\"");
    }
}
