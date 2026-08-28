import type { ClipMetadata } from "@/types/storage";

/**
 * 클립을 "얼마나 볼 만한가" 순으로 세운다.
 *
 * # 왜 만들었나
 *
 * 홈은 클립을 **만들어진 시각순**으로 깔고 있었다. 그건 "무엇이 내 하이라이트였나"
 * 라는 질문에 아무 답도 하지 않는다 — 마지막에 저장된 어시스트가 그 판의 펜타킬
 * 위에 놓인다. 백엔드는 이미 `highlight_score` 를 계산하는데(체력·수적열세·단독·
 * 시점 배수), 프론트에서 그 값을 읽는 코드가 **한 곳도 없었다.**
 *
 * # 컴포저와 같은 것 / 다른 것
 *
 * 폴백 규칙은 백엔드와 **같아야 한다**(`auto_composer/composer.rs` 의
 * `effective_score`). 예전 클립은 `highlight_score` 가 없어서 `priority × 20` 으로
 * 같은 눈금에 올린다 — 두 세대가 한 목록에 섞여도 순서가 뒤집히지 않게.
 *
 * 반대로 **재사용 감쇠(`REUSE_DECAY^usage_count`)는 여기 넣지 않는다.** 그건
 * "다음 영상에 무엇을 쓸까" 를 정하는 컴포저의 관심사다. 홈이 답하는 질문은
 * "이 판에서 뭐가 좋았나" 이고, 그 답이 내가 자동편집을 몇 번 돌렸는지에 따라
 * 바뀌면 안 된다. 같은 판을 두 번 열었는데 최고의 순간이 바뀌면 그건 결함이다.
 */

/**
 * 예전 클립(`highlight_score` 없음)을 같은 눈금에 올리는 배수.
 *
 * `priority` 5 = 100, 1 = 20 으로 놓으면 점수 눈금(펜타 100 · 킬 25)과 대체로
 * 겹친다. 값이 백엔드와 어긋나면 `clipRanking.test.ts` 가 Rust 소스를 직접 읽어
 * 깨뜨린다.
 */
export const PRIORITY_TO_SCORE = 20;

/** 정렬에 쓰는 점수. 화면에 숫자로 내보내지 않는다 — 사람에게는 이유를 준다. */
export function effectiveScore(clip: ClipMetadata): number {
  const score = clip.highlight_score;
  if (typeof score === "number" && Number.isFinite(score)) {
    return score;
  }
  return (clip.priority ?? 0) * PRIORITY_TO_SCORE;
}

/**
 * 점수 높은 순. 원본 배열은 건드리지 않는다.
 *
 * 동점은 **먼저 일어난 것 먼저** — 같은 판을 다시 열어도 순서가 같아야 한다.
 * `file_path` 까지 내려가는 이유는 이벤트 시각마저 같은 경우(같은 순간에서
 * 갈라진 클립)에도 완전한 순서를 만들기 위해서다.
 */
export function rankClips(clips: readonly ClipMetadata[]): ClipMetadata[] {
  return [...clips].sort((a, b) => {
    const diff = effectiveScore(b) - effectiveScore(a);
    if (diff !== 0) return diff;

    const byTime = (a.event_time ?? 0) - (b.event_time ?? 0);
    if (byTime !== 0) return byTime;

    return String(a.file_path).localeCompare(String(b.file_path));
  });
}
