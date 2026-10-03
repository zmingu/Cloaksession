/**
 * Public surface of the i18n layer.
 *
 * Wave3 component migrations import from `../../i18n` (this barrel) rather
 * than reaching into individual files, so the internal layout can change
 * without touching every component.
 */
export { en, type TranslationKey } from "./en.ts";
export { zhCN } from "./zh-CN.ts";
export {
  DICTIONARIES,
  FALLBACK_LANGUAGE,
  createTranslator,
  getCurrentLanguage,
  normalizeLanguage,
  setCurrentLanguage,
  t,
  translate,
  type TranslationParams,
  type Translator,
} from "./translate.ts";
export { LanguageProvider, useLanguage, useT } from "./LanguageProvider.tsx";
