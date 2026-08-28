import { useRef, useEffect } from "react";
import { UnifiedGameStatus } from "@/api/lcu";
import { notifyGameStarted, notifyGameEnded } from "@/lib/notifications";
import { useRecordingStore } from "@/stores/recordingStore";

/**
 * 게임 상태 변경을 감지하여 데스크톱 알림을 보내는 훅
 * show_notifications 설정을 존중합니다.
 */
export function useGameNotifications(gameStatus: UnifiedGameStatus | null) {
  const prevInGame = useRef<boolean>(false);
  const settings = useRecordingStore((s) => s.settings);

  useEffect(() => {
    if (!gameStatus || !settings.show_notifications) return;

    const currentInGame = gameStatus.in_game;

    // 게임 시작 감지
    if (currentInGame && !prevInGame.current) {
      notifyGameStarted(gameStatus.champion_name ?? undefined);
    }

    // 게임 종료 감지
    if (!currentInGame && prevInGame.current) {
      notifyGameEnded(gameStatus.session_clip_count ?? 0);
    }

    prevInGame.current = currentInGame;
  }, [gameStatus, settings.show_notifications]);
}
