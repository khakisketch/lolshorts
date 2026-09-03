import fs from "fs";
import path from "path";

const i18next: typeof import("i18next") = jest.requireActual("i18next");

const workflowKeys = [
  "mediaJobs.title",
  "mediaJobs.queued",
  "mediaJobs.processing",
  "mediaJobs.completed",
  "mediaJobs.failed",
  "mediaJobs.cancelled",
  "mediaJobs.paused",
  "mediaJobs.recoverable",
  "mediaJobs.resume",
  "mediaJobs.discard",
  "mediaJobs.cancel",
  "mediaJobs.retry",
  "mediaJobs.empty",
  "outputValidation.title",
  "outputValidation.duration",
  "outputValidation.clipCount",
  "outputValidation.outputFile",
  "outputValidation.fileSize",
  "outputValidation.verified",
  "outputValidation.valid",
  "outputValidation.warning",
  "outputValidation.invalid",
  "outputValidation.unknown",
  "outputValidation.revalidate",
  "outputValidation.needsReview",
  "outputValidation.missing",
  "resultSeries.title",
  "resultSeries.part",
  "resultSeries.parts",
  "resultSeries.totalDuration",
  "resultSeries.totalSize",
  "resultSeries.deleteGroup",
  "resultSeries.deletePart",
  "resultSeries.sharePart",
  "platformExport.title",
  "platformExport.tiktok",
  "platformExport.instagramReels",
  "platformExport.exporting",
  "platformExport.passthrough",
  "platformExport.convert",
  "platformExport.cancel",
  "platformExport.busy",
  "platformExport.confirmWarning",
  "platformExport.success",
  "platformExport.failed",
  "appUpdater.title",
  "appUpdater.description",
  "appUpdater.releaseNotes",
  "appUpdater.later",
  "appUpdater.install",
  "appUpdater.retry",
  "appUpdater.working",
  "appUpdater.downloading",
  "appUpdater.installing",
  "appUpdater.progressLabel",
  "appUpdater.windowsExitNotice",
  "appUpdater.settingsTitle",
  "appUpdater.currentVersion",
  "appUpdater.upToDate",
  "appUpdater.disabled",
  "appUpdater.check",
  "appUpdater.checking",
  "appUpdater.errors.updater_disabled",
  "appUpdater.errors.update_busy",
  "appUpdater.errors.update_check_failed",
  "appUpdater.errors.update_check_timeout",
  "appUpdater.errors.update_download_timeout",
  "appUpdater.errors.update_signature_invalid",
  "appUpdater.errors.update_install_failed",
  "appUpdater.errors.unknown",
] as const;

// Only ko/en are human-maintained and user-selectable (src/i18n.ts, spec G013).
// The rest resolve through the English fallback.
const coreLocales = ["en", "ko"] as const;
const localeRoot = path.join(process.cwd(), "src", "locales");
const allowedIdenticalStrings = new Set<string>();

function readTranslation(locale: string): Record<string, unknown> {
  return JSON.parse(
    fs.readFileSync(path.join(localeRoot, locale, "translation.json"), "utf8"),
  ) as Record<string, unknown>;
}

describe("media-workflow locale coverage", () => {
  const english = readTranslation("en");
  const locales = fs
    .readdirSync(localeRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  it("resolves every new workflow key in every locale bundle on disk", async () => {
    expect(locales.length).toBeGreaterThanOrEqual(2);

    for (const locale of locales) {
      const instance = i18next.createInstance();
      await instance.init({
        fallbackLng: "en",
        lng: locale,
        resources: {
          en: { translation: english },
          [locale]: { translation: readTranslation(locale) },
        },
      });

      for (const key of workflowKeys) {
        expect(
          instance.t(key, {
            current: 1,
            total: 2,
            count: 2,
            duration: "1:00",
            size: "10 MB",
            part: 1,
            available: "1.3.0",
            version: "1.2.0",
          }),
        ).not.toBe(key);
      }
    }
  });

  it("keeps workflow strings native in the maintained locales", async () => {
    for (const locale of coreLocales.filter((locale) => locale !== "en")) {
      const translation = readTranslation(locale);
      const instance = i18next.createInstance();
      await instance.init({
        lng: locale,
        resources: {
          en: { translation: english },
          [locale]: { translation },
        },
      });

      for (const key of workflowKeys) {
        const localized = instance.t(key, {
          current: 1,
          total: 2,
          count: 2,
          duration: "1:00",
          size: "10 MB",
          part: 1,
          available: "1.3.0",
          version: "1.2.0",
        });
        const englishValue = instance.t(key, {
          lng: "en",
          current: 1,
          total: 2,
          count: 2,
          duration: "1:00",
          size: "10 MB",
          part: 1,
          available: "1.3.0",
          version: "1.2.0",
        });
        expect(
          allowedIdenticalStrings.has(key) || localized !== englishValue,
        ).toBe(true);
      }
    }
  });

  it("does not seed workflow sections into locales that never carried them", () => {
    // ja/zh-CN/es/pt-BR still hold these sections from before the picker was
    // trimmed to ko/en (spec G013); that leftover is harmless. What must not
    // happen is a NEW partial translation appearing in a locale that had none.
    const everCarriedWorkflowKeys = new Set([
      "en",
      "ko",
      "ja",
      "zh-CN",
      "es",
      "pt-BR",
    ]);
    for (const locale of locales.filter(
      (locale) => !everCarriedWorkflowKeys.has(locale),
    )) {
      const translation = readTranslation(locale);
      for (const section of [
        "mediaJobs",
        "outputValidation",
        "resultSeries",
        "platformExport",
        "appUpdater",
      ]) {
        expect(translation).not.toHaveProperty(section);
      }
    }
  });
});
