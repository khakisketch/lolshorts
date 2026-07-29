// src/types/storage.ts
// Aligned with Rust backend: src-tauri/src/storage/models.rs

export type GameResult = 'Win' | 'Loss' | 'Remake';

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
  | 'champion_kill'
  | 'turret_kill'
  | 'inhibitor_kill'
  | 'dragon_kill'
  | 'baron_kill'
  | 'ace'
  | 'first_blood'
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

export interface ClipMetadata {
  file_path: string;
  thumbnail_path?: string | null;
  event_type: EventType;
  event_time: number;
  priority: number;
  duration: number;
  created_at: string;
  usage_count?: number;
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
