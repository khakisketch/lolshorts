/**
 * IPC argument-casing contract tests.
 *
 * Tauri v2 derives a command's argument keys from the Rust parameter names with
 * heck's `to_lower_camel_case` (tauri-macros' default `ArgumentCase::Camel`) and
 * resolves them with an exact `v.get(key)` — no snake_case fallback. The frontend
 * writes snake_case, so `cmd()` converts on the way out via `toIpcArgs`.
 *
 * Nothing else in the suite covers this boundary: `jest.setup.js` mocks
 * `src/api/client` wholesale, and every other test mocks `cmd` or `invoke` above
 * the conversion — which is exactly how the original mismatch stayed green.
 */
import {
  snakeKeyToCamel,
  isSupportedArgKey,
  toIpcArgs,
  type IpcArgWarning,
} from "./ipcArgs";

describe("snakeKeyToCamel", () => {
  it.each([
    ["game_id", "gameId"],
    ["clip_file_path", "clipFilePath"],
    ["duration_secs", "durationSecs"],
    ["privacy_status", "privacyStatus"],
    ["h264_level", "h264Level"],
    ["_force_refresh", "forceRefresh"],
    ["a__b", "aB"],
    ["title", "title"],
  ])("converts %s -> %s", (input, expected) => {
    expect(snakeKeyToCamel(input)).toBe(expected);
  });

  it("is idempotent on keys that are already lowerCamelCase", () => {
    for (const key of ["gameId", "summonerName", "outputPath"]) {
      expect(snakeKeyToCamel(key)).toBe(key);
      expect(snakeKeyToCamel(snakeKeyToCamel(key))).toBe(key);
    }
  });
});

describe("isSupportedArgKey", () => {
  it("accepts lowercase snake_case and lowerCamelCase", () => {
    for (const key of [
      "game_id",
      "_force_refresh",
      "h264_level",
      "gameId",
      "title",
    ]) {
      expect(isSupportedArgKey(key)).toBe(true);
    }
  });

  it("rejects keys that mix case with underscores, where heck diverges", () => {
    // heck lowercases each word's remainder: video_URL -> videoUrl, Game_id -> gameId,
    // whereas this converter would emit videoURL / GameId and miss on the Rust side.
    for (const key of ["video_URL", "Game_id", "CLIP_PATH"]) {
      expect(isSupportedArgKey(key)).toBe(false);
    }
  });

  it("passes through underscore-free keys, which are whatever the author typed", () => {
    // `videoID` is indistinguishable from a deliberate key here (heck output can
    // legitimately contain single-letter words, e.g. a_b -> aB), so this check does
    // not pretend to catch it — it is an ordinary typo, not a conversion divergence.
    expect(isSupportedArgKey("videoID")).toBe(true);
  });
});

describe("toIpcArgs", () => {
  it("converts every top-level key", () => {
    expect(
      toIpcArgs({
        video_path: "a.mp4",
        privacy_status: "private",
        thumbnail_path: "b.png",
        title: "t",
      }),
    ).toEqual({
      videoPath: "a.mp4",
      privacyStatus: "private",
      thumbnailPath: "b.png",
      title: "t",
    });
  });

  it("does NOT convert nested object keys (they follow Rust serde, i.e. snake_case)", () => {
    const config = {
      game_ids: ["g1"],
      target_duration: 60,
      audio_levels: { game_audio: 60, background_music: 80 },
    };
    expect(toIpcArgs({ config })).toEqual({ config });
  });

  it("does NOT convert keys inside arrays", () => {
    expect(
      toIpcArgs({
        clips: [{ file_path: "a.mp4", trim_start: 1 }],
        output_path: "out.mp4",
      }),
    ).toEqual({
      clips: [{ file_path: "a.mp4", trim_start: 1 }],
      outputPath: "out.mp4",
    });
  });

  it("normalizes undefined and null to undefined so invoke applies its own default", () => {
    expect(toIpcArgs(undefined)).toBeUndefined();
    expect(toIpcArgs(null)).toBeUndefined();
  });

  it("preserves values by reference", () => {
    const clips = [{ file_path: "a.mp4" }];
    const out = toIpcArgs({ clip_paths: clips, output_path: null })!;
    expect(out.clipPaths).toBe(clips);
    expect(out.outputPath).toBeNull();
  });

  it("reports keys it cannot convert faithfully", () => {
    const warnings: IpcArgWarning[] = [];
    toIpcArgs({ video_URL: "v" }, (w) => warnings.push(w));
    expect(warnings).toEqual([{ kind: "unsupported-key", key: "video_URL" }]);
  });

  it("reports collisions instead of silently dropping a value", () => {
    const warnings: IpcArgWarning[] = [];
    const out = toIpcArgs({ force_refresh: 1, forceRefresh: 2 }, (w) =>
      warnings.push(w),
    );
    expect(warnings).toContainEqual({
      kind: "key-collision",
      key: "forceRefresh",
      camel: "forceRefresh",
    });
    expect(out).toEqual({ forceRefresh: 2 });
  });

  it("stays silent for well-formed args", () => {
    const warnings: IpcArgWarning[] = [];
    toIpcArgs({ game_id: "g", clip_file_path: "c.mp4" }, (w) =>
      warnings.push(w),
    );
    expect(warnings).toEqual([]);
  });
});
