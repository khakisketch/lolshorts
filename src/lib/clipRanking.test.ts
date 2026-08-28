import fs from "fs";
import path from "path";
import { effectiveScore, PRIORITY_TO_SCORE, rankClips } from "./clipRanking";
import type { ClipMetadata } from "@/types/storage";

const COMPOSER_RS = path.resolve(
  __dirname,
  "../../src-tauri/src/video/auto_composer/composer.rs",
);

function readRustSource(filePath: string): string {
  return fs.readFileSync(filePath, "utf8").replace(/\r\n/g, "\n");
}

function clip(overrides: Partial<ClipMetadata> = {}): ClipMetadata {
  return {
    file_path: "C:/clips/a.mp4",
    thumbnail_path: null,
    event_type: "champion_kill",
    event_time: 100,
    priority: 1,
    duration: 13,
    created_at: "2026-07-30T00:00:00Z",
    usage_count: 0,
    ...overrides,
  };
}

/**
 * 폴백 눈금이 백엔드와 어긋나면 두 세대의 클립이 섞였을 때 순서가 뒤집힌다.
 * 화면이 "최고의 순간" 이라고 띄운 것이 자동편집이 고르는 것과 달라진다.
 */
describe("clipRanking (backend mirror)", () => {
  it("예전 클립 폴백 눈금이 composer.rs 와 같다", () => {
    const source = readRustSource(COMPOSER_RS);
    const match =
      /highlight_score\.unwrap_or\(\(c\.priority as f64\) \* ([\d.]+)\)/.exec(
        source,
      );

    expect(match).not.toBeNull();
    expect(Number(match![1])).toBe(PRIORITY_TO_SCORE);
  });

  it("컴포저는 재사용 감쇠를 쓰지만 홈은 쓰지 않는다", () => {
    // 이 단언은 "감쇠가 백엔드에 존재한다" 는 사실을 고정한다. 홈이 그걸
    // 일부러 빼고 있으므로, 누군가 감쇠를 지우면 이 주석이 가리키는 근거가
    // 사라진 것이라 여기서 먼저 알아야 한다.
    const source = readRustSource(COMPOSER_RS);
    expect(source).toContain("REUSE_DECAY");

    const used = clip({ highlight_score: 100, usage_count: 3 });
    const fresh = clip({ file_path: "C:/clips/b.mp4", highlight_score: 90 });

    // 감쇠를 넣었다면 100 * 0.6^3 = 21.6 이라 fresh 가 이겼을 것이다.
    expect(rankClips([fresh, used])[0].file_path).toBe(used.file_path);
  });
});

describe("clipRanking (정렬)", () => {
  it("점수 높은 순으로 세운다", () => {
    const ranked = rankClips([
      clip({ file_path: "a", highlight_score: 25 }),
      clip({ file_path: "b", highlight_score: 100 }),
      clip({ file_path: "c", highlight_score: 55 }),
    ]);
    expect(ranked.map((c) => c.file_path)).toEqual(["b", "c", "a"]);
  });

  it("점수가 없는 예전 클립은 priority 로 같은 눈금에 올린다", () => {
    // priority 5 -> 100 이므로 점수 55 인 셧다운보다 위여야 한다.
    expect(effectiveScore(clip({ priority: 5 }))).toBe(100);
    expect(effectiveScore(clip({ priority: 1 }))).toBe(20);

    const ranked = rankClips([
      clip({ file_path: "shutdown", highlight_score: 55 }),
      clip({ file_path: "old-penta", priority: 5 }),
    ]);
    expect(ranked[0].file_path).toBe("old-penta");
  });

  it("동점이면 먼저 일어난 것이 위 — 같은 판을 다시 열어도 순서가 같다", () => {
    const ranked = rankClips([
      clip({ file_path: "late", highlight_score: 25, event_time: 900 }),
      clip({ file_path: "early", highlight_score: 25, event_time: 100 }),
    ]);
    expect(ranked.map((c) => c.file_path)).toEqual(["early", "late"]);

    // 두 번 돌려도 같아야 한다(완전한 순서).
    const again = rankClips(ranked);
    expect(again.map((c) => c.file_path)).toEqual(["early", "late"]);
  });

  it("원본 배열을 건드리지 않는다", () => {
    const input = [
      clip({ file_path: "a", highlight_score: 10 }),
      clip({ file_path: "b", highlight_score: 90 }),
    ];
    rankClips(input);
    expect(input.map((c) => c.file_path)).toEqual(["a", "b"]);
  });

  it("점수가 이상한 값이어도 순서가 무너지지 않는다", () => {
    // 백엔드가 NaN 을 낼 일은 없지만, 그걸로 목록 전체가 깨지면 안 된다.
    const ranked = rankClips([
      clip({ file_path: "nan", highlight_score: NaN }),
      clip({ file_path: "good", highlight_score: 50 }),
    ]);
    expect(ranked[0].file_path).toBe("good");
  });
});
