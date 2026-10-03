import { Component, StrictMode, useCallback, useEffect, useState, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { settings as settingsApi } from "./lib/ipc";
import { LanguageProvider } from "./i18n/LanguageProvider";
import { FALLBACK_LANGUAGE, normalizeLanguage, setCurrentLanguage, t } from "./i18n/translate";
import type { AppLanguage } from "./types";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

/**
 * Provider-free error boundary. It must not depend on LanguageProvider (it
 * wraps the provider, and would otherwise be useless if the provider itself
 * threw), so it translates through the module-level current language, which the
 * provider keeps in sync. Only the title is localized; the raw stack stays
 * verbatim for diagnosis.
 */
interface ErrorBoundaryState {
  error: Error | null;
}

class ErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("Renderer error:", error, info);
  }

  override render(): ReactNode {
    if (this.state.error) {
      return (
        <div className="p-8 font-mono text-sm">
          <div className="text-red-400 mb-2 font-semibold">{t("app.rendererError.title")}</div>
          <pre className="whitespace-pre-wrap text-[--color-fg-muted]">
            {String(this.state.error.stack ?? this.state.error.message)}
          </pre>
        </div>
      );
    }
    return this.props.children;
  }
}

window.addEventListener("error", (e) => {
  console.error("window error", e.error ?? e.message);
});
window.addEventListener("unhandledrejection", (e) => {
  console.error("unhandled rejection", e.reason);
});

/**
 * Minimal, language-neutral launch placeholder. It intentionally shows no
 * Chinese copy: the HTML is authored with `lang="zh-CN"`, so a saved-English
 * user must not see a full Chinese flash before settings resolve.
 */
function LaunchPlaceholder(): ReactNode {
  return (
    <div className="h-screen flex items-center justify-center" aria-busy="true">
      <div className="w-6 h-6 rounded-full border-2 border-white/15 border-t-white/60 animate-spin" />
    </div>
  );
}

type GateState =
  | { status: "loading" }
  | { status: "ready"; language: AppLanguage }
  | { status: "error"; error: string };

/**
 * Reads the persisted settings BEFORE mounting the full UI, so the first
 * painted app frame already uses the saved language. On failure it falls back
 * to Chinese and shows an actionable, retryable message instead of hanging.
 */
function EntryGate(): ReactNode {
  const [state, setState] = useState<GateState>({ status: "loading" });

  const load = useCallback((): void => {
    setState({ status: "loading" });
    settingsApi
      .get()
      .then((settings) => {
        const language = normalizeLanguage(settings.language);
        setCurrentLanguage(language);
        setState({ status: "ready", language });
      })
      .catch((error: unknown) => {
        // Chinese fallback: the initial HTML is zh-CN and the product default
        // is Chinese, so a failed load degrades to the documented default.
        setCurrentLanguage(FALLBACK_LANGUAGE);
        setState({ status: "error", error: String(error) });
      });
  }, []);

  useEffect(load, [load]);

  if (state.status === "loading") return <LaunchPlaceholder />;

  if (state.status === "error") {
    return (
      <div className="h-screen flex items-center justify-center p-8">
        <div className="max-w-[520px] text-center">
          <div className="text-red-400 mb-2 font-semibold">
            {t("app.settingsLoadFailed.title")}
          </div>
          <p role="alert" className="text-[13px] text-slate-400 leading-relaxed mb-4 break-words">
            {t("app.settingsLoadFailed.body", { error: state.error })}
          </p>
          <button
            type="button"
            onClick={load}
            className="btn-secondary px-3 py-[7px] text-[12px] rounded-[9px]"
          >
            {t("common.retry")}
          </button>
        </div>
      </div>
    );
  }

  return (
    <LanguageProvider initialLanguage={state.language}>
      <App />
    </LanguageProvider>
  );
}

createRoot(root).render(
  <StrictMode>
    <ErrorBoundary>
      <EntryGate />
    </ErrorBoundary>
  </StrictMode>,
);
