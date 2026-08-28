import { normalizeReplayStatus } from "./lcu";

describe("normalizeReplayStatus", () => {
  it.each([
    ["NotDownloaded", "notDownloaded"],
    ["downloading", "downloading"],
    ["download-in-progress", "downloading"],
    ["Complete", "ready"],
    ["ready-to-watch", "ready"],
    ["unexpected-state", "unknown"],
  ] as const)("maps %s to %s", (raw, expected) => {
    expect(normalizeReplayStatus(raw)).toBe(expected);
  });
});
