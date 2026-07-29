/**
 * 클립의 `event_type` 을 게이머가 읽는 말로.
 *
 * 백엔드는 이걸 Rust 의 태그드 enum 으로 보낸다 — 단순 변형은 문자열
 * (`"champion_kill"`), 값을 지닌 변형은 객체(`{ multikill: 3 }`). 화면 여러 곳이
 * 각자 `Object.keys(clip.event_type)[0]` 같은 코드를 써서 `champion_kill` 을 그대로
 * 노출하고 있었으므로(게이머에게 이건 영어 코드값이다) 판정을 여기 한 곳으로 모은다.
 *
 * 번역은 하지 않고 i18n 키만 돌려준다 — 문구는 로케일 파일이 SSOT.
 */

import type { EventType } from "@/types/storage";

export interface EventLabel {
  /** `t()` 에 그대로 넣는 키. */
  key: string;
  /** 보간 값. 없으면 생략. */
  params?: Record<string, string | number>;
  /** 알려진 이름으로 풀리지 않은 경우(백엔드가 새 변형을 추가했을 때). */
  unknown?: boolean;
}

const SIMPLE_LABELS: Record<string, string> = {
  champion_kill: "championKill",
  turret_kill: "turretKill",
  inhibitor_kill: "inhibitorKill",
  dragon_kill: "dragonKill",
  baron_kill: "baronKill",
  ace: "ace",
  first_blood: "firstBlood",
};

/** 멀티킬은 숫자가 아니라 이름으로 부른다 — "3 멀티킬" 이라고 말하는 사람은 없다. */
const MULTIKILL_NAMES: Record<number, string> = {
  2: "double",
  3: "triple",
  4: "quadra",
  5: "penta",
};

export function eventLabel(eventType: EventType | null | undefined): EventLabel {
  if (eventType == null) {
    return { key: "events.unknown", unknown: true };
  }

  if (typeof eventType === "string") {
    const name = SIMPLE_LABELS[eventType];
    return name
      ? { key: `events.${name}` }
      : // 백엔드가 새 변형을 추가했는데 여기 반영이 안 된 경우. 원문을 그대로
        // 흘리면 화면에 `void_grub_kill` 같은 코드가 뜨므로 일반 명칭으로 받는다.
        { key: "events.unknown", unknown: true };
  }

  if ("multikill" in eventType) {
    const count = eventType.multikill;
    const name = MULTIKILL_NAMES[count];
    return name
      ? { key: `events.multikill.${name}` }
      : { key: "events.multikill.other", params: { count } };
  }

  if ("custom" in eventType) {
    const text = String(eventType.custom).trim();
    return text
      ? { key: "events.custom", params: { name: text } }
      : { key: "events.unknown", unknown: true };
  }

  return { key: "events.unknown", unknown: true };
}

/** `12.4` -> `"12초"` 용 반올림. 0.5초 미만은 1초로 올린다(0초 클립은 없다). */
export function clipSeconds(duration: number): number {
  if (!Number.isFinite(duration) || duration <= 0) return 0;
  return Math.max(1, Math.round(duration));
}
