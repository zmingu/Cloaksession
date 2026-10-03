/**
 * Pure translation core — no React, no HTML.
 *
 * `translate()` looks up a key in a dictionary and substitutes named
 * `{{placeholder}}` tokens with the supplied params. The result is a plain
 * string; callers render it as a React text node, so interpolated values can
 * never be interpreted as markup. There is deliberately no `dangerouslySetInnerHTML`
 * path and no HTML parsing anywhere in this module.
 */
import { en, type TranslationKey } from "./en.ts";
import { zhCN } from "./zh-CN.ts";
import type { AppLanguage } from "../types.ts";

export type { TranslationKey };

/** Params for named `{{placeholder}}` interpolation. */
export type TranslationParams = Record<string, string | number>;

export const DICTIONARIES: Record<AppLanguage, Record<TranslationKey, string>> = {
  en,
  "zh-CN": zhCN,
};

/**
 * The language used when settings cannot be loaded, or before they are known.
 * The initial HTML is authored in Chinese, so the fallback matches first paint.
 */
export const FALLBACK_LANGUAGE: AppLanguage = "zh-CN";

/** Narrow an arbitrary string (e.g. a stale settings file value) to a language. */
export function normalizeLanguage(value: unknown): AppLanguage {
  return value === "en" ? "en" : "zh-CN";
}

// Module-level language, kept in sync by the LanguageProvider. It lets
// Provider-free callers (the entry gate and the error boundary) translate with
// the last known language without importing React.
let currentLanguage: AppLanguage = FALLBACK_LANGUAGE;

export function getCurrentLanguage(): AppLanguage {
  return currentLanguage;
}

/**
 * Set the active language for Provider-free callers and mirror it onto
 * `document.documentElement.lang` (AC6). Kept out of React render so it is a
 * plain, side-effecting setter.
 */
export function setCurrentLanguage(language: AppLanguage): void {
  currentLanguage = language;
  if (typeof document !== "undefined") document.documentElement.lang = language;
}

const PLACEHOLDER = /\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}/g;

/**
 * Resolve `key` in `language` and interpolate `params`.
 *
 * - Unknown keys fall back to the English entry, then to the key itself, so a
 *   missing entry can never throw at runtime.
 * - Missing params are left as the literal `{{name}}` token so the gap is
 *   visible rather than silently blank.
 */
export function translate(
  language: AppLanguage,
  key: TranslationKey,
  params?: TranslationParams,
): string {
  const dictionary = DICTIONARIES[language] ?? DICTIONARIES[FALLBACK_LANGUAGE];
  const template = dictionary[key] ?? en[key] ?? key;
  if (!params) return template;
  return template.replace(PLACEHOLDER, (match, name: string) => {
    const value = params[name];
    return value === undefined || value === null ? match : String(value);
  });
}

/**
 * Translate using the module-level current language. Provider-free fallback for
 * the entry gate and error boundary; React components should prefer `useT()`.
 */
export function t(key: TranslationKey, params?: TranslationParams): string {
  return translate(currentLanguage, key, params);
}

/**
 * Bind a language to a translator. This is the function shape exposed by
 * `useT`; keeping it free of React makes it usable in error boundaries and
 * other non-hook contexts.
 */
export type Translator = (key: TranslationKey, params?: TranslationParams) => string;

export function createTranslator(language: AppLanguage): Translator {
  return (key, params) => translate(language, key, params);
}
