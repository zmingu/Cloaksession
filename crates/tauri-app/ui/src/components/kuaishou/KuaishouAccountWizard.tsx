import { useEffect, useRef, useState, type JSX } from "react";
import {
  Blocks,
  Check,
  CircleAlert,
  Dices,
  Fingerprint,
  Globe,
  Loader2,
  Network,
  RefreshCw,
  type LucideIcon,
} from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import type { TranslationKey } from "../../i18n/en";
import { fingerprintSeedOf, withRandomFingerprintSeed } from "../../lib/chromixSeed";
import { profiles as profilesApi } from "../../lib/ipc";
import { kuaishouAuth } from "../../lib/kuaishouAuth";
import { kuaishouIdentity, type KuaishouIdentitySnapshot } from "../../lib/kuaishouIdentity";
import {
  kuaishouSubject,
  type KuaishouInitErrorCode,
  type KuaishouInitStepRecord,
} from "../../lib/kuaishouSubject";
import { useKuaishouIdentities } from "../../lib/KuaishouIdentityProvider";
import { parseProxyString } from "../../lib/parseProxy";
import type { ExtensionConfig, ProxyConfig } from "../../types";
import { Modal } from "../atoms";
import { Button } from "../atoms/Button";
import { IdentityAvatar } from "../profile/KuaishouIdentity";
import { ProxyTester } from "../profile/ProxyTester";
import { BrowserSection } from "../profile/BrowserSection";
import { ChromixProfileOptions } from "../profile/ChromixProfileOptions";
import { ExtensionsSection } from "../profile/ExtensionsSection";
import {
  Field,
  Input,
  SHEET_HEIGHT,
  SectionRail,
  type SectionId,
} from "../profile/profileSheetKit";

/** How often the wizard re-captures the login page while waiting for a scan. */
const QR_POLL_MS = 2000;

/** How often the wizard reads the persisted initialization steps while running. */
const INIT_POLL_MS = 5000;

/** How often the wizard re-observes the identity so it never goes stale (>30s). */
const INIT_DETECT_MS = 10_000;

/** Give up waiting on the campaign after this long and let the user leave. */
const INIT_TIMEOUT_MS = 180_000;

/** Localized label for one persisted initialization step state (shared keys). */
const INIT_STATE_KEY: Record<KuaishouInitStepRecord["state"], TranslationKey> = {
  pending: "kuaishou.init.state.pending",
  running: "kuaishou.init.state.running",
  done: "kuaishou.init.state.done",
  failed: "kuaishou.init.state.failed",
};

/** Localized label for a failed step's error code (shared with the shop list). */
const INIT_ERROR_KEY: Record<KuaishouInitErrorCode, TranslationKey> = {
  "interrupted-needs-verification": "kuaishou.init.error.interruptedNeedsVerification",
  "context-changed": "kuaishou.init.error.contextChanged",
  "timed-out": "kuaishou.init.error.timedOut",
  "page-unsupported": "kuaishou.init.error.pageUnsupported",
  "page-crashed": "kuaishou.init.error.pageCrashed",
  "attachment-unavailable": "kuaishou.init.error.attachmentUnavailable",
  "ocr-unavailable": "kuaishou.init.error.ocrUnavailable",
  "ocr-failed": "kuaishou.init.error.ocrFailed",
  "validation-failed": "kuaishou.init.error.validationFailed",
  "persistence-unverified": "kuaishou.init.error.persistenceUnverified",
};

/** The wizard's own view of the initialization campaign. */
type InitState = "running" | "done" | "failed" | "timeout";

/** A failed campaign: an error code from a step, or the retry IPC message. */
interface InitFailure {
  code: KuaishouInitErrorCode | null;
  message: string | null;
}

/** Prefilled, editable home page for a shop account: the Kuaishou shop home. */
const SHOP_HOME_URL = "https://s.kwaixiaodian.com/zone/home";

/**
 * 小店账号建号向导（快手 › 小店 › 添加账号）。
 *
 * 账号即环境：向导创建的就是一个浏览器环境，列表里一个环境 = 一个小店账号。
 * 第一步是 Chromix 风格的分区表单（主页 / 代理 / 扩展 / 指纹，无 General）：
 *  - 主页预填小店首页 `https://s.kwaixiaodian.com/zone/home`（可编辑）；
 *  - 代理可选，填了就让 Chromix 按代理出口自动对齐（`chromixOptions.geoip`）；
 *  - 扩展按 staged 模式收集，创建时一并写入；
 *  - 指纹直接用 Chromix 指纹组件（`ChromixProfileOptions`），并在打开时
 *    自动写入一个随机 `--fingerprint=<seed>`，另有「随机指纹」按钮可一键重掷。
 * 创建时不再传 fingerprint —— 身份由 Chromix 拥有，CloakBrowser 指纹已废弃。
 *
 * 流程：填表单 → 创建环境 → 隐藏启动（窗口移出屏幕）→ 向导内轮询取二维码 →
 * 自动读出快手ID/昵称/头像 → 完成。
 *
 * 命名：向导**不**让用户填名字。环境先以「未命名」创建，识别到账号后
 * 自动用昵称（无昵称则用快手ID）改名。若创建成功但用户提前取消，
 * 名称保持「未命名」，之后可在列表里改。
 *
 * 初始化（主体采集 / 切片权限）由既有的「识别到有效账号后自动补做」机制处理，
 * 本向导不驱动。
 */

type Step = "form" | "creating" | "waiting" | "done";

interface Props {
  open: boolean;
  onClose: () => void;
  /** Fired once the profile exists so the caller can refresh its list. */
  onCreated?: (profileId: string) => void;
}

interface DraftProxy {
  enabled: boolean;
  type: "http" | "socks5";
  host: string;
  port: string;
  username: string;
  password: string;
}

const EMPTY_PROXY: DraftProxy = {
  enabled: false,
  type: "http",
  host: "",
  port: "",
  username: "",
  password: "",
};

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 小店账号建号向导（快手 › 小店 › 添加账号）。
 *
 * 账号即环境：向导创建的就是一个浏览器环境，列表里一个环境 = 一个小店账号。
 * 第一步是 Chromix 风格的分区表单（主页 / 代理 / 扩展 / 指纹，无 General）：
 *  - 主页预填小店首页 `https://s.kwaixiaodian.com/zone/home`（可编辑）；
 *  - 代理可选，填了就让 Chromix 按代理出口自动对齐（`chromixOptions.geoip`）；
 *  - 扩展按 staged 模式收集，创建时一并写入；
 *  - 指纹直接用 Chromix 指纹组件（`ChromixProfileOptions`），并在打开时
 *    自动写入一个随机 `--fingerprint=<seed>`，另有「随机指纹」按钮可一键重掷。
 * 创建时不再传 fingerprint —— 身份由 Chromix 拥有，CloakBrowser 指纹已废弃。
 *
 * 流程：填表单 → 创建环境 → 隐藏启动（窗口移出屏幕）→ 向导内轮询取二维码 →
 * 自动读出快手ID/昵称/头像 → 完成。
 *
 * 命名：向导**不**让用户填名字。环境先以「未命名」创建，识别到账号后
 * 自动用昵称（无昵称则用快手ID）改名。若创建成功但用户提前取消，
 * 名称保持「未命名」，之后可在列表里改。
 *
 * 初始化（主体采集 / 切片权限）：账号识别后由本向导**主动驱动**——保持隐藏浏览器
 * 运行、触发一次补做（`kuaishou_init_retry`）并轮询持久化的初始化步骤，直到主体与
 * 切片两步都完成；全部完成前不关闭浏览器，失败/超时给出提示并允许用户手动关闭。
 */
export function KuaishouAccountWizard({ open, onClose, onCreated }: Props): JSX.Element {
  const t = useT();
  const { entries } = useKuaishouIdentities();

  const [step, setStep] = useState<Step>("form");
  const [section, setSection] = useState<SectionId>("browser");
  const [startUrl, setStartUrl] = useState(SHOP_HOME_URL);
  const [proxyDraft, setProxyDraft] = useState<DraftProxy>(EMPTY_PROXY);
  const [extensions, setExtensions] = useState<ExtensionConfig[]>([]);
  const [chromixOptions, setChromixOptions] = useState<Record<string, unknown>>({});
  const [profileId, setProfileId] = useState<string | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  // The wizard drives detection itself rather than waiting on the app-wide
  // 15s poll, so the sign-in completes the moment the QR is scanned.
  const [snapshot, setSnapshot] = useState<KuaishouIdentitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Set once the profile has been renamed from the detected account.
  const [named, setNamed] = useState(false);
  // Wizard-driven initialization: once the account is detected the wizard keeps
  // the hidden browser alive, triggers the catch-up run and polls the persisted
  // steps until both are done (or the campaign fails / times out).
  const [initState, setInitState] = useState<InitState>("running");
  const [initSteps, setInitSteps] = useState<KuaishouInitStepRecord[]>([]);
  const [initFailure, setInitFailure] = useState<InitFailure | null>(null);
  const [initAttempt, setInitAttempt] = useState(0);

  const alive = useRef(true);
  // A created profile must survive a close: we only hide/stop the browser.
  const created = useRef(false);
  // One rename per wizard run; guarded by a ref so the effect cannot double-fire.
  const renamed = useRef(false);
  // One retry request per detection; guarded so a re-render cannot double-fire.
  const initStarted = useRef(false);

  // The wizard's own rail: no General, Chinese labels, home / proxy / extensions
  // / fingerprint. Kept local so the profile sheets' English 5-section rail is
  // left untouched.
  const sections: Array<{ id: SectionId; label: string; icon: LucideIcon }> = [
    { id: "browser", label: t("kuaishou.wizard.section.home"), icon: Globe },
    { id: "proxy", label: t("kuaishou.wizard.section.proxy"), icon: Network },
    { id: "extensions", label: t("kuaishou.wizard.section.extensions"), icon: Blocks },
    { id: "chromix", label: t("kuaishou.wizard.section.chromix"), icon: Fingerprint },
  ];

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // Reset everything when the wizard is (re)opened.
  useEffect(() => {
    if (!open) return;
    setStep("form");
    setSection("browser");
    setStartUrl(SHOP_HOME_URL);
    setProxyDraft(EMPTY_PROXY);
    setExtensions([]);
    // Auto-random fingerprint: write a fresh `--fingerprint=<seed>` so Chromix
    // derives a coherent random device instead of reusing a persistent seed.
    setChromixOptions(withRandomFingerprintSeed({}));
    setProfileId(null);
    setQr(null);
    setSnapshot(null);
    setError(null);
    setNamed(false);
    setInitState("running");
    setInitSteps([]);
    setInitFailure(null);
    setInitAttempt(0);
    created.current = false;
    renamed.current = false;
    initStarted.current = false;
  }, [open]);

  const platformUserId = snapshot?.platformUserId ?? null;

  /** One-click random fingerprint: replace the seed with a fresh one. */
  function randomizeFingerprint(): void {
    setChromixOptions((current) => withRandomFingerprintSeed(current));
  }

  // Once a real identity is read for our profile, move to "done" AND take over
  // initialization: keep the hidden browser alive, trigger the catch-up run
  // (guarded so it fires exactly once), and let the polling effect below drive
  // the progress UI. The profile/browser is never closed before this completes.
  useEffect(() => {
    if (!platformUserId || !profileId) return;
    setStep("done");
    if (initStarted.current) return;
    initStarted.current = true;
    void kuaishouSubject.retry(profileId).catch((cause: unknown) => {
      // A rejected retry (e.g. "a detection or initialization is already
      // running") is surfaced rather than retried in a loop.
      if (alive.current) setInitFailure({ code: null, message: errorText(cause) });
    });
  }, [platformUserId, profileId]);

  // Name the account from the detected identity: nickname when present, else the
  // Kuaishou ID. Best-effort — a failure leaves the placeholder name in place.
  useEffect(() => {
    if (!platformUserId || !profileId || renamed.current) return;
    const next = (snapshot?.nickname || snapshot?.platformUserId || "").trim();
    if (!next) return;
    renamed.current = true;
    void (async () => {
      try {
        await profilesApi.update(profileId, { name: next });
        if (alive.current) setNamed(true);
      } catch {
        renamed.current = false;
      }
    })();
  }, [platformUserId, profileId, snapshot]);

  // While waiting, keep re-reading the hidden page (for the QR) and asking the
  // backend whether the profile has signed in yet. Stops on "done", on unmount,
  // and when the wizard closes — so no timer or read leaks.
  useEffect(() => {
    // `open` is part of the gate: the wizard component stays mounted (the
    // Modal only renders null), so closing must tear the timer down here.
    if (!open || step !== "waiting" || !profileId) return;
    let active = true;
    let timer = 0;
    const tick = async (): Promise<void> => {
      try {
        // A `null` means "no valid QR right now" — not rendered yet, or the page
        // auto-refreshed an expired one — so clear the frame rather than keep a
        // dead QR on screen.
        const image = await kuaishouAuth.loginQr(profileId);
        if (active) setQr(image);
      } catch {
        // A transient read failure must not abort the wait; keep polling.
      }
      try {
        const found = await kuaishouIdentity.detect(profileId);
        if (active && found.platformUserId) setSnapshot(found);
      } catch {
        // Not signed in yet (or the page is not ready) — keep polling.
      }
      if (active) timer = window.setTimeout(() => void tick(), QR_POLL_MS);
    };
    void tick();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [open, step, profileId]);

  // Wizard-driven initialization. Runs only while the wizard is open, on the
  // "done" step, with a detected account — the hidden browser stays alive for
  // the whole campaign. Two cadences share one timer chain so nothing leaks:
  //   - every tick reads the persisted steps (both done → finished; a failed
  //     step → failed);
  //   - every INIT_DETECT_MS it re-observes the identity so the backend's
  //     ≤30s freshness window never expires mid-run;
  //   - after INIT_TIMEOUT_MS it stops waiting and offers a manual close.
  // `open` and `step` are dependencies because the Modal keeps this component
  // mounted while closed, and the cleanup clears the timer.
  useEffect(() => {
    if (!open || step !== "done" || !profileId || !platformUserId) return;
    if (initState !== "running") return;
    let active = true;
    let timer = 0;
    let lastDetect = 0;
    const startedAt = Date.now();
    const tick = async (): Promise<void> => {
      try {
        const rows = await kuaishouSubject.steps(platformUserId);
        if (!active) return;
        setInitSteps(rows);
        const subject = rows.find((item) => item.step === "subject");
        const slice = rows.find((item) => item.step === "slice");
        if (subject?.state === "done" && slice?.state === "done") {
          setInitFailure(null);
          setInitState("done");
          return;
        }
        const failed = rows.find((item) => item.state === "failed");
        if (failed) {
          setInitFailure({ code: failed.lastErrorCode, message: null });
          setInitState("failed");
          return;
        }
      } catch {
        // A transient read failure must not abort the campaign; retry next tick.
      }
      if (!active) return;
      if (Date.now() - startedAt >= INIT_TIMEOUT_MS) {
        setInitState("timeout");
        return;
      }
      // Keep the observation fresh so the next campaign attempt is accepted.
      if (Date.now() - lastDetect >= INIT_DETECT_MS) {
        lastDetect = Date.now();
        try {
          const found = await kuaishouIdentity.detect(profileId);
          if (active && found.platformUserId) setSnapshot(found);
        } catch {
          // Detection is best-effort; the backend re-validates before any write.
        }
      }
      if (active) timer = window.setTimeout(() => void tick(), INIT_POLL_MS);
    };
    void tick();
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [open, step, profileId, platformUserId, initState, initAttempt]);

  /** Manual retry: clear the terminal state and run the campaign again. */
  function retryInit(): void {
    if (!profileId) return;
    setInitFailure(null);
    setInitSteps([]);
    setInitState("running");
    initStarted.current = true;
    void kuaishouSubject.retry(profileId).catch((cause: unknown) => {
      if (alive.current) setInitFailure({ code: null, message: errorText(cause) });
    });
    setInitAttempt((value) => value + 1);
  }

  const initSubject = initSteps.find((item) => item.step === "subject") ?? null;
  const initSlice = initSteps.find((item) => item.step === "slice") ?? null;
  const initComplete = initState === "done";
  const initErrorLabel = initFailure
    ? initFailure.code
      ? t(INIT_ERROR_KEY[initFailure.code])
      : initFailure.message ?? ""
    : "";
  const stepLabel = (record: KuaishouInitStepRecord | null): string =>
    record ? t(INIT_STATE_KEY[record.state]) : t("kuaishou.init.state.pending");
  const stepTone = (record: KuaishouInitStepRecord | null): string =>
    record?.state === "done"
      ? "text-emerald-300"
      : record?.state === "failed"
        ? "text-amber-300"
        : record?.state === "running"
          ? "text-sky-300"
          : "text-slate-400";

  function buildProxy(): ProxyConfig | undefined {
    if (!proxyDraft.enabled) return undefined;
    if (!proxyDraft.host.trim()) throw new Error(t("kuaishou.wizard.proxyHostRequired"));
    return {
      type: proxyDraft.type,
      host: proxyDraft.host.trim(),
      port: Number(proxyDraft.port) || (proxyDraft.type === "http" ? 8080 : 1080),
      username: proxyDraft.username || undefined,
      password: proxyDraft.password || undefined,
    };
  }

  const proxyForTester: ProxyConfig | undefined =
    proxyDraft.enabled && proxyDraft.host
      ? {
          type: proxyDraft.type,
          host: proxyDraft.host,
          port: Number(proxyDraft.port) || (proxyDraft.type === "http" ? 8080 : 1080),
          username: proxyDraft.username || undefined,
          password: proxyDraft.password || undefined,
        }
      : undefined;

  async function submit(): Promise<void> {
    let built: ProxyConfig | undefined;
    try {
      built = buildProxy();
    } catch (cause) {
      setError(errorText(cause));
      setSection("proxy");
      return;
    }

    setError(null);
    setStep("creating");
    let createdId: string | null = null;
    try {
      // Chromix owns the identity. With a proxy set we ask it to resolve the
      // exit region (GeoIP) so timezone/locale align automatically — the native
      // Chromix alignment, no separate fingerprint reconciliation needed.
      const options = built ? { ...chromixOptions, geoip: true } : chromixOptions;

      // Create the profile with a placeholder name; the real name comes from
      // the detected account once the scan completes. No `fingerprint` is sent:
      // the CloakBrowser fingerprint is dead and Chromix holds the identity.
      const profile = await profilesApi.create({
        name: t("kuaishou.shop.unnamed"),
        proxy: built,
        startUrl: startUrl.trim() || undefined,
        extensions: extensions.length > 0 ? extensions : undefined,
        chromixOptions: options,
      });
      createdId = profile.id;
      created.current = true;
      setProfileId(profile.id);
      onCreated?.(profile.id);

      // Launch hidden and open the shop login page in a new tab.
      await profilesApi.launchKuaishou(profile.id, true);

      if (alive.current) setStep("waiting");
    } catch (cause) {
      if (alive.current) {
        setError(t("kuaishou.wizard.failed", { error: errorText(cause) }));
        // The profile may already exist: stay on "waiting" so the user can
        // still scan, rather than losing the created environment.
        setStep(createdId ? "waiting" : "form");
      }
    }
  }

  async function close(): Promise<void> {
    const id = profileId;
    // A cancelled onboarding must not leave a stray "unnamed" environment. Only
    // discard it when it was created by this wizard run and no account was ever
    // detected (never signed in); an account the user set up is never removed.
    const discard = created.current && !snapshot?.platformUserId;
    if (id) {
      await profilesApi.close(id).catch(() => {});
      if (discard) await profilesApi.delete(id).catch(() => {});
    }
    if (discard) onCreated?.("");
    onClose();
  }

  /** A different profile already reports this Kuaishou ID (warn, never block). */
  const duplicateOf = ((): string | null => {
    const id = snapshot?.platformUserId;
    if (!id || !profileId) return null;
    for (const [otherId, entry] of entries) {
      if (otherId !== profileId && entry.snapshot?.platformUserId === id) {
        return otherId;
      }
    }
    return null;
  })();

  return (
    <Modal
      open={open}
      title={t("kuaishou.wizard.title")}
      width={720}
      onClose={() => void close()}
    >
      {step === "form" ? (
        <div className="flex flex-col" style={{ height: SHEET_HEIGHT }}>
          <div className="flex flex-col sm:flex-row flex-1 min-h-0">
            <SectionRail
              section={section}
              onSelect={setSection}
              sections={sections}
              badges={{ proxy: proxyDraft.enabled && !proxyDraft.host.trim() }}
            />

            {/* Content pane — only this scrolls */}
            <div className="flex-1 min-h-0 min-w-0 overflow-y-auto px-5 py-4 text-[13px] text-slate-300">
              {error && (
                <p role="alert" className="mb-3 break-words text-[12px] text-red-300">
                  {error}
                </p>
              )}

              {section === "browser" && (
                <BrowserSection
                  label={t("kuaishou.wizard.section.home")}
                  defaultUrl={SHOP_HOME_URL}
                  hint={t("kuaishou.wizard.homeHint")}
                  startUrl={startUrl}
                  onStartUrl={setStartUrl}
                />
              )}

              {section === "proxy" && (
                <div className="space-y-2.5">
                  <label className="flex items-center gap-2 text-[12px] text-slate-300 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={proxyDraft.enabled}
                      onChange={(event) => {
                        setProxyDraft((current) => ({ ...current, enabled: event.target.checked }));
                        setError(null);
                      }}
                      className="w-3.5 h-3.5 rounded accent-[var(--accent)]"
                    />
                    {t("profile.proxy.useProxy")}
                  </label>

                  {proxyDraft.enabled && (
                    <>
                      <div className="grid grid-cols-[110px_1fr_90px] gap-2.5">
                        <Field label={t("profile.proxy.type")}>
                          <select
                            value={proxyDraft.type}
                            onChange={(event) =>
                              setProxyDraft((current) => ({
                                ...current,
                                type: event.target.value as "http" | "socks5",
                              }))
                            }
                            className="w-full px-2.5 h-9 rounded-lg bg-white/[0.03] text-[12px] text-slate-200 outline-none"
                            style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
                          >
                            <option value="http">HTTP</option>
                            <option value="socks5">SOCKS5</option>
                          </select>
                        </Field>
                        <Field label={t("profile.proxy.host")}>
                          <Input
                            value={proxyDraft.host}
                            onChange={(v) => {
                              setProxyDraft((current) => ({ ...current, host: v }));
                              setError(null);
                            }}
                            onPaste={(text) => {
                              const parsed = parseProxyString(text);
                              if (!parsed) return false; // let the default paste fill host
                              setProxyDraft((current) => ({
                                ...current,
                                type: parsed.type ?? current.type,
                                host: parsed.host,
                                port: String(parsed.port),
                                username: parsed.username ?? "",
                                password: parsed.password ?? "",
                              }));
                              return true;
                            }}
                            placeholder="host or host:port:user:pass"
                            mono
                          />
                        </Field>
                        <Field label={t("profile.proxy.port")}>
                          <Input
                            value={proxyDraft.port}
                            onChange={(v) => setProxyDraft((current) => ({ ...current, port: v }))}
                            placeholder="8080"
                            mono
                          />
                        </Field>
                      </div>
                      <div className="grid grid-cols-2 gap-2.5">
                        <Field label={t("profile.proxy.username")}>
                          <Input
                            value={proxyDraft.username}
                            onChange={(v) =>
                              setProxyDraft((current) => ({ ...current, username: v }))
                            }
                            mono
                          />
                        </Field>
                        <Field label={t("profile.proxy.password")}>
                          <Input
                            type="password"
                            value={proxyDraft.password}
                            onChange={(v) =>
                              setProxyDraft((current) => ({ ...current, password: v }))
                            }
                            mono
                          />
                        </Field>
                      </div>
                      <ProxyTester proxy={proxyForTester} />
                    </>
                  )}

                  <p className="text-[11px] text-slate-500">{t("kuaishou.wizard.proxyHint")}</p>
                </div>
              )}

              {/* Extensions — staged into the shared store and passed to create. */}
              {section === "extensions" && (
                <ExtensionsSection
                  profileId={null}
                  staged={extensions}
                  onStagedChange={setExtensions}
                />
              )}

              {section === "chromix" && (
                <div className="space-y-2.5">
                  <div className="flex items-center gap-2 rounded-lg border border-white/10 bg-white/[0.02] px-3 py-2.5">
                    <div className="min-w-0 flex-1">
                      <p className="mono truncate text-[11px] text-slate-300">
                        {fingerprintSeedOf(chromixOptions)
                          ? t("kuaishou.wizard.fp.randomized", { seed: fingerprintSeedOf(chromixOptions) as string })
                          : t("kuaishou.wizard.chromixHint")}
                      </p>
                      <p className="mt-0.5 text-[10px] leading-relaxed text-slate-500">
                        {t("kuaishou.wizard.fp.randomizeHint")}
                      </p>
                    </div>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={randomizeFingerprint}
                      leftIcon={<Dices size={12} />}
                    >
                      {t("kuaishou.wizard.fp.randomize")}
                    </Button>
                  </div>
                  <ChromixProfileOptions options={chromixOptions} onChange={setChromixOptions} />
                </div>
              )}
            </div>
          </div>

          {/* Action bar — always visible below the rail + content. */}
          <div
            className="shrink-0 flex items-center gap-3 px-5 py-3"
            style={{ borderTop: "1px solid rgba(255,255,255,0.06)" }}
          >
            <div className="flex-1 min-w-0" />
            <Button onClick={() => void close()}>{t("common.cancel")}</Button>
            <Button variant="primary" onClick={() => void submit()}>
              {t("kuaishou.wizard.create")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="p-5 space-y-4 min-w-0 text-[13px] text-slate-300">
          {error && (
            <p role="alert" className="break-words text-[12px] text-red-300">
              {error}
            </p>
          )}

          {(step === "creating" || step === "waiting") && (
            <div className="space-y-3">
              <div className="flex items-center gap-2 text-[12px] text-accent-foreground">
                <Loader2 size={14} className="animate-spin" />
                {step === "creating" ? t("kuaishou.wizard.creating") : t("kuaishou.wizard.opening")}
              </div>

              <div className="space-y-1.5">
                <p className="font-medium text-slate-200">{t("kuaishou.wizard.waitingTitle")}</p>
                <p className="text-[12px] text-slate-400 leading-relaxed">
                  {t("kuaishou.wizard.waitingHint")}
                </p>
              </div>

              <div
                data-testid="wizard-qr"
                className="flex items-center justify-center rounded-xl border border-white/10 bg-white/[0.02] p-3"
                style={{ minHeight: 220 }}
              >
                {qr ? (
                  <img
                    src={`data:image/png;base64,${qr}`}
                    alt={t("kuaishou.wizard.waitingTitle")}
                    className="rounded-lg"
                    // The page's QR image is only ~125px; upscale with crisp
                    // (pixelated) rendering so it stays scannable.
                    style={{ width: 260, height: 260, imageRendering: "pixelated" }}
                  />
                ) : (
                  <span className="text-[12px] text-slate-500">{t("kuaishou.wizard.qrPending")}</span>
                )}
              </div>

              <Button
                size="sm"
                disabled={!profileId}
                onClick={() => setQr(null)}
                leftIcon={<RefreshCw size={11} />}
              >
                {t("kuaishou.wizard.refreshQr")}
              </Button>
            </div>
          )}

          {step === "done" && (
            <div className="space-y-3">
              <div className="flex items-center gap-3 min-w-0">
                <IdentityAvatar snapshot={snapshot} size={44} />
                <div className="min-w-0">
                  <p className="text-[12px] text-emerald-300">
                    {named ? t("kuaishou.wizard.named") : t("kuaishou.wizard.detected")}
                  </p>
                  <p className="text-[14px] font-semibold text-slate-100 truncate">
                    {snapshot?.nickname || snapshot?.platformUserId}
                  </p>
                  <p className="mono text-[11px] text-slate-400 truncate">
                    {snapshot?.platformUserId}
                  </p>
                </div>
              </div>

              {duplicateOf && (
                <p role="alert" className="text-[12px] text-amber-300 break-words">
                  {t("kuaishou.wizard.duplicate", { name: duplicateOf })}
                </p>
              )}

              {/* Initialization progress: the wizard keeps the hidden browser
               *  alive and drives the catch-up run until both steps are done. */}
              <div
                data-testid="wizard-init"
                data-state={initState}
                className="space-y-2 rounded-xl border border-white/10 bg-white/[0.02] px-3 py-2.5"
              >
                <div className="flex items-center gap-2 text-[12px]">
                  {initState === "running" ? (
                    <Loader2 size={13} className="animate-spin text-sky-300" />
                  ) : initState === "done" ? (
                    <Check size={13} className="text-emerald-300" />
                  ) : (
                    <CircleAlert size={13} className="text-amber-300" />
                  )}
                  <span
                    className={
                      initState === "running"
                        ? "text-slate-200"
                        : initState === "done"
                          ? "text-emerald-300"
                          : "text-amber-300"
                    }
                  >
                    {initState === "running"
                      ? t("kuaishou.wizard.initPending")
                      : initState === "done"
                        ? t("kuaishou.wizard.initDone")
                        : initState === "timeout"
                          ? t("kuaishou.wizard.initTimeout")
                          : t("kuaishou.wizard.initFailed", { error: initErrorLabel })}
                  </span>
                </div>

                <ul className="space-y-1 text-[11px]">
                  {[initSubject, initSlice].map((record, index) => (
                    <li
                      key={index === 0 ? "subject" : "slice"}
                      data-testid={`wizard-init-step-${index === 0 ? "subject" : "slice"}`}
                      data-state={record?.state ?? "pending"}
                      className="flex items-center justify-between gap-2"
                    >
                      <span className="text-slate-400">
                        {t(index === 0 ? "kuaishou.init.step.subject" : "kuaishou.init.step.slice")}
                      </span>
                      <span className={stepTone(record)}>{stepLabel(record)}</span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}

          <div className="flex justify-end gap-2 pt-1">
            {step === "done" ? (
              <>
                <span className="flex-1 self-center text-[11px] text-slate-500" />
                {initState === "failed" && (
                  <Button size="sm" onClick={retryInit} leftIcon={<RefreshCw size={11} />}>
                    {t("kuaishou.wizard.initRetry")}
                  </Button>
                )}
                <Button
                  variant={initComplete ? "primary" : "secondary"}
                  onClick={() => void close()}
                >
                  {initComplete
                    ? t("kuaishou.wizard.done")
                    : t("kuaishou.wizard.closeWhileInit")}
                </Button>
              </>
            ) : (
              <>
                <span className="flex-1 self-center text-[11px] text-slate-500">
                  {created.current && !snapshot?.platformUserId ? t("kuaishou.wizard.cancelCreated") : ""}
                </span>
                <Button onClick={() => void close()}>{t("common.cancel")}</Button>
              </>
            )}
          </div>
        </div>
      )}
    </Modal>
  );
}
