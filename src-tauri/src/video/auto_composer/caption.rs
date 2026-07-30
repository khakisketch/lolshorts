//! 훅 자막 — 각 클립 앞머리에 "왜 이 장면인지" 한 줄.
//!
//! # 왜 필요한가
//!
//! 지금까지 산출물은 세로로 자른 게임 화면 그 자체였다. 그건 쇼츠가 아니라
//! **세로 상자에 든 클립**이다. 경쟁 서비스가 다 넣는 자막·훅 문구·킬 카운터가
//! 하나도 없었고, 쇼츠는 앞 3초에 볼 이유를 못 주면 넘어간다.
//!
//! # 왜 우리가 유리한가
//!
//! 화면 픽셀을 읽어 하이라이트를 추정하는 경쟁 서비스는 "체력 8% 였다"를 확언할
//! 수 없다. 우리는 Live Client Data API 로 그 순간의 체력·생존 인원·어시스트 수를
//! 직접 받아 `score_reasons` 에 담아 둔다 — 그런데 그 값이 저장만 되고
//! **영상에도 화면에도 한 번도 나오지 않았다**. 여기서 그 값을 픽셀로 만든다.
//!
//! # 언어
//!
//! 한국어 고정이다. 이 앱의 Rust 쪽 사용자 문구(자동 편집 진행 메시지 등)가 이미
//! 전부 한국어이고, 무엇보다 **자막은 픽셀로 구워진다** — 나중에 언어를 바꿔도
//! 이미 만든 영상은 안 바뀌므로 프론트 i18n 이 SSOT 를 가질 수 없는 종류의
//! 문자열이다. 영어 사용자를 받게 되면 여기 표를 로케일별로 나눈다.

use super::super::ClipInfo;
use crate::recording::highlight_score::ScoreReason;
use crate::video::processor::types::CaptionSpec;

/// 자막이 보이는 시간(초).
///
/// 3초를 넘기면 다음 하이라이트를 가리고, 2초 아래면 한글 한 줄을 읽기 어렵다.
/// 클립 자체가 이보다 짧으면 호출부에서 클립 길이로 줄인다.
const CAPTION_SECS: f64 = 2.6;

/// 클립 하나의 훅 자막. 무슨 장면인지 모르면 `None` — 자막 자리에 코드값을
/// 흘리느니 자막을 안 넣는다.
pub(super) fn clip_caption(clip: &ClipInfo) -> Option<CaptionSpec> {
    let title = event_title(&clip.event_type)?;

    let detail = reason_phrases(&clip.score_reasons);
    let detail = if detail.is_empty() {
        None
    } else {
        Some(detail.join(" · "))
    };

    // 클립이 자막보다 짧으면 자막이 다음 클립까지 넘어가 보이는 것처럼 느껴진다.
    let duration_secs = clip
        .duration
        .filter(|d| *d > 0.0)
        .map(|d| CAPTION_SECS.min(d * 0.6))
        .unwrap_or(CAPTION_SECS);

    Some(CaptionSpec {
        title: title.to_string(),
        detail,
        duration_secs,
    })
}

/// 저장된 이벤트 이름 → 자막 제목.
///
/// 입력은 `load_clips_from_games` 가 `EventType` 에서 만든 문자열이다. 단순 변형은
/// 이름 그대로 오고, 나머지는 `EventType::Custom` 에 실려 온 트리거 이름이다
/// (`trigger_to_event_type`). 표에 없는 이름은 `None` — 화면에 `Shutdown` 같은
/// 코드값이 클립 이름으로 나가던 결함을 자막에서 반복하지 않는다.
///
/// 이 표가 백엔드가 만들어 낼 수 있는 이름을 전부 덮는지는
/// `covers_every_event_type_the_backend_can_produce` 가 지킨다.
fn event_title(event_type: &str) -> Option<&'static str> {
    let title = match event_type {
        "PentaKill" => "펜타킬",
        "QuadraKill" => "쿼드라킬",
        "TripleKill" => "트리플킬",
        "DoubleKill" => "더블킬",
        "ChampionKill" => "킬",
        "Shutdown" => "셧다운",
        "FirstBlood" => "퍼스트블러드",
        "FirstBloodVictim" => "퍼블 당함",
        "TradeKill" => "맞교환",
        "LowHpOutplay" => "아슬아슬 생존",
        "Death" => "죽는 장면",
        "Assist" => "어시스트",
        "Steal" => "스틸",
        "BaronKill" => "바론",
        "DragonKill" => "드래곤",
        "ElderDragonKill" => "장로 드래곤",
        "HeraldKill" => "전령",
        "VoidgrubsKill" => "공허 유충",
        "AtakhanKill" => "아타칸",
        "TurretKill" => "포탑",
        "InhibitorKill" => "억제기",
        "Ace" => "에이스",
        "GameEnd" => "게임 끝",
        "ManualReplay" | "ManualSave" => "직접 저장",
        // 1vX 아웃플레이는 인원수가 이름에 박혀서 온다(`Outplay1v3`). 숫자별로
        // 표를 늘리는 대신 접두사로 받는다 — 정적 문자열을 돌려주는 표라
        // 흔한 인원수만 나열한다(감지가 만드는 범위는 1v2~1v5).
        "Outplay1v2" => "1대2 아웃플레이",
        "Outplay1v3" => "1대3 아웃플레이",
        "Outplay1v4" => "1대4 아웃플레이",
        "Outplay1v5" => "1대5 아웃플레이",
        // `Multikill(n)` 의 n>=6 은 감지가 만들지 않지만 열거형이 막지 않는다.
        other if other.starts_with("Multikill") => "연속 킬",
        other if other.starts_with("Outplay1v") => "아웃플레이",
        _ => return None,
    };
    Some(title)
}

/// 점수 이유 → 자막 둘째 줄. 눈에 띄는 것부터, 최대 세 개.
///
/// 순서는 화면 카드(`src/lib/scoreReason.ts`)와 같은 규칙이다 — 같은 클립을 보고
/// 앱과 영상이 다른 순서로 말하면 그 자체가 결함처럼 읽힌다.
fn reason_phrases(reasons: &[ScoreReason]) -> Vec<String> {
    let mut ranked: Vec<(u8, String)> = reasons
        .iter()
        .map(|reason| match reason {
            ScoreReason::Clutch(pct) => (0, format!("체력 {}%", pct)),
            ScoreReason::Outnumbered(allies, enemies) => {
                (1, format!("{}대{}", allies, enemies))
            }
            ScoreReason::Solo => (2, "혼자서".to_string()),
            ScoreReason::MatchPoint => (3, "승부처".to_string()),
            ScoreReason::LateGame => (4, "후반전".to_string()),
        })
        .collect();

    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.truncate(3);
    ranked.into_iter().map(|(_, text)| text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(event_type: &str, reasons: Vec<ScoreReason>) -> ClipInfo {
        ClipInfo {
            id: 1,
            game_id: "g".to_string(),
            event_type: event_type.to_string(),
            event_time: 600.0,
            priority: 3,
            file_path: "c.mp4".to_string(),
            thumbnail_path: None,
            duration: Some(13.0),
            usage_count: 0,
            highlight_score: None,
            event_offset_secs: Some(10.0),
            score_reasons: reasons,
        }
    }

    #[test]
    fn the_hook_says_what_happened_and_why_it_is_worth_watching() {
        let caption = clip_caption(&clip(
            "PentaKill",
            vec![ScoreReason::LateGame, ScoreReason::Clutch(8), ScoreReason::Solo],
        ))
        .expect("자막이 나와야 한다");

        assert_eq!(caption.title, "펜타킬");
        // 눈에 띄는 것부터 — 화면 카드와 같은 순서.
        assert_eq!(caption.detail.as_deref(), Some("체력 8% · 혼자서 · 후반전"));
    }

    #[test]
    fn a_clip_with_no_reasons_still_gets_its_title() {
        // 상황을 못 찍은 클립(게임 상태 캐시가 비어 있었던 경우)도 무슨 장면인지는 안다.
        let caption = clip_caption(&clip("Shutdown", vec![])).unwrap();
        assert_eq!(caption.title, "셧다운");
        assert_eq!(caption.detail, None);
    }

    #[test]
    fn an_unknown_event_name_gets_no_caption_rather_than_a_code_value() {
        // 화면에 `Shutdown` 같은 영어 코드값이 클립 이름으로 나가던 결함을
        // 자막에서 반복하지 않는다.
        assert!(clip_caption(&clip("SomethingNew", vec![])).is_none());
    }

    #[test]
    fn the_caption_never_outlives_a_short_clip() {
        let mut short = clip("ChampionKill", vec![]);
        short.duration = Some(3.0);
        let caption = clip_caption(&short).unwrap();
        assert!(
            caption.duration_secs <= 1.81,
            "3초 클립에 {:.2}초 자막",
            caption.duration_secs
        );

        // 길이를 모르는 클립은 기본값 그대로.
        let mut unknown = clip("ChampionKill", vec![]);
        unknown.duration = None;
        assert_eq!(clip_caption(&unknown).unwrap().duration_secs, CAPTION_SECS);
    }

    /// 백엔드가 **실제로 만들어 낼 수 있는** 이름을 전부 덮는가.
    ///
    /// 표에 없는 이름은 자막이 통째로 빠지고, 그건 조용하다 — 영상은 정상이고
    /// 길이도 맞고 게이트도 초록인데 그 클립만 자막이 없다. `trigger_to_event_type`
    /// 이 변형을 늘렸는데 여기 표가 안 따라오는 순간을 이 테스트가 잡는다.
    #[test]
    fn covers_every_event_type_the_backend_can_produce() {
        use crate::recording::live_client::EventTrigger;
        use crate::storage::models::EventType;

        // `detect_trigger` 가 만들 수 있는 모든 변형(멀티킬·아웃플레이는 실제 범위).
        let triggers = [
            EventTrigger::ChampionKill,
            EventTrigger::Death,
            EventTrigger::Assist,
            EventTrigger::FirstBlood,
            EventTrigger::FirstBloodVictim,
            EventTrigger::Multikill(2),
            EventTrigger::Multikill(3),
            EventTrigger::Multikill(4),
            EventTrigger::Multikill(5),
            EventTrigger::DragonKill,
            EventTrigger::BaronKill,
            EventTrigger::HeraldKill,
            EventTrigger::TurretKill,
            EventTrigger::InhibitorKill,
            EventTrigger::Ace,
            EventTrigger::Steal,
            EventTrigger::GameEnd,
            EventTrigger::ElderDragonKill,
            EventTrigger::VoidgrubsKill,
            EventTrigger::AtakhanKill,
            EventTrigger::Shutdown,
            EventTrigger::Outplay1vX(2),
            EventTrigger::Outplay1vX(3),
            EventTrigger::Outplay1vX(4),
            EventTrigger::Outplay1vX(5),
            EventTrigger::TradeKill,
            EventTrigger::LowHpOutplay,
        ];

        // `load_clips_from_games` 가 `EventType` -> 문자열로 옮기는 규칙과 같아야 한다.
        let as_clip_info_string = |event_type: EventType| -> String {
            match event_type {
                EventType::ChampionKill => "ChampionKill".to_string(),
                EventType::Multikill(2) => "DoubleKill".to_string(),
                EventType::Multikill(3) => "TripleKill".to_string(),
                EventType::Multikill(4) => "QuadraKill".to_string(),
                EventType::Multikill(5) => "PentaKill".to_string(),
                EventType::Multikill(n) => format!("Multikill({})", n),
                EventType::TurretKill => "TurretKill".to_string(),
                EventType::InhibitorKill => "InhibitorKill".to_string(),
                EventType::DragonKill => "DragonKill".to_string(),
                EventType::BaronKill => "BaronKill".to_string(),
                EventType::Ace => "Ace".to_string(),
                EventType::FirstBlood => "FirstBlood".to_string(),
                EventType::Custom(s) => s,
            }
        };

        let missing: Vec<String> = triggers
            .iter()
            .map(|t| as_clip_info_string(crate::recording::auto_clip_manager::trigger_to_event_type(t)))
            .filter(|name| event_title(name).is_none())
            .collect();

        assert!(missing.is_empty(), "자막 표에 없는 이벤트: {:?}", missing);
    }
}
