import { dialog, settings as settingsApi, system, update, onUpdateStatus } from "../../lib/ipc";
import { useEffect, useState, type JSX, type ReactNode } from "react";
import {
  Boxes,
  Check,
  Chrome,
  Copy,
  DownloadCloud,
  Eye,
  EyeOff,
  FileSearch,
  Languages,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Zap,
} from "lucide-react";
import { Pill } from "../atoms";
import { ChromixSettingsEditor } from "./ChromixSettingsEditor";
import { relativeTime } from "../../lib/relativeTime";
import { useScrollFade } from "../../lib/useScrollFade";
import { useLanguage, useT } from "../../i18n/LanguageProvider";
import type { Translator } from "../../i18n/translate";
import type { AppLanguage, AppSettings, SystemInfo, UpdateStatus } from "../../types";

interface Props {
  onImport: () => void;
}

export function Settings({ onImport }: Props): JSX.Element {
  const t = useT();
  const { language, setLanguage } = useLanguage();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [tokenCopied, setTokenCopied] = useState(false);
  const [tokenShown, setTokenShown] = useState(false);
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [lastChecked, setLastChecked] = useState<number>(0);
  const [languageSaving, setLanguageSaving] = useState(false);
  const [languageError, setLanguageError] = useState<string | null>(null);
  const scrollRef = useScrollFade<HTMLDivElement>();

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void settingsApi.get().then(setSettings).catch((error) => setSettingsError(String(error)));
    void system.info().then(setInfo);
    void update.status().then(setUpdateStatus);
    void update.lastChecked().then(setLastChecked);
    // Refresh "last checked" on every status change too, so a background
    // auto-check updates the label live while Settings is open.
    void onUpdateStatus((s) => {
      setUpdateStatus(s);
      void update.lastChecked().then(setLastChecked);
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  async function patch(p: Partial<AppSettings>): Promise<void> {
    if (!settings) return;
    try {
      const next = await settingsApi.update(p);
      setSettings(next);
      setSettingsError(null);
    } catch (error) {
      setSettingsError(t("settings.saveFailed", { error: String(error) }));
    }
  }

  /**
   * Switch the interface language through the existing settings IPC. The
   * global language is updated by `LanguageProvider` only after the server
   * confirms the save, and repeated submits are disabled while in flight, so
   * the UI never claims a change that was not persisted. Switching language
   * does not unmount `App`, so unsaved form input and the current section are
   * preserved.
   */
  async function changeLanguage(next: AppLanguage): Promise<void> {
    if (languageSaving || next === language) return;
    setLanguageSaving(true);
    setLanguageError(null);
    try {
      await setLanguage(next);
    } catch (error) {
      setLanguageError(t("settings.language.saveFailed", { error: String(error) }));
    } finally {
      setLanguageSaving(false);
    }
  }

  async function checkForUpdates(): Promise<void> {
    await update.check();
    setLastChecked(await update.lastChecked());
  }

  function copyMcpUrl(): void {
    if (!info?.mcpHttpUrl) return;
    navigator.clipboard.writeText(info.mcpHttpUrl);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  function copyMcpToken(): void {
    if (!info?.mcpAuthToken) return;
    navigator.clipboard.writeText(info.mcpAuthToken);
    setTokenCopied(true);
    window.setTimeout(() => setTokenCopied(false), 1500);
  }

  if (!settings) {
    return (
      <div className="flex-1 overflow-auto p-8">
        <div className="max-w-[720px] mx-auto text-[13px] text-slate-500">
          {settingsError
            ? <p role="alert" className="text-red-300">{t("settings.loadFailed", { error: settingsError })}</p>
            : t("settings.loading")}
        </div>
      </div>
    );
  }

  return (
    <div ref={scrollRef} role="region" aria-label={t("nav.settings")} className="flex-1 min-w-0 overflow-auto scroll-fade px-3 py-5 sm:px-8 sm:py-6">
      <div className="max-w-[720px] mx-auto">
        <div className="text-lg font-bold tracking-tight text-slate-100 mb-1.5">{t("nav.settings")}</div>
        <div className="text-[13px] text-slate-500 mb-5">
          {t("settings.intro")}
        </div>

        {settingsError && <p role="alert" className="text-[12px] text-red-300 mb-3 break-words">{settingsError}</p>}

        <Row
          icon={<Languages size={16} strokeWidth={1.5} />}
          title={t("settings.language.title")}
          desc={t("settings.language.desc")}
        >
          <div className="flex flex-wrap gap-2" role="group" aria-label={t("settings.language.label")}>
            {languageOptions.map((option) => {
              const selected = language === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  lang={option.value}
                  aria-pressed={selected}
                  disabled={languageSaving}
                  onClick={() => void changeLanguage(option.value)}
                  className="px-3 py-[7px] text-[12px] rounded-[9px] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
                  style={{
                    boxShadow: selected
                      ? "inset 0 0 0 1px var(--ring)"
                      : "inset 0 0 0 1px rgba(255,255,255,0.07)",
                    background: selected ? "var(--accent)" : "rgba(255,255,255,0.025)",
                  }}
                >
                  {t(option.labelKey)}
                </button>
              );
            })}
            {languageSaving && (
              <span className="text-[12px] text-slate-500 self-center">
                {t("settings.language.saving")}
              </span>
            )}
          </div>
          {languageError && (
            <p role="alert" className="text-[12px] text-red-300 mt-2 break-words">{languageError}</p>
          )}
        </Row>

        <Row
          icon={<Zap size={16} strokeWidth={1.5} />}
          title={t("settings.mcp.title")}
          desc={t("settings.mcp.desc")}
        >
          <div className="flex gap-2 items-center flex-wrap">
            <Pill kind={info?.mcpHttpUrl ? "running" : "idle"} dot={!!info?.mcpHttpUrl}>
              {info?.mcpHttpUrl
                ? t("settings.mcp.runningOn", { port: settings.mcpHttpPort })
                : t("settings.mcp.off")}
            </Pill>

            {info?.mcpHttpUrl && (
              <div
                className="flex-1 min-w-0 basis-[220px] flex items-center gap-2"
                style={{
                  padding: "8px 12px",
                  borderRadius: 10,
                  background: "rgba(255,255,255,0.03)",
                  boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.06)",
                }}
              >
                <span className="flex-1 min-w-0 mono text-[12px] text-slate-300 truncate">
                  {info.mcpHttpUrl}
                </span>
                <button
                  type="button"
                  onClick={copyMcpUrl}
                  className="text-[var(--accent-foreground)] hover:opacity-80 transition-colors"
                  aria-label={t("settings.mcp.copyUrlAria")}
                >
                  {copied ? <Check size={13} /> : <Copy size={13} />}
                </button>
              </div>
            )}
          </div>

          {info?.mcpAuthToken && (
            <div className="mt-2">
              <div className="text-[11px] text-slate-500 mb-1">
                {t("settings.mcp.tokenHint")}
              </div>
              <div
                className="flex items-center gap-2"
                style={{
                  padding: "8px 12px",
                  borderRadius: 10,
                  background: "rgba(255,255,255,0.03)",
                  boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.06)",
                }}
              >
                <span className="flex-1 min-w-0 mono text-[12px] text-slate-300 truncate">
                  {tokenShown ? info.mcpAuthToken : "•".repeat(24)}
                </span>
                <button
                  type="button"
                  onClick={() => setTokenShown((v) => !v)}
                  className="text-slate-400 hover:text-slate-200 transition-colors"
                  aria-label={tokenShown ? t("settings.mcp.hideToken") : t("settings.mcp.revealToken")}
                >
                  {tokenShown ? <EyeOff size={13} /> : <Eye size={13} />}
                </button>
                <button
                  type="button"
                  onClick={copyMcpToken}
                  className="text-[var(--accent-foreground)] hover:opacity-80 transition-colors"
                  aria-label={t("settings.mcp.copyTokenAria")}
                >
                  {tokenCopied ? <Check size={13} /> : <Copy size={13} />}
                </button>
              </div>
            </div>
          )}

          <label className="flex items-center gap-2.5 mt-3 text-[12px] text-slate-400 cursor-pointer">
            <input
              type="checkbox"
              checked={settings.mcpHttpEnabled}
              onChange={(e) => void patch({ mcpHttpEnabled: e.target.checked })}
              className="w-3.5 h-3.5 rounded accent-[var(--ring)]"
            />
            {t("settings.mcp.autostart")}
          </label>
        </Row>

        <Row
          icon={<Chrome size={16} strokeWidth={1.5} />}
          title={t("settings.engine.title")}
          desc={t("settings.engine.desc")}
        >
          <div className="grid gap-2 sm:grid-cols-2">
            {engineOptions.map((option) => {
              const selected = settings.browserEngine === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  aria-pressed={selected}
                  onClick={() => void patch({ browserEngine: option.value })}
                  className="text-left p-3 rounded-lg transition-colors"
                  style={{
                    boxShadow: selected
                      ? "inset 0 0 0 1px var(--ring)"
                      : "inset 0 0 0 1px rgba(255,255,255,0.07)",
                    background: selected ? "var(--accent)" : "rgba(255,255,255,0.025)",
                  }}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[13px] font-medium text-slate-100">{option.label}</span>
                    {selected && <Pill kind="running">{t("settings.engine.selected")}</Pill>}
                  </div>
                  <div className="text-[11px] text-slate-500 mt-1 leading-relaxed">
                    {t(option.descKey)}
                  </div>
                </button>
              );
            })}
          </div>
        </Row>

        {settings.browserEngine === "chromix" && (
          <Row
            icon={<Chrome size={16} strokeWidth={1.5} />}
            title={t("settings.chromix.title")}
            desc={t("settings.chromix.desc")}
          >
            <ChromixSettingsEditor
              value={settings.chromix}
              onSave={async (chromix) => {
                const next = await settingsApi.update({ chromix });
                setSettings(next);
                setSettingsError(null);
              }}
            />
          </Row>
        )}

        <Row
          icon={<FileSearch size={16} strokeWidth={1.5} />}
          title={t("settings.binary.title")}
          desc={settings.browserEngine === "chromix"
            ? t("settings.binary.descChromix")
            : t("settings.binary.descDefault")}
        >
          <div className="flex flex-wrap items-center gap-2">
            <input
              aria-label={t("settings.binary.pathAria")}
              type="text"
              value={settings.browserBinaryPath ?? ""}
              onChange={(e) => void patch({ browserBinaryPath: e.target.value })}
              placeholder={settings.browserEngine === "chromix" ? t("settings.binary.phChromix") : t("settings.binary.phDefault")}
              className="flex-1 basis-[180px] min-w-0 h-8 px-2.5 text-[12px] text-slate-200 rounded-lg bg-white/[0.03] outline-none"
              style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.07)" }}
            />
            <button
              type="button"
              className="btn-secondary px-3 h-8 text-[12px] rounded-[9px] whitespace-nowrap"
              onClick={async () => {
                const p = await dialog.pickBrowserBinary();
                if (p) await patch({ browserBinaryPath: p });
              }}
            >
              {t("settings.binary.browse")}
            </button>
            {settings.browserBinaryPath && (
              <button
                type="button"
                className="btn-ghost px-2.5 h-8 text-[12px] rounded-[9px]"
                onClick={() => void patch({ browserBinaryPath: "" })}
              >
                {t("settings.binary.reset")}
              </button>
            )}
          </div>
          <label className="flex items-center gap-2.5 mt-3 text-[12px] text-slate-400 cursor-pointer">
            <input
              type="checkbox"
              checked={settings.skipBrowserDownload ?? false}
              onChange={(e) => void patch({ skipBrowserDownload: e.target.checked })}
              className="w-3.5 h-3.5 rounded accent-[var(--ring)]"
            />
            {settings.browserEngine === "chromix"
              ? t("settings.binary.skipDownload")
              : t("settings.binary.skipDownloadLegacy")}
          </label>
          <div className="text-[11px] text-slate-600 mt-2 leading-relaxed">
            {settings.browserEngine === "chromix"
              ? t("settings.binary.offlineNote")
              : t("settings.binary.offlineNoteLegacy")}
          </div>
        </Row>

        <Row
          icon={<Boxes size={16} strokeWidth={1.5} />}
          title={t("settings.archives.title")}
          desc={t("settings.archives.desc")}
        >
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              className="btn-secondary px-3 py-[7px] text-[12px] rounded-[9px]"
              onClick={onImport}
            >
              {t("settings.archives.import")}
            </button>
            <a
              href="https://github.com/multizenteam/multizen-browser#archives"
              target="_blank"
              rel="noopener"
              className="btn-ghost px-3 py-[7px] text-[12px] rounded-[9px]"
            >
              {t("settings.archives.readDocs")}
            </a>
          </div>
        </Row>

        <Row
          icon={<DownloadCloud size={16} strokeWidth={1.5} />}
          title={t("settings.updates.title")}
          desc={
            info?.platform === "darwin"
              ? t("settings.updates.descMacos")
              : t("settings.updates.descDefault")
          }
        >
          <div className="flex items-center gap-2.5 flex-wrap">
            <button
              type="button"
              className="btn-secondary px-3 py-[7px] text-[12px] rounded-[9px] inline-flex items-center gap-1.5"
              onClick={() => void checkForUpdates()}
              disabled={updateStatus?.kind === "checking"}
            >
              <RefreshCw
                size={12}
                className={updateStatus?.kind === "checking" ? "animate-spin" : ""}
              />
              {t("settings.updates.check")}
            </button>
            <span className="text-[12px] text-slate-400">{updateLabel(t, updateStatus)}</span>
          </div>
          <div className="text-[11px] text-slate-600 mt-2">
            {t("settings.updates.lastChecked", {
              time: lastChecked
                ? relativeTime(new Date(lastChecked).toISOString())
                : t("settings.updates.never"),
            })}
          </div>
          <label className="flex items-center gap-2.5 mt-3 text-[12px] text-slate-400 cursor-pointer">
            <input
              type="checkbox"
              checked={settings.autoUpdate}
              onChange={(e) => void patch({ autoUpdate: e.target.checked })}
              className="w-3.5 h-3.5 rounded accent-[var(--ring)]"
            />
            {t("settings.updates.autoCheck")}
          </label>
        </Row>

        <Row
          icon={<ShieldCheck size={16} strokeWidth={1.5} />}
          title={t("settings.telemetry.title")}
          desc={t("settings.telemetry.desc")}
        >
          <label className="flex items-center gap-2.5 text-[12px] text-slate-400 cursor-pointer">
            <input
              type="checkbox"
              checked={settings.usageReporting}
              onChange={(e) => void patch({ usageReporting: e.target.checked })}
              className="w-3.5 h-3.5 rounded accent-[var(--ring)]"
            />
            {t("settings.telemetry.heartbeat")}
          </label>
          <div className="text-[11px] text-slate-600 mt-2 leading-relaxed">
            {t("settings.telemetry.heartbeatDesc")}
          </div>
        </Row>

        <Row icon={<Sparkles size={16} strokeWidth={1.5} />} title={t("settings.about.title")} desc="">
          <div className="mono text-[12px] text-slate-400 leading-relaxed">
            {t("settings.about.version", {
              version: info?.appVersion ?? "0.0.0",
              platform: info?.platform ?? "—",
            })}
          </div>
        </Row>
      </div>
    </div>
  );
}

const languageOptions: Array<{ value: AppLanguage; labelKey: "settings.language.option.zhCN" | "settings.language.option.en" }> = [
  { value: "zh-CN", labelKey: "settings.language.option.zhCN" },
  { value: "en", labelKey: "settings.language.option.en" },
];

const engineOptions: Array<{
  value: AppSettings["browserEngine"];
  label: string;
  descKey: "settings.engine.cloakbrowserDesc" | "settings.engine.chromixDesc" | "settings.engine.cftDesc";
}> = [
  {
    value: "cloakbrowser",
    label: "CloakBrowser",
    descKey: "settings.engine.cloakbrowserDesc",
  },
  {
    value: "chromix",
    label: "Chromix",
    descKey: "settings.engine.chromixDesc",
  },
  {
    value: "cft",
    label: "Chrome for Testing",
    descKey: "settings.engine.cftDesc",
  },
];

function updateLabel(t: Translator, status: UpdateStatus | null): string {
  switch (status?.kind) {
    case "checking":
      return t("update.status.checking");
    case "up-to-date":
      return t("update.status.upToDate");
    case "available":
      return t("update.status.available", { version: status.version });
    case "downloading":
      return t("update.status.downloading", { version: status.version, percent: status.percent });
    case "ready":
      return t("update.status.ready", { version: status.version });
    case "error":
      return t("update.status.error", { message: status.message });
    default:
      return "";
  }
}

function Row({
  icon,
  title,
  desc,
  children,
}: {
  icon: ReactNode;
  title: string;
  desc: ReactNode;
  children?: ReactNode;
}): JSX.Element {
  return (
    <div
      className="flex items-start gap-3.5"
      style={{
        padding: "16px 0",
        borderBottom: "1px solid rgba(255,255,255,0.04)",
      }}
    >
      <div
        className="hidden sm:flex items-center justify-center flex-shrink-0"
        style={{
          width: 36,
          height: 36,
          borderRadius: 10,
          background: "var(--accent)",
          boxShadow: "inset 0 0 0 1px var(--ring)",
          color: "var(--accent-foreground)",
        }}
      >
        {icon}
      </div>
      <div className="flex-1 min-w-0 break-words">
        <div className="text-[13px] font-semibold text-slate-100">{title}</div>
        {desc && (
          <div className="text-[12px] text-slate-500 mt-1 leading-relaxed max-w-[480px]">
            {desc}
          </div>
        )}
        {children && <div className="mt-2.5">{children}</div>}
      </div>
    </div>
  );
}
