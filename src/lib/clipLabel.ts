import { eventLabel, type EventLabel } from "@/lib/eventLabel";
import { reasonLabels, type ReasonLabel } from "@/lib/scoreReason";
import type { ClipMetadata } from "@/types/storage";

/**
 * 클립 하나가 화면에 말하는 것 전부 — **제목과 이유를 함께** 만든다.
 *
 * # 왜 따로가 아니라 함께인가
 *
 * 둘을 따로 만들면 같은 사실이 두 번, 그것도 **다른 숫자로** 나온다.
 *
 * 「1대3」을 만들 수 있는 경로가 백엔드에 둘 있는데 서로 다른 것을 센다:
 *
 * - `EventType::Custom("Outplay1v3")` — 10초 안에 내가 **잡은** 고유 피해자 수
 *   (`live_client.rs` 의 `recent_solo_kills`)
 * - `ScoreReason::Outnumbered(1, 4)` — 그 순간 **살아 있던** 양 팀 인원
 *   (`capture_moment` 의 `allies_alive`/`enemies_alive`)
 *
 * 그래서 제목이 「1대3 아웃플레이」인데 이유에 「1대4」가 붙는 조합이 실제로
 * 가능하다. 나란히 놓으면 사용자는 둘 중 하나를 틀린 값으로 읽는다.
 *
 * 규칙: **제목이 이미 1vX 를 말하고 있으면 이유에서 `Outnumbered` 를 뺀다.**
 * 제목이 더 구체적이고(내가 실제로 해낸 것), 이유는 배경일 뿐이다.
 *
 * 같은 문제를 백엔드 훅 자막도 갖는다 — `auto_composer/caption.rs` 의
 * `clip_caption` 이 같은 규칙을 적용한다. 한쪽만 고치면 화면과 영상이 다른
 * 말을 하게 되므로 둘을 함께 본다.
 */

/** 제목이 1vX 아웃플레이인지 — `eventLabel` 이 그때만 이 키를 낸다. */
const OUTPLAY_KEY = "events.outplay";

/** 카드 한 줄에 들어가는 이유 개수. 한글 기준 셋을 넘으면 잘리기 시작한다. */
const MAX_REASONS = 3;

export interface ClipLabel {
  /** 굵게 나갈 첫 줄 — 무슨 장면인가. */
  title: EventLabel;
  /** 그 아래 작게 — 왜 볼 만한가. 없으면 빈 배열(줄 자체를 그리지 않는다). */
  reasons: ReasonLabel[];
}

export function clipLabel(clip: ClipMetadata): ClipLabel {
  const title = eventLabel(clip.event_type);
  const reasons = reasonLabels(clip.score_reasons, MAX_REASONS);

  if (title.key !== OUTPLAY_KEY) {
    return { title, reasons };
  }

  // 제목이 「1대3 아웃플레이」면 「1대4」를 또 붙이지 않는다.
  return {
    title,
    reasons: reasons.filter((r) => r.key !== "clip.reason.outnumbered"),
  };
}
