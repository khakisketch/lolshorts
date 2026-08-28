import { clipLabel } from "./clipLabel";
import type { ClipMetadata } from "@/types/storage";

function clip(overrides: Partial<ClipMetadata> = {}): ClipMetadata {
  return {
    file_path: "C:/clips/a.mp4",
    thumbnail_path: null,
    event_type: "champion_kill",
    event_time: 600,
    priority: 1,
    duration: 13,
    created_at: "2026-07-30T00:00:00Z",
    usage_count: 0,
    ...overrides,
  };
}

describe("clipLabel", () => {
  it("제목과 이유를 함께 낸다", () => {
    const { title, reasons } = clipLabel(
      clip({
        event_type: { multikill: 5 },
        score_reasons: [{ Clutch: 8 }, "Solo"],
      }),
    );

    expect(title.key).toBe("events.multikill.penta");
    expect(reasons.map((r) => r.key)).toEqual([
      "clip.reason.clutch",
      "clip.reason.solo",
    ]);
  });

  /**
   * 「1대3 아웃플레이」와 「1대4」는 서로 다른 것을 센다 — 전자는 내가 잡은 수,
   * 후자는 그때 살아 있던 수. 나란히 놓으면 사용자가 하나를 틀린 값으로 읽는다.
   */
  it("제목이 1vX 면 이유에서 수적열세를 뺀다", () => {
    const { title, reasons } = clipLabel(
      clip({
        event_type: { custom: "Outplay1v3" },
        score_reasons: [{ Clutch: 8 }, { Outnumbered: [1, 4] }, "Solo"],
      }),
    );

    expect(title.key).toBe("events.outplay");
    expect(title.params).toEqual({ count: 3 });
    expect(reasons.map((r) => r.key)).toEqual([
      "clip.reason.clutch",
      "clip.reason.solo",
    ]);
    expect(reasons.some((r) => r.key === "clip.reason.outnumbered")).toBe(
      false,
    );
  });

  it("제목이 1vX 가 아니면 수적열세를 그대로 보여준다", () => {
    const { reasons } = clipLabel(
      clip({
        event_type: "champion_kill",
        score_reasons: [{ Outnumbered: [2, 5] }],
      }),
    );
    expect(reasons.map((r) => r.key)).toEqual(["clip.reason.outnumbered"]);
  });

  it("이유가 없으면 빈 배열 — 줄 자체를 그리지 않게", () => {
    expect(clipLabel(clip({ score_reasons: [] })).reasons).toEqual([]);
    expect(clipLabel(clip()).reasons).toEqual([]);
  });

  it("이유는 최대 세 개", () => {
    const { reasons } = clipLabel(
      clip({
        score_reasons: [
          { Clutch: 8 },
          { Outnumbered: [1, 3] },
          "Solo",
          "MatchPoint",
          "LateGame",
        ],
      }),
    );
    expect(reasons).toHaveLength(3);
  });

  it("모르는 이벤트도 이유는 살린다", () => {
    // 백엔드가 새 변형을 냈을 때 제목은 폴백으로 가더라도 이유가 사라지면 안 된다.
    const { title, reasons } = clipLabel(
      clip({ event_type: { custom: "SomethingNew" }, score_reasons: ["Solo"] }),
    );
    expect(title.key).toBe("events.custom");
    expect(reasons.map((r) => r.key)).toEqual(["clip.reason.solo"]);
  });
});
