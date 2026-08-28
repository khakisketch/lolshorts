import type { GameResult } from "@/types/storage";

/**
 * 승/패 색 규칙 한 곳.
 *
 * `Games.tsx` 와 `Replays.tsx` 가 각자 로컬 함수로 같은 규칙을 들고 있었다.
 * 홈에도 판 헤더가 생기면서 세 번째 사본이 될 참이라 여기로 모은다 — 색이
 * 화면마다 달라지면 "이겼는데 왜 다른 색이지" 가 된다.
 *
 * 청록(승) / 마젠타(패)는 앱 전체의 강조색 축과 같다.
 */
export type GameResultTone = "win" | "loss" | "neutral";

export function resultTone(
  result: GameResult | null | undefined,
): GameResultTone {
  if (result === "Win") return "win";
  if (result === "Loss") return "loss";
  // `Remake` 와 아직 회수하지 못한 경우(`null`)는 같은 취급 — 둘 다 자랑도
  // 반성도 아니다.
  return "neutral";
}

/** 배지처럼 테두리·배경까지 칠할 때. */
export const RESULT_BADGE_CLASS: Record<GameResultTone, string> = {
  win: "text-gaming-cyan border-gaming-cyan/40 bg-gaming-cyan/10",
  loss: "text-gaming-magenta border-gaming-magenta/40 bg-gaming-magenta/10",
  neutral: "text-muted-foreground border-white/20 bg-white/5",
};

/** 글자만 칠할 때. */
export const RESULT_TEXT_CLASS: Record<GameResultTone, string> = {
  win: "text-gaming-cyan",
  loss: "text-gaming-magenta",
  neutral: "text-muted-foreground",
};

/** 승/패/재경기 i18n 키. 문구는 로케일 파일이 SSOT. */
export const RESULT_LABEL_KEY: Record<GameResultTone, string> = {
  win: "game.result.win",
  loss: "game.result.loss",
  neutral: "game.result.unknown",
};
