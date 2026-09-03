import fs from "fs";
import path from "path";

const requiredPaths = [
  "results.tabs.clips",
  "results.clips.title",
  "results.clips.sortRecommended",
  "results.clips.sortNewest",
  "results.clips.loadMore",
  "results.clips.skippedItems",
  "results.clips.selectGame",
  "results.clips.clearGameSelection",
  "results.clips.gameRank",
  "results.clips.selectionSummary",
  "results.clips.overTargetWarning",
  "results.clips.createMontage",
  "autoEdit.pinnedClips.summary",
  "autoEdit.pinnedClips.useAutomatic",
  "autoEdit.pinnedClips.overTargetWarning",
  "autoEdit.pinnedClips.overrunTitle",
  "autoEdit.pinnedClips.overrunDescription",
  "autoEdit.pinnedClips.includeAll",
] as const;

function readPath(object: unknown, dottedPath: string): unknown {
  return dottedPath.split(".").reduce<unknown>((current, key) => {
    if (!current || typeof current !== "object") return undefined;
    return (current as Record<string, unknown>)[key];
  }, object);
}

describe("clip vault locale coverage", () => {
  const localeRoot = path.join(process.cwd(), "src", "locales");
  // Only ko/en are human-maintained and user-selectable (see src/i18n.ts, spec
  // G013). The other bundles stay on disk purely as an English fallback, so
  // holding new keys to full parity there is churn with no user benefit.
  const locales = ["en", "ko"];

  it("keeps the new clip-vault and direct-selection keys in the maintained locales", () => {
    for (const locale of locales) {
      const translation = JSON.parse(
        fs.readFileSync(
          path.join(localeRoot, locale, "translation.json"),
          "utf8",
        ),
      ) as unknown;
      for (const key of requiredPaths) {
        expect({ locale, key, value: readPath(translation, key) }).toEqual({
          locale,
          key,
          value: expect.any(String),
        });
      }
    }
  });
});
