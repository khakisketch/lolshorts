import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { LazyBackend } from "./i18n/lazyBackend";

/**
 * Translation files are now loaded lazily using dynamic imports.
 * Only the active language is loaded, reducing initial bundle size by ~75 KB.
 *
 * Benefits:
 * - Initial load: Only English (~4 KB) instead of all 20 languages (~80 KB)
 * - Language switching: Loads on-demand with ~200ms latency
 * - Bundle optimization: Vite creates separate chunks for each language
 */

/**
 * Language configuration.
 *
 * The picker is deliberately limited to Korean and English (product decision
 * 2026-09-03, spec R001/G013). The other `src/locales/*` bundles are kept on
 * disk as an English fallback but are not user-selectable; anyone previously on
 * one of those locales falls back to "en".
 */
export const languages = [
  {
    code: "en",
    name: "English",
    nativeName: "English",
    flag: "🇺🇸",
    regions: ["NA", "EUW", "EUNE", "OCE"],
  },
  {
    code: "ko",
    name: "Korean",
    nativeName: "한국어",
    flag: "🇰🇷",
    regions: ["KR"],
  },
];

i18n
  .use(LazyBackend) // Use lazy loading backend
  .use(LanguageDetector) // Detect user language
  .use(initReactI18next) // Pass i18n to React
  .init({
    // No resources - loaded dynamically by LazyBackend
    fallbackLng: "en", // Default language
    debug: false,

    // Supported languages (used by LanguageDetector). Restricted to ko/en so a
    // detected browser locale like "ja" resolves to the "en" fallback instead of
    // selecting an unlisted bundle.
    supportedLngs: ["en", "ko"],

    // Only load 'translation' namespace
    ns: ["translation"],
    defaultNS: "translation",

    interpolation: {
      escapeValue: false, // React already escapes
    },

    detection: {
      // Order of language detection
      order: ["localStorage", "navigator", "htmlTag"],
      caches: ["localStorage"],
    },

    react: {
      useSuspense: false, // Don't use Suspense for language loading
    },

    backend: {
      loadPath: "/locales/{{lng}}/{{ns}}.json", // Path pattern (not used, just for typing)
    },
  });

export default i18n;
