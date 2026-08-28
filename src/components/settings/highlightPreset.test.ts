import fs from "fs";
import path from "path";
import {
  applyHighlightPreset,
  CanonicalEventFilter,
  DEFAULT_HIGHLIGHT_PRESET,
  EVENT_FILTER_DEFAULTS,
  filtersToPreset,
  HIGHLIGHT_PRESET_FILTERS,
  SELECTABLE_HIGHLIGHT_PRESETS,
  SelectableHighlightPreset,
} from "./highlightPreset";

const MODELS_RS = path.resolve(
  __dirname,
  "../../../src-tauri/src/settings/models.rs",
);

/**
 * 프론트 미러가 백엔드와 어긋나면 기본 설정 화면이 조용히 거짓말을 한다
 * (백엔드는 Custom 인데 화면은 "균형"). 그래서 이 파일은 models.rs 를 실제로
 * 읽어 표를 대조한다 — Rust 쪽 기본값/프리셋/필드가 바뀌면 여기서 먼저 깨진다.
 */
describe("highlightPreset (backend mirror)", () => {
  // 줄바꿈을 LF 로 정규화해서 읽는다. 이 저장소는 git 이 체크아웃 시 CRLF 로
  // 바꾸므로 워킹 카피의 줄바꿈이 환경마다 다르고, 아래 `'\n}\n'` 표식이 그걸
  // 그대로 맞으면 코드는 멀쩡한데 테스트만 "블록의 끝을 찾지 못했습니다" 로
  // 죽는다 — 실제로 한 번 그렇게 깨졌다.
  const source = fs.readFileSync(MODELS_RS, "utf8").replace(/\r\n/g, "\n");

  /** `marker` 로 시작하는 블록에서, 들여쓰기 없는 닫는 중괄호까지 잘라낸다. */
  const blockAfter = (marker: string): string => {
    const start = source.indexOf(marker);
    if (start < 0) {
      throw new Error(
        `models.rs 에서 "${marker}" 를 찾지 못했습니다. 백엔드 구조가 바뀌었다면 미러(highlightPreset.ts)도 함께 갱신하세요.`,
      );
    }
    const end = source.indexOf("\n}\n", start);
    if (end < 0) {
      throw new Error(`"${marker}" 블록의 끝을 찾지 못했습니다.`);
    }
    return source.slice(start, end);
  };

  /** `fn name() -> T { value }` 형태의 serde default 값을 읽는다. */
  const resolveDefaultFn = (name: string): boolean | number => {
    const match = new RegExp(
      `fn ${name}\\(\\)\\s*->\\s*\\w+\\s*\\{\\s*([a-z0-9_]+)`,
    ).exec(source);
    if (!match) {
      throw new Error(
        `models.rs 에서 기본값 함수 ${name}() 를 찾지 못했습니다.`,
      );
    }
    return parseRustValue(match[1]);
  };

  function parseRustValue(raw: string): boolean | number {
    const value = raw.trim();
    if (value === "true") return true;
    if (value === "false") return false;
    if (/^\d+$/.test(value)) return Number(value);
    const fnCall = /^([a-z0-9_]+)\(\)$/.exec(value);
    if (fnCall) return resolveDefaultFn(fnCall[1]);
    throw new Error(`해석할 수 없는 Rust 값: ${raw}`);
  }

  /** 블록 안의 `key: value,` 쌍을 모두 뽑는다(주석 줄은 매칭되지 않는다). */
  const parseFields = (block: string): Record<string, boolean | number> => {
    const fields: Record<string, boolean | number> = {};
    const pattern = /^\s*([a-z][a-z0-9_]*):\s*([^,\n]+),/gm;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(block)) !== null) {
      fields[match[1]] = parseRustValue(match[2]);
    }
    return fields;
  };

  const rustDefaults = parseFields(
    blockAfter("impl Default for EventFilterSettings"),
  );

  const rustPresets = ((): Record<string, Record<string, boolean | number>> => {
    const toFilters = blockAfter("    pub fn to_filters(self)");
    const pattern =
      /Self::(Everything|Balanced|BestOnly)\s*=>\s*EventFilterSettings\s*\{([\s\S]*?)\.\.base/g;
    const presets: Record<string, Record<string, boolean | number>> = {};
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(toFilters)) !== null) {
      presets[match[1]] = parseFields(match[2]);
    }
    return presets;
  })();

  const RUST_TO_WIRE: Record<string, SelectableHighlightPreset> = {
    Everything: "everything",
    Balanced: "balanced",
    BestOnly: "best_only",
  };

  it("parses the backend source (guards against a silently useless test)", () => {
    expect(Object.keys(rustDefaults).length).toBeGreaterThan(20);
    expect(Object.keys(rustPresets).sort()).toEqual([
      "Balanced",
      "BestOnly",
      "Everything",
    ]);
  });

  it("mirrors EventFilterSettings::default() exactly", () => {
    expect(rustDefaults).toEqual(EVENT_FILTER_DEFAULTS);
  });

  it("mirrors every HighlightPreset::to_filters() combination", () => {
    for (const [variant, overrides] of Object.entries(rustPresets)) {
      const expected = { ...EVENT_FILTER_DEFAULTS, ...overrides };
      expect({
        variant,
        filters: HIGHLIGHT_PRESET_FILTERS[RUST_TO_WIRE[variant]],
      }).toEqual({ variant, filters: expected });
    }
  });

  it("mirrors the enum variants and the snake_case wire format", () => {
    const enumBlock = blockAfter("pub enum HighlightPreset");
    const variants = Array.from(
      enumBlock.matchAll(/^\s{4}([A-Z][A-Za-z]*),/gm),
    ).map((match) => match[1]);
    expect(variants).toEqual(["Everything", "Balanced", "BestOnly", "Custom"]);

    // 와이어 값은 serde 의 snake_case 변환에 달려 있다.
    const enumStart = source.indexOf("pub enum HighlightPreset");
    expect(source.slice(Math.max(0, enumStart - 200), enumStart)).toContain(
      'rename_all = "snake_case"',
    );

    expect([...SELECTABLE_HIGHLIGHT_PRESETS]).toEqual([
      "everything",
      "balanced",
      "best_only",
    ]);
  });

  it("mirrors the #[default] variant", () => {
    const enumBlock = blockAfter("pub enum HighlightPreset");
    const defaulted = /#\[default\]\s*\n\s*([A-Z][A-Za-z]*)/.exec(enumBlock);
    expect(defaulted?.[1]).toBe("Balanced");
    expect(DEFAULT_HIGHLIGHT_PRESET).toBe("balanced");
  });
});

describe("filtersToPreset", () => {
  it("round-trips every selectable preset", () => {
    for (const preset of SELECTABLE_HIGHLIGHT_PRESETS) {
      expect(filtersToPreset(HIGHLIGHT_PRESET_FILTERS[preset])).toBe(preset);
    }
  });

  it("reports custom when a single toggle deviates", () => {
    const tweaked: CanonicalEventFilter = {
      ...HIGHLIGHT_PRESET_FILTERS.balanced,
      record_deaths: true,
    };
    expect(filtersToPreset(tweaked)).toBe("custom");
  });

  it("reports custom when a hidden (advanced-only) field deviates", () => {
    const tweaked: CanonicalEventFilter = {
      ...HIGHLIGHT_PRESET_FILTERS.balanced,
      contest_window_secs: 25,
    };
    expect(filtersToPreset(tweaked)).toBe("custom");
  });

  it("reports custom for a filter object missing mirrored fields", () => {
    // 구버전 설정 파일처럼 필드가 빠져 있으면 프리셋으로 단정하지 않는다.
    const partial = {
      ...HIGHLIGHT_PRESET_FILTERS.balanced,
    } as Partial<CanonicalEventFilter>;
    delete partial.record_low_hp;
    expect(filtersToPreset(partial)).toBe("custom");
  });

  it("keeps the presets distinct from each other", () => {
    const presets = SELECTABLE_HIGHLIGHT_PRESETS.map((preset) =>
      JSON.stringify(HIGHLIGHT_PRESET_FILTERS[preset]),
    );
    expect(new Set(presets).size).toBe(presets.length);
  });
});

describe("applyHighlightPreset", () => {
  it("produces exactly the canonical combination", () => {
    const current: CanonicalEventFilter = {
      ...EVENT_FILTER_DEFAULTS,
      record_deaths: true,
      min_priority: 5,
    };
    const applied = applyHighlightPreset("best_only", current);
    expect(applied).toEqual(HIGHLIGHT_PRESET_FILTERS.best_only);
    expect(filtersToPreset(applied)).toBe("best_only");
  });

  it("preserves fields the mirror does not know about", () => {
    const current = {
      ...EVENT_FILTER_DEFAULTS,
      record_future_event: true,
    } as CanonicalEventFilter & { record_future_event: boolean };

    const applied = applyHighlightPreset("balanced", current);
    expect(applied.record_future_event).toBe(true);
  });

  it("does not mutate the input", () => {
    const current: CanonicalEventFilter = { ...EVENT_FILTER_DEFAULTS };
    applyHighlightPreset("everything", current);
    expect(current).toEqual(EVENT_FILTER_DEFAULTS);
  });
});
