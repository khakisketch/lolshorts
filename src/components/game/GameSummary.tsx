import { useTranslation } from "react-i18next";
import {
  RESULT_BADGE_CLASS,
  RESULT_LABEL_KEY,
  resultTone,
} from "@/lib/gameResult";
import type { GameMetadata } from "@/types/storage";

interface GameSummaryProps {
  game: GameMetadata;
  testId?: string;
}

/**
 * 판 한 줄 — 챔피언 · 승패 · KDA.
 *
 * 홈이 "이번 판에서 내가 뭘 잘했나" 를 답하는 화면이 되려면 **어느 판인지**부터
 * 말해야 한다. 지금까지 홈은 `getGameMetadata` 를 아예 부르지 않아서 클립만
 * 덩그러니 놓여 있었다.
 *
 * 이 컴포넌트는 **없어도 되는 것**이다 — 메타데이터를 못 얻어도 클립은 보여야
 * 하므로, 부르는 쪽이 `game` 을 못 구하면 그냥 렌더하지 않는다. 헤더 하나 때문에
 * 화면 전체가 비는 일은 없어야 한다.
 */
export function GameSummary({
  game,
  testId = "game-summary",
}: GameSummaryProps) {
  const { t } = useTranslation();
  const tone = resultTone(game.result);
  const kda = game.kda;

  return (
    <div
      data-testid={testId}
      className="flex flex-wrap items-center gap-x-3 gap-y-1.5"
    >
      <h1
        className="text-lg font-semibold"
        style={{ wordBreak: "keep-all" }}
        data-autofocus
        tabIndex={-1}
      >
        {game.champion}
      </h1>

      <span
        className={[
          "rounded border px-2 py-0.5 text-xs font-bold",
          RESULT_BADGE_CLASS[tone],
        ].join(" ")}
      >
        {t(RESULT_LABEL_KEY[tone])}
      </span>

      {kda && (
        <span className="text-sm tabular-nums text-muted-foreground">
          {/* 슬래시로 잇는다 — 롤 사용자가 KDA 를 읽는 관습 그대로. */}
          {kda.kills} / {kda.deaths} / {kda.assists}
        </span>
      )}
    </div>
  );
}
