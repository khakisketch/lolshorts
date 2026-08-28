import { cmd } from "./client";

export interface MatchInfo {
  game_id: number;
  game_mode: string;
  game_creation: number;
  game_duration: number;
  champion_id: number;
  win: boolean;
  kills: number;
  deaths: number;
  assists: number;
}

export interface PlayerInfo {
  summoner_name: string;
  champion_id: number;
  team_id: number;
}

export interface GameInfo {
  game_id: string;
  champion: string;
  game_mode: string;
  game_time: number;
}

export type ReplayAvailability =
  | "checking"
  | "notDownloaded"
  | "downloading"
  | "ready"
  | "unknown";

/** Normalize the LCU endpoint's version-dependent free-form replay status. */
export const normalizeReplayStatus = (status: string): ReplayAvailability => {
  const normalized = status.toLowerCase().replace(/[^a-z]/g, "");
  if (normalized.includes("notdownload") || normalized.includes("missing")) {
    return "notDownloaded";
  }
  if (
    normalized.includes("complete") ||
    normalized.includes("ready") ||
    normalized.includes("watch")
  ) {
    return "ready";
  }
  if (normalized.includes("download") || normalized.includes("progress")) {
    return "downloading";
  }
  return "unknown";
};

/**
 * Unified game status from backend game_monitor
 * This is the single source of truth for game detection
 * Uses Live Client API (port 2999) for reliable detection including practice mode
 */
export interface UnifiedGameStatus {
  lcu_connected: boolean;
  in_game: boolean;
  game_mode: "Live" | "TFT" | { Replay: string | null };
  summoner_name: string | null;
  champion_name: string | null;
  game_time: number | null;
  is_monitoring: boolean;
  is_recording: boolean;
  /** Clips saved during the current game session (used for the game-end notification). */
  session_clip_count: number;
}

export const lcuApi = {
  connect: () => cmd<boolean>("connect_lcu"),

  checkStatus: () => cmd<boolean>("check_lcu_status"),

  getCurrentGame: () => cmd<GameInfo | null>("get_current_game"),

  /**
   * Get unified game status - single source of truth for game detection
   * Uses Live Client API for reliable detection (works with practice mode, custom games)
   */
  getUnifiedGameStatus: () => cmd<UnifiedGameStatus>("get_unified_game_status"),

  listMatchHistory: (beginIndex: number = 0, endIndex: number = 20) =>
    cmd<MatchInfo[]>("list_match_history", {
      begin_index: beginIndex,
      end_index: endIndex,
    }),

  downloadReplay: (gameId: number) =>
    cmd<void>("download_replay", { game_id: gameId }),

  getReplayStatus: (gameId: number) =>
    cmd<string>("get_replay_status", { game_id: gameId }),

  launchReplay: (gameId: number) =>
    cmd<void>("launch_replay", { game_id: gameId }),

  getGameParticipants: () => cmd<PlayerInfo[]>("get_game_participants"),
};
