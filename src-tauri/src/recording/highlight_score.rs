//! 하이라이트 점수 — "이 클립이 얼마나 볼만한가".
//!
//! # 왜 다시 만드는가
//!
//! 기존 점수는 `EventTrigger::priority()` 하나뿐이었고, **이벤트 종류만** 봤다.
//! 그래서 체력 5% 에서 뒤집은 킬과 풀피 상대를 민 킬이 똑같이 1점이었고,
//! 킬·데스·어시스트·타워가 전부 1점이라 그 안의 순서는 DB 가 돌려준 순서
//! (= 사실상 무작위)로 정해졌다. 5단계 정수는 한 판에서 20개씩 나오는 클립을
//! 줄 세우기에 눈금이 너무 굵다.
//!
//! # 무엇으로 고치는가
//!
//! 롤은 **게임이 정답을 알려주는** 드문 경우다. Live Client Data API 로 이미
//! 받아서 파싱하고 있는 값들(내 체력, 어시스트 수, 양 팀 생존 수, 게임 시간,
//! 레벨)을 점수에 반영한다. 화면 픽셀을 읽어 추정하는 경쟁 서비스는 "체력
//! 8% 였다"를 확언할 수 없지만 우리는 확언할 수 있다 — 그런데 지금까지 그 값을
//! 점수에 한 번도 쓰지 않았다.
//!
//! # 설계 원칙
//!
//! 1. **기본 점수(0~100) × 배수** — 종류로 큰 자리를 잡고, 상황으로 미세 조정.
//!    배수를 곱셈으로 둔 이유는 "펜타킬인데 클러치"가 "킬인데 클러치"보다 더
//!    크게 올라야 하기 때문이다(덧셈이면 같은 폭으로 오른다).
//! 2. **설명 가능** — 모든 점수는 `reasons` 를 함께 낸다. 화면에는 숫자가 아니라
//!    이 이유가 나간다("3.8점"은 게이머에게 아무 뜻이 없지만 "체력 8%에서
//!    펜타킬"은 그 자체가 이유다).
//! 3. **없는 값에 기대지 않는다** — 모든 상황 신호는 `Option`. 값이 없으면 배수
//!    1.0 으로 지나간다. 관전 모드나 API 가 잠깐 죽은 순간에도 순위가 무너지지
//!    않아야 한다.
//! 4. **다양성은 점수가 아니라 선택 단계에서** — 같은 교전에서 나온 킬 다섯 개는
//!    각각 높은 점수를 받아 마땅하다. 그중 하나만 쓰는 것은 고르는 쪽의 일이다
//!    (`dedupe_by_moment`).

use serde::{Deserialize, Serialize};

/// 점수를 매길 이벤트의 종류. `EventTrigger` 와 1:1 은 아니다 — 점수 관점에서
/// 갈라야 하는 것(솔로킬 vs 일반 킬)은 나누고, 같은 것은 합쳤다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightKind {
    Pentakill,
    Quadrakill,
    Triplekill,
    Doublekill,
    /// 1vX 아웃플레이. `n` = 상대한 인원.
    Outplay(u8),
    ObjectiveSteal,
    Ace,
    Shutdown,
    FirstBlood,
    ElderDragon,
    Baron,
    Atakhan,
    Dragon,
    Herald,
    Voidgrubs,
    Inhibitor,
    Turret,
    Kill,
    TradeKill,
    Assist,
    /// 내가 퍼블을 당한 것. 죽는 장면이지만 판의 첫 사건이라 복기 가치가 있다.
    FirstBloodVictim,
    Death,
    /// 게임 종료. 이긴 판의 마지막 장면은 볼 이유가 있지만 진 판은 덜하다.
    GameEnd {
        won: bool,
    },
    /// 사용자가 직접 저장한 것(F8/F9/F10). 사람이 고른 것이므로 중간 이상은 준다.
    ManualSave,
}

impl HighlightKind {
    /// 종류만으로 정해지는 기본 점수(0~100).
    ///
    /// 눈금을 100 으로 넓힌 이유: 배수가 붙어도 등급이 뒤집히지 않을 만큼 간격이
    /// 필요하다. 예컨대 "일반 킬(25)이 클러치 배수 1.5 를 받아도 37.5" 라서
    /// "트리플킬(70)"을 넘지 못한다 — 이건 의도한 성질이다. 반대로 "더블킬(50)이
    /// 1v3 상황에서 1.5×1.25 = 93" 은 펜타킬 근처까지 올라온다.
    pub fn base(self) -> f64 {
        match self {
            HighlightKind::Pentakill => 100.0,
            HighlightKind::Outplay(n) if n >= 3 => 90.0,
            HighlightKind::Quadrakill => 85.0,
            HighlightKind::ObjectiveSteal => 80.0,
            HighlightKind::Outplay(_) => 75.0,
            HighlightKind::Triplekill => 70.0,
            HighlightKind::Ace => 68.0,
            HighlightKind::ElderDragon => 60.0,
            HighlightKind::Shutdown => 55.0,
            HighlightKind::Baron => 55.0,
            HighlightKind::Doublekill => 50.0,
            HighlightKind::ManualSave => 50.0,
            HighlightKind::GameEnd { won: true } => 48.0,
            HighlightKind::FirstBlood => 45.0,
            HighlightKind::Atakhan => 45.0,
            HighlightKind::Dragon => 35.0,
            HighlightKind::Herald => 30.0,
            HighlightKind::Inhibitor => 30.0,
            HighlightKind::Kill => 25.0,
            HighlightKind::GameEnd { won: false } => 25.0,
            HighlightKind::Voidgrubs => 22.0,
            HighlightKind::TradeKill => 20.0,
            HighlightKind::FirstBloodVictim => 18.0,
            HighlightKind::Turret => 15.0,
            HighlightKind::Assist => 12.0,
            HighlightKind::Death => 10.0,
        }
    }
}

/// 이벤트가 일어난 순간의 상황. 전부 `Option` — 못 읽었으면 그 축은 점수에
/// 영향을 주지 않는다.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MomentContext {
    /// 이벤트 직후 내 체력 비율(0.0~1.0).
    pub my_health_ratio: Option<f64>,
    /// 어시스트한 아군 수. 0 이면 단독.
    pub assist_count: Option<u32>,
    /// 이 순간 살아 있던 아군 수(나 포함).
    pub allies_alive: Option<u32>,
    /// 이 순간 살아 있던 적군 수.
    pub enemies_alive: Option<u32>,
    /// 게임 경과 시간(초).
    pub game_time_secs: Option<f64>,
    /// 게임이 끝나기까지 남은 시간(초). 게임이 끝난 뒤에만 채울 수 있다.
    pub secs_before_game_end: Option<f64>,
}

/// 점수에 붙는 사람 말 이유. 화면에 나가는 것은 숫자가 아니라 이것이다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreReason {
    /// 체력이 아주 낮은 상태였다. 값은 퍼센트(정수).
    Clutch(u8),
    /// 도움 없이 혼자 해냈다.
    Solo,
    /// 수적 열세였다. (아군, 적군)
    Outnumbered(u32, u32),
    /// 후반전이었다.
    LateGame,
    /// 승부가 갈리기 직전이었다.
    MatchPoint,
}

/// 한 클립의 최종 점수와 그 이유.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighlightScore {
    pub value: f64,
    pub reasons: Vec<ScoreReason>,
}

/// 체력 배수. 이벤트 **직후** 체력이므로, 킬을 따낸 뒤 체력이 바닥이면 그 교전이
/// 아슬아슬했다는 뜻이다.
///
/// # 체력 0% 는 클러치가 아니다
///
/// 0% 는 살아남은 게 아니라 **죽은 것**이다. 예전에는 `ratio <= 0.10` 갈래가
/// 0.0 도 받아 최고 배수 1.50 을 줬고, 실게임에서 이렇게 나왔다:
///
/// ```text
///   Assist   29.0 — [Clutch(0), Outnumbered(2,5), LateGame]
///   GameEnd  43.1 — [Clutch(0), LateGame]
/// ```
///
/// 어시스트는 내가 죽은 뒤 팀이 마무리해도 발생하고, 게임 끝은 거의 항상 부활
/// 대기 중이다. 죽은 순간이 가장 높은 가산점을 받고 훅 자막에 「체력 0%」가
/// 찍혔다 — 시청자에게 자랑할 문구가 아니다.
///
/// 종류별 제외(`Death`·`FirstBloodVictim`·`GameEnd`)는 `score()` 가 맡는다.
/// 여기서는 **값 자체가 말이 안 되는 경우**만 거른다.
fn clutch_multiplier(ratio: f64) -> (f64, Option<ScoreReason>) {
    // 0% = 죽음. 살아남지 못했으면 아슬아슬한 것도 아니다.
    if ratio <= 0.0 {
        return (1.0, None);
    }

    let pct = (ratio * 100.0).round().clamp(0.0, 100.0) as u8;
    if ratio <= 0.10 {
        (1.50, Some(ScoreReason::Clutch(pct)))
    } else if ratio <= 0.25 {
        (1.30, Some(ScoreReason::Clutch(pct)))
    } else if ratio <= 0.40 {
        (1.15, Some(ScoreReason::Clutch(pct)))
    } else {
        (1.0, None)
    }
}

/// 단독성 배수. 아군 넷이 붙어 한 명을 정리한 장면은 하이라이트가 아니다.
fn solo_multiplier(assists: u32) -> (f64, Option<ScoreReason>) {
    match assists {
        0 => (1.25, Some(ScoreReason::Solo)),
        1 => (1.10, None),
        2 => (1.0, None),
        _ => (0.90, None),
    }
}

/// 수적 열세 배수. 1v3 은 그 자체로 이야기가 된다.
fn outnumbered_multiplier(allies: u32, enemies: u32) -> (f64, Option<ScoreReason>) {
    if allies == 0 || enemies <= allies {
        return (1.0, None);
    }
    let gap = enemies - allies;
    let m = match gap {
        1 => 1.12,
        2 => 1.25,
        _ => 1.40,
    };
    (m, Some(ScoreReason::Outnumbered(allies, enemies)))
}

/// 시점 배수. 초반 킬은 흔하고 판을 가르지 않는다.
///
/// 퍼스트블러드는 예외다 — 정의상 초반에만 일어나므로 감점하면 항상 깎인다.
/// (당한 쪽도 같은 순간이므로 같이 예외로 둔다.)
fn timing_multiplier(kind: HighlightKind, secs: f64) -> (f64, Option<ScoreReason>) {
    if secs >= 20.0 * 60.0 {
        (1.15, Some(ScoreReason::LateGame))
    } else if secs < 5.0 * 60.0
        && !matches!(
            kind,
            HighlightKind::FirstBlood | HighlightKind::FirstBloodVictim
        )
    {
        (0.90, None)
    } else {
        (1.0, None)
    }
}

/// 클립 하나의 점수를 낸다.
///
/// 반환값은 상한이 없다 — 정규화는 하지 않는다. 순위를 정하는 데만 쓰이므로
/// 절대값의 의미는 중요하지 않고, 상한을 두면 최상위 구간이 뭉개진다.
pub fn score(kind: HighlightKind, ctx: &MomentContext) -> HighlightScore {
    let mut value = kind.base();
    let mut reasons = Vec::new();

    let apply = |m: (f64, Option<ScoreReason>), value: &mut f64, reasons: &mut Vec<_>| {
        *value *= m.0;
        if let Some(r) = m.1 {
            reasons.push(r);
        }
    };

    // 클러치를 물어볼 수 있는 순간인가.
    //
    // - `Death`·`FirstBloodVictim` — 죽는 장면이라 체력이 언제나 0
    // - `GameEnd` — 게임이 끝난 시점의 체력은 아무 의미가 없다. 이겼든 졌든
    //   그 순간 부활 대기 중이면 0 이고, 살아 있었다면 그 값이 우연일 뿐이다.
    //   실게임에서 「게임 끝 · 체력 0%」가 43.1점을 받았다
    //
    // 오브젝트·스틸은 **제외하지 않는다** — 체력 5% 에 바론을 스틸한 건 진짜
    // 클러치이고, 그게 이 앱이 확언할 수 있는 종류의 사실이다.
    if !matches!(
        kind,
        HighlightKind::Death | HighlightKind::FirstBloodVictim | HighlightKind::GameEnd { .. }
    ) {
        if let Some(ratio) = ctx.my_health_ratio {
            apply(clutch_multiplier(ratio), &mut value, &mut reasons);
        }
    }

    // 단독성·열세는 "누가 해냈나"를 다루므로 킬 계열에만 의미가 있다.
    // 오브젝트나 게임 종료에 붙이면 엉뚱한 이유가 화면에 나간다.
    if kind_is_combat(kind) {
        if let Some(assists) = ctx.assist_count {
            apply(solo_multiplier(assists), &mut value, &mut reasons);
        }
        if let (Some(a), Some(e)) = (ctx.allies_alive, ctx.enemies_alive) {
            apply(outnumbered_multiplier(a, e), &mut value, &mut reasons);
        }
    }

    if let Some(secs) = ctx.game_time_secs {
        apply(timing_multiplier(kind, secs), &mut value, &mut reasons);
    }

    // 승부가 갈리기 직전 2분. 마지막 한타는 결과를 아는 채로 보면 더 재미있다.
    if let Some(remaining) = ctx.secs_before_game_end {
        if remaining <= 120.0 {
            value *= 1.20;
            reasons.push(ScoreReason::MatchPoint);
        }
    }

    HighlightScore { value, reasons }
}

/// 전투 계열인가 — 단독성·열세 배수를 붙일 대상인가.
fn kind_is_combat(kind: HighlightKind) -> bool {
    matches!(
        kind,
        HighlightKind::Pentakill
            | HighlightKind::Quadrakill
            | HighlightKind::Triplekill
            | HighlightKind::Doublekill
            | HighlightKind::Outplay(_)
            | HighlightKind::Shutdown
            | HighlightKind::FirstBlood
            | HighlightKind::Kill
            | HighlightKind::TradeKill
            | HighlightKind::Assist
    )
}

/// 점수를 매길 후보 하나.
#[derive(Debug, Clone)]
pub struct Candidate<T> {
    pub item: T,
    pub kind: HighlightKind,
    /// 게임 내 이벤트 시각(초).
    pub at_secs: f64,
    pub score: HighlightScore,
}

/// 같은 순간에서 나온 후보들을 하나로 접는다.
///
/// 한 번의 교전에서 킬 이벤트가 셋 나오면 클립 셋이 만들어지고, 그 셋은 **거의
/// 같은 영상**이다(pre-roll 이 겹친다). 점수 단계에서 깎으면 안 된다 — 각각은
/// 정당하게 높은 점수를 받아야 한다. 대신 고를 때 그중 최고점 하나만 남긴다.
///
/// `window_secs` 는 설정의 장면 병합 간격(기본 15초)과 같은 뜻이다.
pub fn dedupe_by_moment<T>(
    mut candidates: Vec<Candidate<T>>,
    window_secs: f64,
) -> Vec<Candidate<T>> {
    if candidates.len() <= 1 {
        return candidates;
    }

    // 시간순으로 훑으며 창 안에 있는 것끼리 묶고, 묶음마다 최고점만 남긴다.
    candidates.sort_by(|a, b| {
        a.at_secs
            .partial_cmp(&b.at_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut kept: Vec<Candidate<T>> = Vec::new();
    for cand in candidates {
        match kept.last_mut() {
            Some(last) if cand.at_secs - last.at_secs <= window_secs => {
                if cand.score.value > last.score.value {
                    *last = cand;
                }
            }
            _ => kept.push(cand),
        }
    }
    kept
}

/// 점수 높은 순으로 정렬한다. 동점이면 이른 시각이 앞.
///
/// 동점 처리를 명시하는 이유: 예전 구현은 동점 구간의 순서를 DB 반환 순서에
/// 맡겼고, 그래서 "킬 1점짜리 여덟 개" 중 무엇이 뽑히는지가 실행마다 달랐다.
pub fn rank<T>(candidates: &mut [Candidate<T>]) {
    candidates.sort_by(|a, b| {
        b.score
            .value
            .partial_cmp(&a.score.value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.at_secs
                    .partial_cmp(&b.at_secs)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> MomentContext {
        MomentContext::default()
    }

    /// **체력 0% 는 클러치가 아니라 죽은 것이다.**
    ///
    /// 실게임 회귀: `Assist 29.0 — [Clutch(0), ...]`, `GameEnd 43.1 — [Clutch(0), ...]`.
    /// 죽은 순간이 최고 가산점(1.50)을 받고 훅 자막에 「체력 0%」가 찍혔다.
    #[test]
    fn zero_health_is_death_not_a_clutch() {
        let dead = MomentContext {
            my_health_ratio: Some(0.0),
            ..ctx()
        };

        for kind in [
            HighlightKind::Assist,
            HighlightKind::Kill,
            HighlightKind::ObjectiveSteal,
        ] {
            let scored = score(kind, &dead);
            assert!(
                !scored
                    .reasons
                    .iter()
                    .any(|r| matches!(r, ScoreReason::Clutch(_))),
                "{:?} 에 체력 0% 클러치가 붙었다: {:?}",
                kind,
                scored.reasons
            );
        }
    }

    /// 게임이 끝난 시점의 체력은 의미가 없다 — 이겼든 졌든 우연이다.
    #[test]
    fn the_end_of_the_game_never_counts_as_a_clutch() {
        for ratio in [0.0, 0.05, 0.3, 0.9] {
            let moment = MomentContext {
                my_health_ratio: Some(ratio),
                ..ctx()
            };
            for won in [true, false] {
                let scored = score(HighlightKind::GameEnd { won }, &moment);
                assert!(
                    !scored
                        .reasons
                        .iter()
                        .any(|r| matches!(r, ScoreReason::Clutch(_))),
                    "게임 끝(won={}, 체력 {})에 클러치가 붙었다: {:?}",
                    won,
                    ratio,
                    scored.reasons
                );
            }
        }
    }

    /// 반대 방향도 지킨다 — 살아남은 저체력은 여전히 클러치다.
    #[test]
    fn surviving_on_low_health_is_still_a_clutch() {
        let barely = MomentContext {
            my_health_ratio: Some(0.05),
            ..ctx()
        };

        // 체력 5% 에 바론 스틸 — 이 앱이 확언할 수 있는 종류의 사실이다.
        let scored = score(HighlightKind::ObjectiveSteal, &barely);
        assert!(
            scored.reasons.contains(&ScoreReason::Clutch(5)),
            "저체력 스틸에 클러치가 안 붙었다: {:?}",
            scored.reasons
        );
        assert!(scored.value > HighlightKind::ObjectiveSteal.base());
    }

    #[test]
    fn base_scores_are_ordered_by_how_watchable_the_event_is() {
        assert!(HighlightKind::Pentakill.base() > HighlightKind::Quadrakill.base());
        assert!(HighlightKind::Quadrakill.base() > HighlightKind::Triplekill.base());
        assert!(HighlightKind::Triplekill.base() > HighlightKind::Doublekill.base());
        assert!(HighlightKind::Doublekill.base() > HighlightKind::Kill.base());
        assert!(HighlightKind::Kill.base() > HighlightKind::Assist.base());
        assert!(HighlightKind::ObjectiveSteal.base() > HighlightKind::Baron.base());
        assert!(HighlightKind::Outplay(3).base() > HighlightKind::Outplay(2).base());
    }

    #[test]
    fn winning_the_game_beats_losing_it() {
        assert!(
            HighlightKind::GameEnd { won: true }.base()
                > HighlightKind::GameEnd { won: false }.base()
        );
    }

    #[test]
    fn no_context_means_the_base_score_stands() {
        let s = score(HighlightKind::Kill, &ctx());
        assert_eq!(s.value, HighlightKind::Kill.base());
        assert!(s.reasons.is_empty());
    }

    #[test]
    fn a_kill_at_eight_percent_health_outranks_a_comfortable_one() {
        let clutch = score(
            HighlightKind::Kill,
            &MomentContext {
                my_health_ratio: Some(0.08),
                ..Default::default()
            },
        );
        let safe = score(
            HighlightKind::Kill,
            &MomentContext {
                my_health_ratio: Some(0.95),
                ..Default::default()
            },
        );
        assert!(clutch.value > safe.value);
        assert_eq!(clutch.reasons, vec![ScoreReason::Clutch(8)]);
        assert!(safe.reasons.is_empty());
    }

    #[test]
    fn deaths_are_not_treated_as_clutch_even_though_health_is_zero() {
        // 죽으면 체력은 항상 0 이다. 이 예외가 없으면 모든 데스가 최고 배수를 받는다.
        let s = score(
            HighlightKind::Death,
            &MomentContext {
                my_health_ratio: Some(0.0),
                ..Default::default()
            },
        );
        assert_eq!(s.value, HighlightKind::Death.base());
        assert!(s.reasons.is_empty());
    }

    #[test]
    fn a_solo_kill_beats_the_same_kill_with_four_assists() {
        let solo = score(
            HighlightKind::Kill,
            &MomentContext {
                assist_count: Some(0),
                ..Default::default()
            },
        );
        let piled_on = score(
            HighlightKind::Kill,
            &MomentContext {
                assist_count: Some(4),
                ..Default::default()
            },
        );
        assert!(solo.value > piled_on.value);
        assert_eq!(solo.reasons, vec![ScoreReason::Solo]);
    }

    #[test]
    fn being_outnumbered_raises_the_score_and_says_so() {
        let s = score(
            HighlightKind::Doublekill,
            &MomentContext {
                allies_alive: Some(1),
                enemies_alive: Some(3),
                ..Default::default()
            },
        );
        assert!(s.value > HighlightKind::Doublekill.base());
        assert!(s.reasons.contains(&ScoreReason::Outnumbered(1, 3)));
    }

    #[test]
    fn having_the_numbers_is_not_a_bonus() {
        let s = score(
            HighlightKind::Kill,
            &MomentContext {
                allies_alive: Some(4),
                enemies_alive: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(s.value, HighlightKind::Kill.base());
    }

    #[test]
    fn objective_kills_do_not_get_combat_modifiers() {
        // 바론에 "혼자 해냈다"는 이유가 붙으면 화면 문구가 이상해진다.
        let s = score(
            HighlightKind::Baron,
            &MomentContext {
                assist_count: Some(0),
                allies_alive: Some(1),
                enemies_alive: Some(4),
                ..Default::default()
            },
        );
        assert_eq!(s.value, HighlightKind::Baron.base());
        assert!(s.reasons.is_empty());
    }

    #[test]
    fn early_kills_are_discounted_but_first_blood_is_not() {
        let early_kill = score(
            HighlightKind::Kill,
            &MomentContext {
                game_time_secs: Some(120.0),
                ..Default::default()
            },
        );
        assert!(early_kill.value < HighlightKind::Kill.base());

        let fb = score(
            HighlightKind::FirstBlood,
            &MomentContext {
                game_time_secs: Some(120.0),
                ..Default::default()
            },
        );
        // 퍼스트블러드는 정의상 초반에만 난다. 감점하면 언제나 깎인다.
        assert_eq!(fb.value, HighlightKind::FirstBlood.base());
    }

    #[test]
    fn late_game_is_worth_more() {
        let late = score(
            HighlightKind::Kill,
            &MomentContext {
                game_time_secs: Some(25.0 * 60.0),
                ..Default::default()
            },
        );
        assert!(late.value > HighlightKind::Kill.base());
        assert!(late.reasons.contains(&ScoreReason::LateGame));
    }

    #[test]
    fn the_last_two_minutes_carry_a_bonus() {
        let s = score(
            HighlightKind::Ace,
            &MomentContext {
                secs_before_game_end: Some(45.0),
                ..Default::default()
            },
        );
        assert!(s.reasons.contains(&ScoreReason::MatchPoint));
        assert!(s.value > HighlightKind::Ace.base());
    }

    #[test]
    fn modifiers_compound_so_a_great_moment_climbs_past_a_bigger_but_plain_one() {
        // 1v3 에서 체력 6% 로 낸 더블킬은, 아무 맥락 없는 트리플킬보다 위에 온다.
        let heroic_double = score(
            HighlightKind::Doublekill,
            &MomentContext {
                my_health_ratio: Some(0.06),
                assist_count: Some(0),
                allies_alive: Some(1),
                enemies_alive: Some(3),
                ..Default::default()
            },
        );
        let plain_triple = score(HighlightKind::Triplekill, &ctx());
        assert!(
            heroic_double.value > plain_triple.value,
            "{} vs {}",
            heroic_double.value,
            plain_triple.value
        );
    }

    #[test]
    fn a_plain_kill_never_climbs_past_a_pentakill() {
        // 반대 방향의 안전장치: 배수가 아무리 붙어도 등급을 통째로 뒤집지는 않는다.
        let best_possible_kill = score(
            HighlightKind::Kill,
            &MomentContext {
                my_health_ratio: Some(0.01),
                assist_count: Some(0),
                allies_alive: Some(1),
                enemies_alive: Some(5),
                game_time_secs: Some(30.0 * 60.0),
                secs_before_game_end: Some(10.0),
            },
        );
        let plain_penta = score(HighlightKind::Pentakill, &ctx());
        assert!(
            best_possible_kill.value < plain_penta.value,
            "{} vs {}",
            best_possible_kill.value,
            plain_penta.value
        );
    }

    fn cand(kind: HighlightKind, at: f64, hp: Option<f64>) -> Candidate<&'static str> {
        let ctx = MomentContext {
            my_health_ratio: hp,
            ..Default::default()
        };
        Candidate {
            item: "clip",
            kind,
            at_secs: at,
            score: score(kind, &ctx),
        }
    }

    #[test]
    fn one_teamfight_yields_one_clip_not_five() {
        // 같은 교전(15초 창) 안의 킬 셋 -> 최고점 하나만 남는다.
        let kept = dedupe_by_moment(
            vec![
                cand(HighlightKind::Kill, 600.0, None),
                cand(HighlightKind::Doublekill, 605.0, None),
                cand(HighlightKind::Kill, 612.0, None),
            ],
            15.0,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].kind, HighlightKind::Doublekill);
    }

    #[test]
    fn separate_fights_are_kept_separately() {
        let kept = dedupe_by_moment(
            vec![
                cand(HighlightKind::Kill, 600.0, None),
                cand(HighlightKind::Kill, 900.0, None),
            ],
            15.0,
        );
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn dedupe_walks_the_whole_chain_not_just_neighbours() {
        // 5초 간격 넷 -> 전체가 한 덩어리. 앞뒤만 비교하면 두 개가 남는다.
        let kept = dedupe_by_moment(
            vec![
                cand(HighlightKind::Kill, 600.0, None),
                cand(HighlightKind::Kill, 605.0, None),
                cand(HighlightKind::Triplekill, 610.0, None),
                cand(HighlightKind::Kill, 615.0, None),
            ],
            15.0,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].kind, HighlightKind::Triplekill);
    }

    #[test]
    fn dedupe_handles_empty_and_single_inputs() {
        let empty: Vec<Candidate<&str>> = Vec::new();
        assert!(dedupe_by_moment(empty, 15.0).is_empty());
        assert_eq!(
            dedupe_by_moment(vec![cand(HighlightKind::Kill, 1.0, None)], 15.0).len(),
            1
        );
    }

    #[test]
    fn ranking_is_deterministic_when_scores_tie() {
        // 예전 구현이 무너진 지점: 동점이면 DB 순서에 맡겨서 실행마다 달랐다.
        let mut a = vec![
            cand(HighlightKind::Kill, 900.0, None),
            cand(HighlightKind::Kill, 100.0, None),
            cand(HighlightKind::Kill, 500.0, None),
        ];
        let mut b = vec![
            cand(HighlightKind::Kill, 500.0, None),
            cand(HighlightKind::Kill, 900.0, None),
            cand(HighlightKind::Kill, 100.0, None),
        ];
        rank(&mut a);
        rank(&mut b);
        let at = |v: &Vec<Candidate<&str>>| v.iter().map(|c| c.at_secs).collect::<Vec<_>>();
        assert_eq!(at(&a), at(&b));
        assert_eq!(at(&a), vec![100.0, 500.0, 900.0]);
    }

    #[test]
    fn ranking_puts_the_best_moment_first() {
        let mut v = vec![
            cand(HighlightKind::Kill, 100.0, None),
            cand(HighlightKind::Pentakill, 200.0, None),
            cand(HighlightKind::Doublekill, 300.0, None),
        ];
        rank(&mut v);
        assert_eq!(v[0].kind, HighlightKind::Pentakill);
        assert_eq!(v[2].kind, HighlightKind::Kill);
    }
}
