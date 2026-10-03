/**
 * React bindings for the translation layer.
 *
 * `LanguageProvider` is the single owner of the active UI language. It is
 * mounted once, above the app tree, so switching languages re-renders consumers
 * through context without remounting `App` or unmounting any forms.
 *
 * Persistence contract: `setLanguage` writes through the existing settings IPC
 * and only updates the in-memory/global language after the server confirms
 * success. A failed save leaves the previous language active, so the UI never
 * claims a change that was not stored.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { settings as settingsApi } from "../lib/ipc";
import type { AppLanguage } from "../types";
import {
  createTranslator,
  normalizeLanguage,
  setCurrentLanguage,
  type Translator,
} from "./translate.ts";

interface LanguageContextValue {
  language: AppLanguage;
  /** Persist and activate `next`; resolves to the language actually in effect. */
  setLanguage: (next: AppLanguage) => Promise<AppLanguage>;
  t: Translator;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

interface Props {
  /** Language resolved by the entry gate before the app mounted. */
  initialLanguage: AppLanguage;
  children: ReactNode;
}

export function LanguageProvider({ initialLanguage, children }: Props): ReactNode {
  const [language, setLanguageState] = useState<AppLanguage>(initialLanguage);

  // Keep the module-level language (for the Provider-free error boundary) and
  // `document.lang` in sync with the active language (AC6).
  useEffect(() => {
    setCurrentLanguage(language);
  }, [language]);

  const setLanguage = useCallback(async (next: AppLanguage): Promise<AppLanguage> => {
    const saved = await settingsApi.update({ language: next });
    const resolved = normalizeLanguage(saved.language);
    setCurrentLanguage(resolved);
    setLanguageState(resolved);
    return resolved;
  }, []);

  const value = useMemo<LanguageContextValue>(
    () => ({ language, setLanguage, t: createTranslator(language) }),
    [language, setLanguage],
  );

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

function useLanguageContext(): LanguageContextValue {
  const value = useContext(LanguageContext);
  if (!value) throw new Error("useLanguage must be used within a LanguageProvider");
  return value;
}

/** Active language plus the persistence-aware setter. */
export function useLanguage(): Pick<LanguageContextValue, "language" | "setLanguage"> {
  const { language, setLanguage } = useLanguageContext();
  return { language, setLanguage };
}

/** Translator bound to the active language. */
export function useT(): Translator {
  return useLanguageContext().t;
}
