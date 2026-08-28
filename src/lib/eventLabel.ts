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

/**
 * `Custom("...")` 로 오는 트리거 이름 → i18n 키.
 *
 * `EventType` 열거형에는 변형이 아홉 개뿐이라, 감지가 가르는 나머지 트리거는
 * 전부 `Custom` 에 이름 문자열로 실려 온다(`trigger_to_event_type`). 그 문자열은
 * `"Shutdown"` 같은 영어 식별자라서, 매핑이 없으면 `events.custom` 을 타고 화면에
 * **코드값 그대로** 나간다 — 한국어 UI 에서 클립 이름이 "Shutdown" 이 된다.
 *
 * 백엔드 열거형을 늘리는 대신 여기서 받는 이유는, 이 이름들이 저장된 클립에도
 * 이미 그 문자열로 들어가 있어서 어차피 화면이 알아들어야 하기 때문이다.
 */
const CUSTOM_LABELS: Record<string, string> = {
  Shutdown: "shutdown",
  Death: "death",
  FirstBloodVictim: "firstBloodVictim",
  Assist: "assist",
  Steal: "steal",
  GameEnd: "gameEnd",
  HeraldKill: "heraldKill",
  ElderDragonKill: "elderDragonKill",
  VoidgrubsKill: "voidgrubsKill",
  AtakhanKill: "atakhanKill",
  TradeKill: "tradeKill",
  LowHpOutplay: "lowHpOutplay",
  ManualReplay: "manualReplay",
  ManualSave: "manualReplay",
};

/**
 * 1vX 아웃플레이는 인원수가 이름에 박혀서 온다(`Outplay1v3`). 정확 매칭이 안 되므로
 * 접두사로 받고 숫자를 보간 값으로 넘긴다.
 */
const OUTPLAY_PATTERN = /^Outplay1v(\d+)$/;

/** 멀티킬은 숫자가 아니라 이름으로 부른다 — "3 멀티킬" 이라고 말하는 사람은 없다. */
const MULTIKILL_NAMES: Record<number, string> = {
  2: "double",
  3: "triple",
  4: "quadra",
  5: "penta",
};

export function eventLabel(
  eventType: EventType | null | undefined,
): EventLabel {
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
    if (!text) return { key: "events.unknown", unknown: true };

    const known = CUSTOM_LABELS[text];
    if (known) return { key: `events.${known}` };

    const outplay = OUTPLAY_PATTERN.exec(text);
    if (outplay) {
      return {
        key: "events.outplay",
        params: { count: Number(outplay[1]) },
      };
    }

    // 매핑에 없는 이름은 원문 그대로 흘린다. 영어 코드가 보이는 편이 "하이라이트"
    // 라고 뭉뚱그리는 것보다 낫다 — 무엇이 빠졌는지 화면에서 바로 드러난다.
    return { key: "events.custom", params: { name: text } };
  }

  return { key: "events.unknown", unknown: true };
}

/** `12.4` -> `"12초"` 용 반올림. 0.5초 미만은 1초로 올린다(0초 클립은 없다). */
export function clipSeconds(duration: number): number {
  if (!Number.isFinite(duration) || duration <= 0) return 0;
  return Math.max(1, Math.round(duration));
}
