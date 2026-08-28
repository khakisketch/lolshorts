/**
 * Custom i18next backend plugin for lazy loading translation files
 *
 * This backend uses dynamic imports to load translation files on-demand,
 * significantly reducing initial bundle size by only loading the active language.
 */

import { BackendModule, ReadCallback } from "i18next";
import { logger } from "@/lib/logger";

interface LazyBackendOptions {
  loadPath: string;
}

/**
 * Dynamically imports translation file for the specified language
 *
 * @param language - Language code (e.g., 'en', 'ko', 'zh-CN')
 * @returns Promise resolving to the translation object
 */
async function loadTranslation(
  language: string,
): Promise<Record<string, unknown>> {
  try {
    // Normalize language code for file path (e.g., 'zh-CN' -> 'zh-CN')
    const normalizedLang = language;

    // Dynamic import based on language code
    // Vite will create separate chunks for each language file
    const module = await import(
      `../locales/${normalizedLang}/translation.json`
    );

    return module.default || module;
  } catch (error) {
    logger.error(`Failed to load translation for language: ${language}`, error);
    throw new Error(`Translation file not found: ${language}`);
  }
}

/**
 * Custom backend plugin for i18next that loads translations lazily
 */
export const LazyBackend: BackendModule<LazyBackendOptions> = {
  type: "backend",

  init(
    _services: unknown,
    _backendOptions: LazyBackendOptions,
    _i18nextOptions: unknown,
  ) {
    // No initialization needed
  },

  read(language: string, namespace: string, callback: ReadCallback) {
    // Only support 'translation' namespace
    if (namespace !== "translation") {
      callback(new Error(`Unsupported namespace: ${namespace}`), null);
      return;
    }

    loadTranslation(language)
      .then((data) => {
        callback(null, data);
      })
      .catch((error) => {
        callback(error, null);
      });
  },
};
