import fs from "fs";
import path from "path";
import { clipSeconds, eventLabel } from "./eventLabel";
import ko from "@/locales/ko/translation.json";

describe("eventLabel", () => {
  it("단순 변형을 사람 말 키로 바꾼다", () => {
    expect(eventLabel("champion_kill")).toEqual({ key: "events.championKill" });
    expect(eventLabel("baron_kill")).toEqual({ key: "events.baronKill" });
    expect(eventLabel("ace")).toEqual({ key: "events.ace" });
    expect(eventLabel("first_blood")).toEqual({ key: "events.firstBlood" });
  });

  it("멀티킬은 숫자가 아니라 이름으로 부른다", () => {
    expect(eventLabel({ multikill: 2 })).toEqual({
      key: "events.multikill.double",
    });
    expect(eventLabel({ multikill: 3 })).toEqual({
      key: "events.multikill.triple",
    });
    expect(eventLabel({ multikill: 4 })).toEqual({
      key: "events.multikill.quadra",
    });
    expect(eventLabel({ multikill: 5 })).toEqual({
      key: "events.multikill.penta",
    });
  });

  it("이름이 없는 멀티킬 수는 숫자로 흘린다", () => {
    expect(eventLabel({ multikill: 6 })).toEqual({
      key: "events.multikill.other",
      params: { count: 6 },
    });
  });

  it("custom 은 그 이름을 쓴다", () => {
    expect(eventLabel({ custom: "수동 저장" })).toEqual({
      key: "events.custom",
      params: { name: "수동 저장" },
    });
  });

  it("모르는 값은 코드값을 노출하지 않고 일반 명칭으로 받는다", () => {
    // 백엔드가 새 변형을 추가했을 때 화면에 `void_grub_kill` 이 뜨면 안 된다.
    const label = eventLabel("void_grub_kill" as never);
    expect(label).toEqual({ key: "events.unknown", unknown: true });
    expect(JSON.stringify(label)).not.toMatch(/void_grub/);
  });

  it("null 과 빈 custom 도 안전하다", () => {
    expect(eventLabel(null).unknown).toBe(true);
    expect(eventLabel(undefined).unknown).toBe(true);
    expect(eventLabel({ custom: "   " }).unknown).toBe(true);
  });

  it("돌려주는 키가 ko 로케일에 전부 존재한다", () => {
    // 키만 돌려주는 설계라, 로케일에 없으면 화면에 키 문자열이 그대로 뜬다.
    const keys = [
      eventLabel("champion_kill"),
      eventLabel("turret_kill"),
      eventLabel("inhibitor_kill"),
      eventLabel("dragon_kill"),
      eventLabel("baron_kill"),
      eventLabel("ace"),
      eventLabel("first_blood"),
      eventLabel({ multikill: 2 }),
      eventLabel({ multikill: 3 }),
      eventLabel({ multikill: 4 }),
      eventLabel({ multikill: 5 }),
      eventLabel({ multikill: 9 }),
      eventLabel({ custom: "x" }),
      eventLabel(null),
    ].map((l) => l.key);

    for (const key of keys) {
      const value = key
        .split(".")
        .reduce<unknown>(
          (node, part) => (node as Record<string, unknown>)?.[part],
          ko,
        );
      expect(typeof value).toBe("string");
    }
  });

  it("ko 와 en 이 같은 키 집합을 가진다", () => {
    const enPath = path.resolve(__dirname, "../locales/en/translation.json");
    const en = JSON.parse(fs.readFileSync(enPath, "utf8"));
    const flatten = (node: unknown, prefix = ""): string[] =>
      typeof node === "object" && node !== null
        ? Object.entries(node).flatMap(([k, v]) =>
            flatten(v, prefix ? `${prefix}.${k}` : k),
          )
        : [prefix];
    expect(flatten((en as Record<string, unknown>).events).sort()).toEqual(
      flatten((ko as Record<string, unknown>).events).sort(),
    );
  });
});

/**
 * 백엔드가 트리거를 늘리면 화면에 **코드값이 그대로** 나간다.
 *
 * `EventType` 열거형은 변형이 아홉 개뿐이라 나머지 트리거는 전부
 * `Custom("이름")` 에 영어 식별자로 실려 온다. 여기 매핑이 없으면
 * `events.custom` 폴백을 타서 한국어 UI 에 "Shutdown" 이 뜬다 — 실제로 한 번
 * 그렇게 나갔고, 그래서 매핑이 생겼다. 그 사고가 반복되지 않도록 Rust 원문을
 * 읽어 대조한다(문구 자체가 아니라 **이름 집합**을 고정한다).
 */
describe("eventLabel (backend mirror)", () => {
  const AUTO_CLIP_MANAGER_RS = path.resolve(
    __dirname,
    "../../src-tauri/src/recording/auto_clip_manager.rs",
  );

  it("백엔드가 내보내는 Custom 트리거 이름이 전부 사람 말로 매핑돼 있다", () => {
    const source = fs
      .readFileSync(AUTO_CLIP_MANAGER_RS, "utf8")
      .replace(/\r\n/g, "\n");

    // `EventTrigger::X => EventType::Custom("Name".to_string())`
    const names = [
      ...source.matchAll(/EventType::Custom\("([A-Za-z]+)"\.to_string\(\)\)/g),
    ].map((m) => m[1]);

    // 매핑을 찾지 못했다면 정규식이 낡은 것이지 결함이 없는 게 아니다.
    expect(names.length).toBeGreaterThan(5);

    for (const name of names) {
      const label = eventLabel({ custom: name });
      expect({ name, key: label.key }).not.toEqual({
        name,
        key: "events.custom",
      });
    }
  });

  it("숫자가 박히는 1vX 아웃플레이도 코드값으로 새지 않는다", () => {
    // `format!("Outplay1v{}", n)` — 정확 매칭이 안 되므로 접두사로 받는다.
    for (const n of [2, 3, 4, 5]) {
      expect(eventLabel({ custom: `Outplay1v${n}` })).toEqual({
        key: "events.outplay",
        params: { count: n },
      });
    }
  });
});

describe("clipSeconds", () => {
  it("초를 반올림한다", () => {
    expect(clipSeconds(12.4)).toBe(12);
    expect(clipSeconds(12.6)).toBe(13);
  });

  it("0초 클립은 만들지 않는다 — 아주 짧아도 1초로 보인다", () => {
    expect(clipSeconds(0.3)).toBe(1);
  });

  it("이상한 값은 0 으로 떨어뜨린다", () => {
    expect(clipSeconds(0)).toBe(0);
    expect(clipSeconds(-5)).toBe(0);
    expect(clipSeconds(NaN)).toBe(0);
  });
});
