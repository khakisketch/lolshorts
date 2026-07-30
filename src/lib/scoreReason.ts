import type { ScoreReason } from "@/types/storage";

/**
 * 하이라이트 점수의 **이유**를 사람 말로 옮긴다.
 *
 * # 왜 이유만 보여주나
 *
 * 점수 자체(`highlight_score`)는 화면에 내보내지 않는다. "37.5점" 은 게이머에게
 * 아무 뜻이 없고, 눈금을 설명하려 들면 그 순간 이 앱은 정렬 알고리즘을 자랑하는
 * 도구가 된다. 반면 "체력 8% · 1v3" 은 설명이 필요 없다 — 그 자체가 이유다.
 *
 * # 왜 이 값이 우리만의 것인가
 *
 * 화면 픽셀을 읽어 하이라이트를 추정하는 경쟁 서비스는 "체력 8% 였다"를 확언할
 * 수 없다. 우리는 Live Client Data API 로 그 순간의 체력·생존 인원·어시스트 수를
 * 직접 받는다. 그런데 이 값들은 지금까지 저장만 되고 **아무 데도 나오지 않았다** —
 * 앱이 가진 유일한 차별점이 화면에 없었다.
 *
 * i18n 키는 `clip.reason.*`. 문구 SSOT 는 각 로케일의 `translation.json` 이다.
 */

/** i18n 에 넘길 키와 보간 값. 문구 자체는 여기서 만들지 않는다. */
export interface ReasonLabel {
  key: string;
  params?: Record<string, number>;
}

/**
 * 한 이유를 i18n 키로. 모르는 모양이면 `null` — 화면에 코드값을 흘리지 않는다.
 *
 * 와이어 표현은 Rust serde 기본값(외부 태깅)이라 단순 변형은 문자열,
 * 값을 가진 변형은 `{ 변형이름: 값 }` 으로 온다.
 */
export function reasonLabel(reason: ScoreReason): ReasonLabel | null {
  if (reason === "Solo") return { key: "clip.reason.solo" };
  if (reason === "LateGame") return { key: "clip.reason.lateGame" };
  if (reason === "MatchPoint") return { key: "clip.reason.matchPoint" };

  if (typeof reason === "object" && reason !== null) {
    if ("Clutch" in reason && Number.isFinite(reason.Clutch)) {
      return { key: "clip.reason.clutch", params: { percent: reason.Clutch } };
    }
    if ("Outnumbered" in reason && Array.isArray(reason.Outnumbered)) {
      const [allies, enemies] = reason.Outnumbered;
      if (Number.isFinite(allies) && Number.isFinite(enemies)) {
        return { key: "clip.reason.outnumbered", params: { allies, enemies } };
      }
    }
  }

  return null;
}

/**
 * 화면에 나갈 순서 — 가장 눈에 띄는 것부터.
 *
 * 한 클립에 이유가 셋 넘게 붙는 일은 드물지만, 붙는다면 "체력 8%" 가 "후반전"
 * 보다 먼저 나와야 한다. 카드 한 줄에 다 안 들어가면 뒤쪽이 잘리기 때문이다.
 */
const REASON_RANK: Record<string, number> = {
  "clip.reason.clutch": 0,
  "clip.reason.outnumbered": 1,
  "clip.reason.solo": 2,
  "clip.reason.matchPoint": 3,
  "clip.reason.lateGame": 4,
};

/**
 * 클립의 이유들을 화면 순서대로. 모르는 모양은 조용히 버린다.
 *
 * `limit` 은 카드 한 줄 예산이다. 기본 3개를 넘기면 한글 기준으로 카드 폭을
 * 넘어 잘리기 시작한다.
 */
export function reasonLabels(
  reasons: ScoreReason[] | null | undefined,
  limit = 3,
): ReasonLabel[] {
  if (!reasons?.length) return [];

  return reasons
    .map(reasonLabel)
    .filter((label): label is ReasonLabel => label !== null)
    .sort((a, b) => (REASON_RANK[a.key] ?? 99) - (REASON_RANK[b.key] ?? 99))
    .slice(0, limit);
}
