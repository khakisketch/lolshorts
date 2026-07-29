/**
 * "지금 이 설정으로 실제로 뭐가 담기나" 판정.
 *
 * # 왜 필요한가
 *
 * 하루에 두 번, 설정값 하나 때문에 한 판 분량의 클립을 통째로 잃었다.
 *
 * - `min_priority: 5` — 킬·멀티킬·에이스·바론·드래곤이 전부 감지됐는데 **하나도
 *   저장되지 않았다.** 로그에는 `Event filtered out by settings` 가 수십 줄
 *   찍혔지만 화면은 "녹화 중"만 보여줬다.
 * - 「확실한 것만」 프리셋 — 일반 킬이 전부 빠지는데, 설명은 "클립 수는 적습니다"
 *   라고만 말한다. 무엇이 빠지는지는 알 수 없다.
 *
 * 둘 다 앱은 정상 동작이었다. 문제는 **아무것도 안 담기는 상태와 잘 담기는
 * 상태가 화면에서 구분되지 않는다**는 것이다.
 *
 * # 판정 방식
 *
 * 백엔드 `should_record_event` 는 두 관문을 차례로 통과시킨다
 * (`auto_clip_manager.rs`):
 *
 * 1. `trigger.priority() >= min_priority`
 * 2. 그 이벤트의 `record_*` 토글이 켜져 있음
 *
 * 여기서도 같은 순서로 판정한다. 우선순위 표는 `EventTrigger::priority()`
 * (`live_client.rs`) 의 미러이고, 어긋나면 `captureCoverage.test.ts` 가
 * Rust 소스를 직접 읽어 깨뜨린다.
 */

/** 화면에 보여줄 장면 하나. */
export interface CaptureScene {
  /** `EventFilterSettings` 의 토글 이름. */
  flag: string;
  /** `EventTrigger::priority()` 값. */
  priority: number;
  /** i18n 키 뒤에 붙는 이름 (`settings.basic.highlights.scenes.<flag>`). */
  labelKey: string;
}

/**
 * 백엔드가 실제로 트리거를 가르는 장면들과 그 우선순위.
 *
 * `record_nexus` 는 없다 — 백엔드에 소비처가 없다(`settingSpecs.ts` 참조).
 */
export const CAPTURE_SCENES: readonly CaptureScene[] = [
  { flag: "record_kills", priority: 1, labelKey: "record_kills" },
  { flag: "record_deaths", priority: 1, labelKey: "record_deaths" },
  // 퍼블을 당한 것 — 데스의 하위 상황인데 우선순위는 일반 데스보다 높다.
  {
    flag: "record_first_blood_victim",
    priority: 3,
    labelKey: "record_first_blood_victim",
  },
  { flag: "record_assists", priority: 1, labelKey: "record_assists" },
  { flag: "record_turret", priority: 1, labelKey: "record_turret" },
  { flag: "record_dragon", priority: 2, labelKey: "record_dragon" },
  { flag: "record_herald", priority: 2, labelKey: "record_herald" },
  { flag: "record_inhibitor", priority: 2, labelKey: "record_inhibitor" },
  { flag: "record_voidgrubs", priority: 2, labelKey: "record_voidgrubs" },
  { flag: "record_trade_kill", priority: 2, labelKey: "record_trade_kill" },
  { flag: "record_first_blood", priority: 3, labelKey: "record_first_blood" },
  { flag: "record_baron", priority: 3, labelKey: "record_baron" },
  { flag: "record_atakhan", priority: 3, labelKey: "record_atakhan" },
  { flag: "record_shutdown", priority: 3, labelKey: "record_shutdown" },
  { flag: "record_game_end", priority: 3, labelKey: "record_game_end" },
  { flag: "record_ace", priority: 4, labelKey: "record_ace" },
  { flag: "record_steal", priority: 4, labelKey: "record_steal" },
  { flag: "record_elder", priority: 4, labelKey: "record_elder" },
  { flag: "record_low_hp", priority: 4, labelKey: "record_low_hp" },
  // 멀티킬은 단계마다 우선순위가 다르다(더블 2 … 펜타 5). 토글은 하나뿐이므로
  // **가장 낮은 단계**로 잡는다 — 그래야 "더블킬이 빠진다"를 정확히 말할 수 있다.
  { flag: "record_multikills", priority: 2, labelKey: "record_multikills" },
  // 1vX 아웃플레이도 인원수에 따라 4 또는 5. 낮은 쪽으로.
  { flag: "record_outplay", priority: 4, labelKey: "record_outplay" },
];

export type CoverageLevel = "none" | "narrow" | "normal";

export interface CoverageVerdict {
  level: CoverageLevel;
  /** 실제로 담기는 장면 (토글 ON + 우선순위 통과). */
  captured: CaptureScene[];
  /**
   * 토글은 켜져 있는데 **우선순위 문턱에 막혀** 버려지는 장면.
   *
   * 이 목록이 비어 있지 않다는 것은 사용자가 "담겠다"고 켜 둔 것이 조용히
   * 버려진다는 뜻이라, 화면이 반드시 말해야 한다.
   */
  blockedByPriority: CaptureScene[];
}

export interface CoverageInput {
  min_priority?: number;
  [flag: string]: boolean | number | undefined;
}

/**
 * 담기는 장면이 몇 개인지에 따라 등급을 매긴다.
 *
 * - `none` — 하나도 안 담긴다. 녹화는 돌지만 결과물이 0개다.
 * - `narrow` — 3개 이하. 한 판에 클립이 안 나올 수 있다.
 * - `normal` — 그 외.
 */
export function evaluateCoverage(filter: CoverageInput): CoverageVerdict {
  const minPriority =
    typeof filter.min_priority === "number" ? filter.min_priority : 1;

  const enabled = CAPTURE_SCENES.filter((s) => filter[s.flag] === true);
  const captured = enabled.filter((s) => s.priority >= minPriority);
  const blockedByPriority = enabled.filter((s) => s.priority < minPriority);

  let level: CoverageLevel;
  if (captured.length === 0) {
    level = "none";
  } else if (captured.length <= 3) {
    level = "narrow";
  } else {
    level = "normal";
  }

  return { level, captured, blockedByPriority };
}
