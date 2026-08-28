import fs from "fs";
import path from "path";
import { reasonLabel, reasonLabels } from "./scoreReason";
import type { ScoreReason } from "@/types/storage";
import ko from "@/locales/ko/translation.json";
import en from "@/locales/en/translation.json";

const HIGHLIGHT_SCORE_RS = path.resolve(
  __dirname,
  "../../src-tauri/src/recording/highlight_score.rs",
);

/** `pub enum ScoreReason { ... }` 블록에서 변형 이름을 뽑는다. */
function declaredVariants(): string[] {
  const source = fs.readFileSync(HIGHLIGHT_SCORE_RS, "utf8");
  const start = source.indexOf("pub enum ScoreReason {");
  expect(start).toBeGreaterThan(-1);
  const block = source.slice(start, source.indexOf("\n}", start));

  const names: string[] = [];
  // 주석(`///`)은 건너뛰고 `    Variant` 또는 `    Variant(...)` 줄만.
  for (const line of block.split("\n").slice(1)) {
    const m = /^\s{4}([A-Z]\w*)\s*(\(|,)/.exec(line);
    if (m) names.push(m[1]);
  }
  return names;
}

/** 그 변형이 값을 몇 개 싣는가 — 와이어 모양이 문자열인지 객체인지 가른다. */
function payloadArity(variant: string): number {
  const source = fs.readFileSync(HIGHLIGHT_SCORE_RS, "utf8");
  const start = source.indexOf("pub enum ScoreReason {");
  const block = source.slice(start, source.indexOf("\n}", start));
  const m = new RegExp(`^\\s{4}${variant}\\((.*?)\\)`, "m").exec(block);
  if (!m) return 0;
  return m[1].split(",").filter((s) => s.trim().length > 0).length;
}

/** 변형 이름 -> 실제로 서버가 보낼 모양의 값. */
function sampleFor(variant: string): ScoreReason {
  const arity = payloadArity(variant);
  if (arity === 0) return variant as ScoreReason;
  if (arity === 1) return { [variant]: 8 } as unknown as ScoreReason;
  return { [variant]: [1, 3] } as unknown as ScoreReason;
}

/**
 * 이 화면은 한 번 코드값을 흘린 전력이 있다 — 클립 이름이 한국어 UI 에
 * `Shutdown` 으로 나갔다. 점수 이유도 같은 경로(백엔드 enum -> 화면)를 타므로,
 * 변형이 늘어나는 순간을 테스트가 먼저 잡아야 한다.
 */
describe("scoreReason (backend mirror)", () => {
  it("Rust 의 ScoreReason 변형을 전부 사람 말로 옮길 수 있다", () => {
    const variants = declaredVariants();
    // 표를 실제로 읽어왔는지부터 확인 — 정규식이 빗나가면 0개가 되어 통과해 버린다.
    expect(variants).toContain("Solo");
    expect(variants.length).toBeGreaterThanOrEqual(5);

    const missing = variants.filter((v) => reasonLabel(sampleFor(v)) === null);
    expect(missing).toEqual([]);
  });

  it("옮긴 키가 ko/en 두 로케일에 모두 있다", () => {
    // 키가 없으면 i18next 는 키 문자열을 그대로 렌더한다 — 화면에
    // `clip.reason.clutch` 가 보이는 것은 코드값 노출과 같은 결함이다.
    const lookup = (dict: unknown, key: string) =>
      key
        .split(".")
        .reduce<unknown>(
          (acc, part) =>
            acc && typeof acc === "object"
              ? (acc as Record<string, unknown>)[part]
              : undefined,
          dict,
        );

    for (const variant of declaredVariants()) {
      const label = reasonLabel(sampleFor(variant));
      expect(label).not.toBeNull();
      expect(typeof lookup(ko, label!.key)).toBe("string");
      expect(typeof lookup(en, label!.key)).toBe("string");
    }
  });
});

describe("scoreReason (표시 규칙)", () => {
  it("값 있는 변형은 보간 값을 함께 넘긴다", () => {
    expect(reasonLabel({ Clutch: 8 })).toEqual({
      key: "clip.reason.clutch",
      params: { percent: 8 },
    });
    expect(reasonLabel({ Outnumbered: [1, 3] })).toEqual({
      key: "clip.reason.outnumbered",
      params: { allies: 1, enemies: 3 },
    });
  });

  it("모르는 모양은 화면에 흘리지 않는다", () => {
    // 백엔드가 변형을 늘렸는데 여기 매핑이 없으면, 코드값을 보여주느니 감춘다.
    // (위 미러 테스트가 그 상황 자체를 먼저 깨뜨린다.)
    expect(reasonLabel("Unheard" as unknown as ScoreReason)).toBeNull();
    expect(reasonLabel({ Clutch: NaN } as unknown as ScoreReason)).toBeNull();
    expect(
      reasonLabel({ Outnumbered: "nope" } as unknown as ScoreReason),
    ).toBeNull();
  });

  it("눈에 띄는 이유가 먼저 나온다", () => {
    // 카드 한 줄에 다 안 들어가면 뒤가 잘리므로 "체력 8%" 가 "후반전" 보다 앞.
    const labels = reasonLabels(["LateGame", "Solo", { Clutch: 8 }]);
    expect(labels.map((l) => l.key)).toEqual([
      "clip.reason.clutch",
      "clip.reason.solo",
      "clip.reason.lateGame",
    ]);
  });

  it("카드 한 줄 예산을 넘기지 않는다", () => {
    const labels = reasonLabels([
      { Clutch: 8 },
      { Outnumbered: [1, 3] },
      "Solo",
      "MatchPoint",
      "LateGame",
    ]);
    expect(labels).toHaveLength(3);
  });

  it("이유가 없으면 빈 목록이다", () => {
    expect(reasonLabels([])).toEqual([]);
    expect(reasonLabels(undefined)).toEqual([]);
    expect(reasonLabels(null)).toEqual([]);
  });
});
