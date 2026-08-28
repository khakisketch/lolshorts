/**
 * `HighlightPreset` — 백엔드 `src-tauri/src/settings/models.rs` 의 프론트 미러.
 *
 * 백엔드는 프리셋을 저장 구조에 담지 않는다. 설정 파일에는 여전히 개별 이벤트
 * 토글만 있고, 프리셋은 "지금 이 토글 조합이 어떤 묶음인가"를 되묻는 파생값이다
 * (`HighlightPreset::from_filters`). IPC 로도 아직 노출되지 않으므로 기본 설정
 * 화면이 프리셋을 보여주려면 같은 판정을 프론트에서도 해야 한다.
 *
 * 두 판정이 어긋나면 화면이 조용히 거짓말을 하게 되므로(백엔드는 Custom 인데
 * 화면은 "균형"이라고 표시), `highlightPreset.test.ts` 가 models.rs 를 직접 읽어
 * 아래 표와 대조한다. Rust 쪽 기본값·프리셋·필드가 바뀌면 그 테스트가 먼저 깨진다.
 */

/** `#[serde(rename_all = "snake_case")]` 가 만드는 와이어 값. */
export type HighlightPreset =
  | "everything"
  | "balanced"
  | "best_only"
  | "custom";

/** 사용자가 직접 고를 수 있는 프리셋(= `Custom` 제외). */
export type SelectableHighlightPreset = Exclude<HighlightPreset, "custom">;

/** `#[default]` 가 붙은 변형. */
export const DEFAULT_HIGHLIGHT_PRESET: SelectableHighlightPreset = "balanced";

/**
 * 기본 화면의 표시 순서. 담기는 양이 많은 것 → 적은 것.
 * (`from_filters` 의 탐색 순서와는 별개다 — 프리셋끼리 겹치지 않으므로 순서가
 * 판정에 영향을 주지 않는다.)
 */
export const SELECTABLE_HIGHLIGHT_PRESETS: readonly SelectableHighlightPreset[] =
  ["everything", "balanced", "best_only"];

/**
 * `EventFilterSettings` 전체 필드. `@/types` 의 `EventFilterSettings` 는 화면에
 * 노출된 17개만 담고 있지만, 백엔드의 프리셋 판정은 구조체 전체를 `PartialEq` 로
 * 비교하므로 미러는 숨은 필드까지 포함해야 한다.
 */
export interface CanonicalEventFilter {
  record_kills: boolean;
  record_multikills: boolean;
  record_first_blood: boolean;
  record_shutdown: boolean;
  record_deaths: boolean;
  record_first_blood_victim: boolean;
  record_assists: boolean;
  record_dragon: boolean;
  record_baron: boolean;
  record_elder: boolean;
  record_herald: boolean;
  record_turret: boolean;
  record_inhibitor: boolean;
  record_nexus: boolean;
  record_ace: boolean;
  record_game_end: boolean;
  record_steal: boolean;
  record_voidgrubs: boolean;
  record_atakhan: boolean;
  record_outplay: boolean;
  record_trade_kill: boolean;
  record_low_hp: boolean;
  min_priority: number;
  min_game_duration_secs: number;
  contest_window_secs: number;
}

/** `EventFilterSettings::default()` 미러. */
export const EVENT_FILTER_DEFAULTS: CanonicalEventFilter = {
  record_kills: true,
  record_multikills: true,
  record_first_blood: true,
  // 셧다운은 킬 계열이라 기본으로 담는다 — `false` 이던 동안 기본 설정 사용자는
  // "킬을 담겠다"고 켜 둔 채로 연속킬 저지 장면을 잃고 있었다.
  record_shutdown: true,
  record_deaths: false,
  record_first_blood_victim: false,
  // Balanced 프리셋과 같은 값. 어긋나면 새 설치가 "직접 설정" 으로 뜬다.
  record_assists: true,
  record_dragon: true,
  record_baron: true,
  record_elder: true,
  record_herald: true,
  record_turret: false,
  record_inhibitor: false,
  record_nexus: true,
  record_ace: true,
  record_game_end: true,
  record_steal: true,
  record_voidgrubs: true,
  record_atakhan: true,
  record_outplay: true,
  record_trade_kill: true,
  record_low_hp: true,
  min_priority: 1,
  min_game_duration_secs: 300,
  contest_window_secs: 10,
};

/** `HighlightPreset::to_filters()` 미러 — 기본값 위에 프리셋 override 를 얹은 결과. */
export const HIGHLIGHT_PRESET_FILTERS: Record<
  SelectableHighlightPreset,
  CanonicalEventFilter
> = {
  everything: {
    ...EVENT_FILTER_DEFAULTS,
    record_deaths: true,
    record_first_blood_victim: true,
    record_assists: true,
    record_turret: true,
  },
  balanced: {
    ...EVENT_FILTER_DEFAULTS,
    record_assists: true,
  },
  best_only: {
    ...EVENT_FILTER_DEFAULTS,
    record_kills: false,
    record_assists: false,
    record_deaths: false,
    record_first_blood: false,
    record_dragon: false,
    record_herald: false,
    record_inhibitor: false,
    record_voidgrubs: false,
    record_ace: false,
    min_priority: 3,
  },
};

/** 화면에 노출된 필터든 아니든, 프리셋 판정에 쓰이는 값 묶음. */
export type EventFilterLike = Partial<CanonicalEventFilter>;

/**
 * `HighlightPreset::from_filters()` 미러. 어느 프리셋과도 맞지 않으면 `custom`.
 *
 * 비교는 미러가 아는 필드에 한정된다. 백엔드가 새 필드를 추가하면
 * (미러가 그 필드를 모르는 동안) 프론트가 백엔드보다 관대해질 수 있는데,
 * 그 상태를 오래 두지 않으려고 드리프트 테스트가 필드 집합까지 대조한다.
 */
export function filtersToPreset(filters: EventFilterLike): HighlightPreset {
  const searchOrder: SelectableHighlightPreset[] = [
    "balanced",
    "everything",
    "best_only",
  ];

  for (const preset of searchOrder) {
    const canonical = HIGHLIGHT_PRESET_FILTERS[preset];
    const matches = (
      Object.keys(canonical) as (keyof CanonicalEventFilter)[]
    ).every((key) => filters[key] === canonical[key]);
    if (matches) return preset;
  }

  return "custom";
}

/**
 * 프리셋이 규정하는 조합을 현재 필터에 적용한다.
 *
 * 미러가 모르는 필드는 그대로 남긴다(스프레드 순서). 백엔드가 `to_filters()` 에서
 * 기본값 전체를 새로 만드는 것과 같은 결과이되, 프론트가 모르는 값을 지워
 * 역직렬화를 깨뜨리지는 않는다.
 */
export function applyHighlightPreset<T extends EventFilterLike>(
  preset: SelectableHighlightPreset,
  current: T,
): T & CanonicalEventFilter {
  return { ...current, ...HIGHLIGHT_PRESET_FILTERS[preset] };
}
