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
//! **자막은 픽셀로 구워진다** — 나중에 사용자가 UI 언어를 바꿔도 이미 만든
//! 영상은 안 바뀐다. 그래서 프론트 i18next 를 SSOT 로 그대로 가져다 쓸 수 없고,
//! 자동 편집을 요청한 시점의 UI 언어를 `AutoEditConfig::caption_locale` 로 스냅샷
//! 해서 넘겨받는다.
//!
//! ko/en 두 로케일만 지원한다 — 앱이 20개 로케일을 UI 문구로는 지원하지만, 실제로
//! 사람이 다듬어 유지되는 것은 이 둘뿐이다(나머지는 `src/locales/*` 파일 크기가
//! ko/en 의 1/3 이하). 번역 품질이 그대로 영상에 박히는 자막을 나머지 18개까지
//! 억지로 만들면 어색한 문구가 영구히 남는다 — 지원 안 하는 로케일은 영어로
//! 떨어진다(앱의 `fallbackLng` 과 같은 규칙).

use super::super::ClipInfo;
use crate::recording::highlight_score::ScoreReason;
use crate::video::processor::types::CaptionSpec;
use serde::{Deserialize, Serialize};

/// 자막에 쓸 언어. ko/en 만 — 위 모듈 문서 참조.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptionLocale {
    Ko,
    #[default]
    En,
}

impl CaptionLocale {
    /// 프론트가 보내는 i18next 언어 코드(`"ko"`, `"ko-KR"`, `"en-US"` 등) → 자막 로케일.
    ///
    /// i18next 는 지역 서브태그가 붙은 코드를 그대로 저장하기도 한다
    /// (`localStorage` 감지 결과가 `ko-KR` 로 오는 브라우저가 있다). 앞 두 글자만
    /// 본다 — 지원하는 것이 딱 두 갈래뿐이라 그 이상 파싱할 이유가 없다.
    pub fn from_ui_language(code: &str) -> Self {
        if code.to_lowercase().starts_with("ko") {
            CaptionLocale::Ko
        } else {
            CaptionLocale::En
        }
    }
}

/// 자막이 보이는 시간(초).
///
/// 3초를 넘기면 다음 하이라이트를 가리고, 2초 아래면 한 줄을 읽기 어렵다.
/// 클립 자체가 이보다 짧으면 호출부에서 클립 길이로 줄인다.
const CAPTION_SECS: f64 = 2.6;

/// 클립 하나의 훅 자막. 무슨 장면인지 모르면 `None` — 자막 자리에 코드값을
/// 흘리느니 자막을 안 넣는다.
pub(super) fn clip_caption(clip: &ClipInfo, locale: CaptionLocale) -> Option<CaptionSpec> {
    let title = event_title(&clip.event_type, locale)?;

    // 제목이 이미 1vX 를 말하고 있으면 이유에서 수적열세를 뺀다.
    //
    // 「1대3 아웃플레이」와 「1대4」는 **서로 다른 것을 센다** — 전자는 10초 안에
    // 내가 잡은 고유 피해자 수(`recent_solo_kills`), 후자는 그 순간 살아 있던
    // 양 팀 인원(`capture_moment`). 나란히 구워 놓으면 보는 사람이 둘 중 하나를
    // 틀린 값으로 읽는다.
    //
    // 화면 쪽 같은 규칙: `src/lib/clipLabel.ts`. 한쪽만 고치면 카드와 영상이
    // 다른 말을 하게 되므로 둘을 함께 본다.
    let title_says_outplay = clip.event_type.starts_with("Outplay1v");

    let detail = reason_phrases(&clip.score_reasons, locale, title_says_outplay);
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
/// `covers_every_event_type_the_backend_can_produce` 가 지킨다(두 로케일 모두).
fn event_title(event_type: &str, locale: CaptionLocale) -> Option<&'static str> {
    let title = match (event_type, locale) {
        ("PentaKill", CaptionLocale::Ko) => "펜타킬",
        ("PentaKill", CaptionLocale::En) => "Pentakill",
        ("QuadraKill", CaptionLocale::Ko) => "쿼드라킬",
        ("QuadraKill", CaptionLocale::En) => "Quadrakill",
        ("TripleKill", CaptionLocale::Ko) => "트리플킬",
        ("TripleKill", CaptionLocale::En) => "Triple kill",
        ("DoubleKill", CaptionLocale::Ko) => "더블킬",
        ("DoubleKill", CaptionLocale::En) => "Double kill",
        ("ChampionKill", CaptionLocale::Ko) => "킬",
        ("ChampionKill", CaptionLocale::En) => "Kill",
        ("Shutdown", CaptionLocale::Ko) => "셧다운",
        ("Shutdown", CaptionLocale::En) => "Shutdown",
        ("FirstBlood", CaptionLocale::Ko) => "퍼스트블러드",
        ("FirstBlood", CaptionLocale::En) => "First blood",
        ("FirstBloodVictim", CaptionLocale::Ko) => "퍼블 당함",
        ("FirstBloodVictim", CaptionLocale::En) => "First blood (them)",
        ("TradeKill", CaptionLocale::Ko) => "맞교환",
        ("TradeKill", CaptionLocale::En) => "Trade kill",
        ("LowHpOutplay", CaptionLocale::Ko) => "아슬아슬 생존",
        ("LowHpOutplay", CaptionLocale::En) => "Clutch survival",
        ("Death", CaptionLocale::Ko) => "죽는 장면",
        ("Death", CaptionLocale::En) => "Death",
        ("Assist", CaptionLocale::Ko) => "어시스트",
        ("Assist", CaptionLocale::En) => "Assist",
        ("Steal", CaptionLocale::Ko) => "스틸",
        ("Steal", CaptionLocale::En) => "Objective steal",
        ("BaronKill", CaptionLocale::Ko) => "바론",
        ("BaronKill", CaptionLocale::En) => "Baron",
        ("DragonKill", CaptionLocale::Ko) => "드래곤",
        ("DragonKill", CaptionLocale::En) => "Dragon",
        ("ElderDragonKill", CaptionLocale::Ko) => "장로 드래곤",
        ("ElderDragonKill", CaptionLocale::En) => "Elder dragon",
        ("HeraldKill", CaptionLocale::Ko) => "전령",
        ("HeraldKill", CaptionLocale::En) => "Rift Herald",
        ("VoidgrubsKill", CaptionLocale::Ko) => "공허 유충",
        ("VoidgrubsKill", CaptionLocale::En) => "Voidgrubs",
        ("AtakhanKill", CaptionLocale::Ko) => "아타칸",
        ("AtakhanKill", CaptionLocale::En) => "Atakhan",
        ("TurretKill", CaptionLocale::Ko) => "포탑",
        ("TurretKill", CaptionLocale::En) => "Turret",
        ("InhibitorKill", CaptionLocale::Ko) => "억제기",
        ("InhibitorKill", CaptionLocale::En) => "Inhibitor",
        ("Ace", CaptionLocale::Ko) => "에이스",
        ("Ace", CaptionLocale::En) => "Ace",
        ("GameEnd", CaptionLocale::Ko) => "게임 끝",
        ("GameEnd", CaptionLocale::En) => "Game over",
        ("ManualReplay" | "ManualSave", CaptionLocale::Ko) => "직접 저장",
        ("ManualReplay" | "ManualSave", CaptionLocale::En) => "Manual save",
        // 1vX 아웃플레이는 인원수가 이름에 박혀서 온다(`Outplay1v3`). 숫자별로
        // 표를 늘리는 대신 접두사로 받는다 — 정적 문자열을 돌려주는 표라
        // 흔한 인원수만 나열한다(감지가 만드는 범위는 1v2~1v5).
        ("Outplay1v2", CaptionLocale::Ko) => "1대2 아웃플레이",
        ("Outplay1v2", CaptionLocale::En) => "1v2 outplay",
        ("Outplay1v3", CaptionLocale::Ko) => "1대3 아웃플레이",
        ("Outplay1v3", CaptionLocale::En) => "1v3 outplay",
        ("Outplay1v4", CaptionLocale::Ko) => "1대4 아웃플레이",
        ("Outplay1v4", CaptionLocale::En) => "1v4 outplay",
        ("Outplay1v5", CaptionLocale::Ko) => "1대5 아웃플레이",
        ("Outplay1v5", CaptionLocale::En) => "1v5 outplay",
        // `Multikill(n)` 의 n>=6 은 감지가 만들지 않지만 열거형이 막지 않는다.
        (other, CaptionLocale::Ko) if other.starts_with("Multikill") => "연속 킬",
        (other, CaptionLocale::En) if other.starts_with("Multikill") => "Multikill",
        (other, CaptionLocale::Ko) if other.starts_with("Outplay1v") => "아웃플레이",
        (other, CaptionLocale::En) if other.starts_with("Outplay1v") => "Outplay",
        _ => return None,
    };
    Some(title)
}

/// 점수 이유 → 자막 둘째 줄. 눈에 띄는 것부터, 최대 세 개.
///
/// 순서는 화면 카드(`src/lib/scoreReason.ts`)와 같은 규칙이다 — 같은 클립을 보고
/// 앱과 영상이 다른 순서로 말하면 그 자체가 결함처럼 읽힌다.
///
/// `skip_outnumbered` 는 제목이 이미 1vX 를 말할 때 켠다 — 걸러낸 **뒤에** 셋으로
/// 자르므로, 수적열세를 뺀 자리에 다음 이유가 올라온다.
fn reason_phrases(
    reasons: &[ScoreReason],
    locale: CaptionLocale,
    skip_outnumbered: bool,
) -> Vec<String> {
    let mut ranked: Vec<(u8, String)> = reasons
        .iter()
        .filter(|reason| !(skip_outnumbered && matches!(reason, ScoreReason::Outnumbered(..))))
        .map(|reason| match (reason, locale) {
            (ScoreReason::Clutch(pct), CaptionLocale::Ko) => (0, format!("체력 {}%", pct)),
            (ScoreReason::Clutch(pct), CaptionLocale::En) => (0, format!("{}% HP", pct)),
            (ScoreReason::Outnumbered(allies, enemies), CaptionLocale::Ko) => {
                (1, format!("{}대{}", allies, enemies))
            }
            (ScoreReason::Outnumbered(allies, enemies), CaptionLocale::En) => {
                (1, format!("{}v{}", allies, enemies))
            }
            (ScoreReason::Solo, CaptionLocale::Ko) => (2, "혼자서".to_string()),
            (ScoreReason::Solo, CaptionLocale::En) => (2, "Solo".to_string()),
            (ScoreReason::MatchPoint, CaptionLocale::Ko) => (3, "승부처".to_string()),
            (ScoreReason::MatchPoint, CaptionLocale::En) => (3, "Match point".to_string()),
            (ScoreReason::LateGame, CaptionLocale::Ko) => (4, "후반전".to_string()),
            (ScoreReason::LateGame, CaptionLocale::En) => (4, "Late game".to_string()),
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
        let caption = clip_caption(
            &clip(
                "PentaKill",
                vec![
                    ScoreReason::LateGame,
                    ScoreReason::Clutch(8),
                    ScoreReason::Solo,
                ],
            ),
            CaptionLocale::Ko,
        )
        .expect("자막이 나와야 한다");

        assert_eq!(caption.title, "펜타킬");
        // 눈에 띄는 것부터 — 화면 카드와 같은 순서.
        assert_eq!(caption.detail.as_deref(), Some("체력 8% · 혼자서 · 후반전"));
    }

    #[test]
    fn the_same_clip_in_english() {
        let caption = clip_caption(
            &clip(
                "PentaKill",
                vec![
                    ScoreReason::LateGame,
                    ScoreReason::Clutch(8),
                    ScoreReason::Solo,
                ],
            ),
            CaptionLocale::En,
        )
        .expect("자막이 나와야 한다");

        assert_eq!(caption.title, "Pentakill");
        assert_eq!(caption.detail.as_deref(), Some("8% HP · Solo · Late game"));
    }

    /// 제목이 「1대3 아웃플레이」인데 이유에 「1대4」가 또 나오면 안 된다.
    ///
    /// 둘은 서로 다른 것을 센다 — 전자는 10초 안에 내가 잡은 고유 피해자 수,
    /// 후자는 그 순간 살아 있던 양 팀 인원. 나란히 구우면 보는 사람이 하나를
    /// 틀린 값으로 읽는다. 화면 쪽 같은 규칙은 `src/lib/clipLabel.ts`.
    #[test]
    fn an_outplay_title_does_not_repeat_the_headcount_in_its_detail() {
        let mut outplay = clip(
            "Outplay1v3",
            vec![
                ScoreReason::Clutch(8),
                ScoreReason::Outnumbered(1, 4),
                ScoreReason::Solo,
            ],
        );
        outplay.event_type = "Outplay1v3".to_string();

        let caption = clip_caption(&outplay, CaptionLocale::Ko).unwrap();
        assert_eq!(caption.title, "1대3 아웃플레이");

        let detail = caption.detail.expect("이유가 있어야 한다");
        assert!(!detail.contains("1대4"), "수적열세가 중복됐다: {}", detail);
        // 뺀 자리에 다음 이유가 올라온다 — 걸러낸 뒤에 셋으로 자르기 때문이다.
        assert_eq!(detail, "체력 8% · 혼자서");
    }

    /// 반대로 제목이 1vX 가 아니면 수적열세는 그대로 보여준다.
    #[test]
    fn a_normal_title_keeps_the_headcount() {
        let caption = clip_caption(
            &clip("ChampionKill", vec![ScoreReason::Outnumbered(2, 5)]),
            CaptionLocale::Ko,
        )
        .unwrap();
        assert_eq!(caption.detail.as_deref(), Some("2대5"));
    }

    #[test]
    fn a_clip_with_no_reasons_still_gets_its_title() {
        // 상황을 못 찍은 클립(게임 상태 캐시가 비어 있었던 경우)도 무슨 장면인지는 안다.
        let caption = clip_caption(&clip("Shutdown", vec![]), CaptionLocale::Ko).unwrap();
        assert_eq!(caption.title, "셧다운");
        assert_eq!(caption.detail, None);
    }

    #[test]
    fn an_unknown_event_name_gets_no_caption_rather_than_a_code_value() {
        // 화면에 `Shutdown` 같은 영어 코드값이 클립 이름으로 나가던 결함을
        // 자막에서 반복하지 않는다.
        assert!(clip_caption(&clip("SomethingNew", vec![]), CaptionLocale::Ko).is_none());
        assert!(clip_caption(&clip("SomethingNew", vec![]), CaptionLocale::En).is_none());
    }

    #[test]
    fn the_caption_never_outlives_a_short_clip() {
        let mut short = clip("ChampionKill", vec![]);
        short.duration = Some(3.0);
        let caption = clip_caption(&short, CaptionLocale::Ko).unwrap();
        assert!(
            caption.duration_secs <= 1.81,
            "3초 클립에 {:.2}초 자막",
            caption.duration_secs
        );

        // 길이를 모르는 클립은 기본값 그대로.
        let mut unknown = clip("ChampionKill", vec![]);
        unknown.duration = None;
        assert_eq!(
            clip_caption(&unknown, CaptionLocale::Ko)
                .unwrap()
                .duration_secs,
            CAPTION_SECS
        );
    }

    /// 지원 밖 UI 언어(예: 일본어)는 영어로 떨어진다 — 앱의 `fallbackLng` 과 같은 규칙.
    #[test]
    fn unsupported_ui_languages_fall_back_to_english() {
        assert_eq!(CaptionLocale::from_ui_language("ja"), CaptionLocale::En);
        assert_eq!(CaptionLocale::from_ui_language("zh-CN"), CaptionLocale::En);
        assert_eq!(CaptionLocale::from_ui_language("th"), CaptionLocale::En);
    }

    /// i18next 가 지역 서브태그를 붙여 저장한 경우(`ko-KR`)도 한국어로 판정돼야 한다.
    #[test]
    fn regional_variants_of_korean_are_still_korean() {
        assert_eq!(CaptionLocale::from_ui_language("ko"), CaptionLocale::Ko);
        assert_eq!(CaptionLocale::from_ui_language("ko-KR"), CaptionLocale::Ko);
        assert_eq!(CaptionLocale::from_ui_language("KO"), CaptionLocale::Ko);
    }

    /// 백엔드가 **실제로 만들어 낼 수 있는** 이름을 전부 덮는가 — 두 로케일 모두.
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

        let names: Vec<String> = triggers
            .iter()
            .map(|t| {
                as_clip_info_string(crate::recording::auto_clip_manager::trigger_to_event_type(
                    t,
                ))
            })
            .collect();

        for locale in [CaptionLocale::Ko, CaptionLocale::En] {
            let missing: Vec<&String> = names
                .iter()
                .filter(|name| event_title(name, locale).is_none())
                .collect();
            assert!(
                missing.is_empty(),
                "{:?} 자막 표에 없는 이벤트: {:?}",
                locale,
                missing
            );
        }
    }
}
