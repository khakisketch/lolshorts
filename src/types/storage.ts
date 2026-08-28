// src/types/storage.ts
// Aligned with Rust backend: src-tauri/src/storage/models.rs

export type GameResult = "Win" | "Loss" | "Remake";

export interface KDA {
  kills: number;
  deaths: number;
  assists: number;
}

export interface GameMetadata {
  game_id: string;
  champion: string;
  game_mode: string;
  start_time: string;
  end_time: string | null;
  result: GameResult | null;
  kda: KDA | null;
}

// EventType is a tagged enum in Rust with snake_case serialization.
// Simple variants serialize as strings, Multikill/Custom carry data.
export type EventType =
  | "champion_kill"
  | "turret_kill"
  | "inhibitor_kill"
  | "dragon_kill"
  | "baron_kill"
  | "ace"
  | "first_blood"
  | { multikill: number }
  | { custom: string };

export interface EventData {
  event_id: number;
  event_type: EventType;
  timestamp: number;
  priority: number;
  participants: string[];
  details?: Record<string, unknown> | null;
}

/**
 * 점수가 그렇게 나온 이유 — `recording::highlight_score::ScoreReason` 미러.
 *
 * 와이어 표현이 PascalCase 인 것은 오타가 아니다. 이 enum 에는 `EventType` 과 달리
 * `#[serde(rename_all)]` 이 없어 serde 기본값(외부 태깅 · 변형 이름 그대로)으로
 * 나간다. 이미 디스크에 그 모양으로 쓰인 클립이 있을 수 있어 맞춰 둔다 —
 * 케이스가 어긋나면 `clips.json` 전체가 역직렬화에 실패해 클립 목록이 통째로
 * 비어 버린다(빠진 필드와 달리 `#[serde(default)]` 가 구해 주지 않는다).
 *
 * 변형이 늘거나 이름이 바뀌면 `scoreReason.test.ts` 가 Rust 소스를 직접 읽어 깨뜨린다.
 */
export type ScoreReason =
  /** 체력이 아주 낮은 상태였다. 값은 퍼센트(정수). */
  | { Clutch: number }
  /** 도움 없이 혼자 해냈다. */
  | "Solo"
  /** 수적 열세였다. `[아군, 적군]`. */
  | { Outnumbered: [number, number] }
  /** 후반전이었다. */
  | "LateGame"
  /** 승부가 갈리기 직전이었다. */
  | "MatchPoint";

export interface ClipMetadata {
  file_path: string;
  thumbnail_path?: string | null;
  event_type: EventType;
  event_time: number;
  priority: number;
  duration: number;
  /**
   * 이 클립 **안에서** 하이라이트가 일어나는 지점(초).
   *
   * 클립 중앙이 아니다 — 킬은 pre 10 / post 3, 게임 종료는 pre 30 / post 10 이라
   * 트리거마다 다르다. 미리보기가 여기서부터 재생하면 사용자가 매번 앞부분을
   * 건너뛰지 않아도 된다. 예전 클립에는 없다.
   */
  event_offset_secs?: number | null;
  /**
   * 하이라이트 점수. 화면에 **숫자로 내보내지 않는다** — "37.5점" 은 게이머에게
   * 아무 뜻이 없다. 정렬과 강조에만 쓰고, 사람에게는 `score_reasons` 를 보여준다.
   */
  highlight_score?: number | null;
  /** 점수가 그렇게 나온 이유. 화면에 나가는 것은 숫자가 아니라 이쪽이다. */
  score_reasons?: ScoreReason[];
  created_at: string;
  usage_count?: number;
}

export type ClipVaultSort = "best" | "newest";

export interface ClipVaultGameGroup {
  game_id: string;
  game: GameMetadata | null;
  clips: ClipMetadata[];
  clip_count: number;
}

export interface ClipVaultPage {
  groups: ClipVaultGameGroup[];
  next_cursor: string | null;
  skipped_item_count: number;
}

export interface StorageStats {
  total_games: number;
  total_clips: number;
  /** clips 테이블에 등록된 파일들의 합산 크기 (기존 의미 유지) */
  total_size_bytes: number;
  /** recordings/ 디렉토리(순환 버퍼 세그먼트 + wav 포함) 실사용량 */
  recordings_dir_size_bytes?: number;
  /** exports/ 디렉토리(자동편집 결과물) 실사용량 */
  exports_dir_size_bytes?: number;
  /** recordings + exports 실사용량 합계 */
  total_disk_usage_bytes?: number;
}
