import { useEffect, useRef, useState, type JSX } from "react";
import {
  Blocks,
  Dices,
  Eye,
  Fingerprint,
  Globe,
  Loader2,
  Network,
  RefreshCw,
  type LucideIcon,
} from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import { fingerprintSeedOf, withRandomFingerprintSeed } from "../../lib/chromixSeed";
import { profiles as profilesApi } from "../../lib/ipc";
import { kuaishouAuth } from "../../lib/kuaishouAuth";
import { kuaishouIdentity, type KuaishouIdentitySnapshot } from "../../lib/kuaishouIdentity";
import { useKuaishouIdentities } from "../../lib/KuaishouIdentityProvider";
import { parseProxyString } from "../../lib/parseProxy";
import { subAccounts } from "../../lib/subAccounts";
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

/** How long the success state stays on screen before the wizard closes itself. */
const AUTO_CLOSE_MS = 1200;

/** Prefilled, editable home page for a viewer account: the Kuaishou main site. */
const SUB_HOME_URL = "https://www.kuaishou.com/";

type Step = "form" | "creating" | "waiting" | "done";

interface Props {
  open: boolean;
  onClose: () => void;
  /** Fired once the environment exists so the caller can refresh its list. */
  onCreated?: (profileId: string) => void;
  /** Fired once the interact registration has been written. */
  onRegistered?: (profileId: string) => void;
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
 * 互动账号建号向导（快手 › 互动账号 › 新建账号）。
 *
 * 账号即环境：向导创建的就是一个浏览器环境，列表里一个环境 = 一个互动账号。
 * 与「小店建号向导」同构（分区表单 → 创建 → 隐藏启动 → 向导内轮询二维码 →
 * 识别 → 登记），区别只在三处：
 *  - 主页预填快手主站 `https://www.kuaishou.com/`，启动入口用
 *    `profiles_launch(entry: "kuaishou-sub")`（后端新标签打开主站）；
 *  - 主站不缓存头像，识别退化时用首字母头像；
 *  - 成功后除了给环境改名，还要写 `business_accounts(kind="kuaishou-sub")`
 *    的互动登记（`save_sub_account`）——这是列表的数据真相。
 *
 * 二维码复用 `kuaishou_login_qr`：主站登录页与小店登录页同构，能取到就内嵌；
 * 取不到（或用户不想等）可一键改用可见窗口扫码，向导继续轮询识别。
 * 身份检测成功即自动登记互动账号并自动关闭向导（无需手动点登记/完成）；
 * 检测失败则保持等待，由用户取消（取消会删除本次创建且未登记的环境）。
 */
export function KuaishouInteractWizard({ open, onClose, onCreated, onRegistered }: Props): JSX.Element {
  const t = useT();
  const { entries } = useKuaishouIdentities();

  const [step, setStep] = useState<Step>("form");
  const [section, setSection] = useState<SectionId>("browser");
  const [startUrl, setStartUrl] = useState(SUB_HOME_URL);
  const [proxyDraft, setProxyDraft] = useState<DraftProxy>(EMPTY_PROXY);
  const [extensions, setExtensions] = useState<ExtensionConfig[]>([]);
  const [chromixOptions, setChromixOptions] = useState<Record<string, unknown>>({});
  const [profileId, setProfileId] = useState<string | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  // The wizard drives detection itself rather than waiting on the app-wide 15s
  // poll, so the registration can complete the moment the QR is scanned.
  const [snapshot, setSnapshot] = useState<KuaishouIdentitySnapshot | null>(null);
  const [visibleWindow, setVisibleWindow] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [registeredName, setRegisteredName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const alive = useRef(true);
  // A created profile must survive a close: we only hide/stop the browser.
  const created = useRef(false);
  // One rename per wizard run; guarded by a ref so the effect cannot double-fire.
  const renamed = useRef(false);
  // One auto-registration per wizard run; guarded so the effect cannot re-enter.
  const registered = useRef(false);
  // Flips synchronously on a successful write so `close()` (which may run from a
  // timer closure holding a stale `registeredName` state) never discards a
  // registered environment.
  const saved = useRef(false);
  // The self-close timer scheduled after a successful auto-registration.
  const closeTimer = useRef(0);

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
      window.clearTimeout(closeTimer.current);
    };
  }, []);

  // Reset everything when the wizard is (re)opened.
  useEffect(() => {
    if (!open) return;
    setStep("form");
    setSection("browser");
    setStartUrl(SUB_HOME_URL);
    setProxyDraft(EMPTY_PROXY);
    setExtensions([]);
    // Auto-random fingerprint: write a fresh `--fingerprint=<seed>` so Chromix
    // derives a coherent random device instead of reusing a persistent seed.
    setChromixOptions(withRandomFingerprintSeed({}));
    setProfileId(null);
    setQr(null);
    setSnapshot(null);
    setVisibleWindow(false);
    setSwitching(false);
    setRegisteredName(null);
    setError(null);
    created.current = false;
    renamed.current = false;
    registered.current = false;
    saved.current = false;
  }, [open]);

  const platformUserId = snapshot?.platformUserId ?? null;

  /** One-click random fingerprint: replace the seed with a fresh one. */
  function randomizeFingerprint(): void {
    setChromixOptions((current) => withRandomFingerprintSeed(current));
  }

  // Name the environment from the detected identity: nickname when present, else
  // the Kuaishou ID. Best-effort — a failure leaves the placeholder name.
  useEffect(() => {
    if (!platformUserId || !profileId || renamed.current) return;
    const next = (snapshot?.nickname || snapshot?.platformUserId || "").trim();
    if (!next) return;
    renamed.current = true;
    void (async () => {
      try {
        await profilesApi.update(profileId, { name: next });
      } catch {
        renamed.current = false;
      }
    })();
  }, [platformUserId, profileId, snapshot]);

  // Auto-register the moment the identity is read: write the interact record from
  // the detected identity, show the success state, then close the wizard on its
  // own. Guarded by a ref so it runs once per wizard run; a failure keeps the
  // wizard open and surfaces the error (the next poll retries, the user can also
  // cancel — cancelling discards the environment this run created).
  useEffect(() => {
    if (!platformUserId || !profileId || registered.current) return;
    const id = profileId;
    const pid = platformUserId;
    const name = (snapshot?.nickname || pid).trim();
    if (!name) return;
    registered.current = true;
    void (async () => {
      try {
        // The business-account guard refuses to bind while the profile is running
        // (`business_guard::require_stopped`), so stop the hidden browser first —
        // the identity has already been read and stays persisted. Cancelling after
        // this discards the just-created environment anyway.
        await profilesApi.close(id).catch(() => {});
        await subAccounts.save({
          profileId: id,
          kind: "kuaishou-sub",
          displayName: name,
          platformUserId: pid,
        });
        if (!alive.current) return;
        saved.current = true;
        setRegisteredName(name);
        setStep("done");
        onRegistered?.(id);
        closeTimer.current = window.setTimeout(() => void close(), AUTO_CLOSE_MS);
      } catch (cause) {
        // Let a later detection retry the registration.
        registered.current = false;
        if (alive.current) {
          setError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
        }
      }
    })();
  }, [platformUserId, profileId, snapshot, onRegistered, t]);

  // While waiting, keep re-reading the page (for the QR) and asking the backend
  // whether the profile has signed in yet. Stops on unmount and when the wizard
  // closes — so no timer or read leaks.
  useEffect(() => {
    // `open` is part of the gate: the wizard component stays mounted (the Modal
    // only renders null), so closing must tear the timer down here.
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
      // exit region (GeoIP) so timezone/locale align automatically.
      const options = built ? { ...chromixOptions, geoip: true } : chromixOptions;

      // Create the profile with a placeholder name; the real name comes from
      // the detected account once the scan completes.
      const profile = await profilesApi.create({
        name: t("kuaishou.interactWizard.unnamed"),
        proxy: built,
        startUrl: startUrl.trim() || undefined,
        extensions: extensions.length > 0 ? extensions : undefined,
        chromixOptions: options,
      });
      createdId = profile.id;
      created.current = true;
      setProfileId(profile.id);
      onCreated?.(profile.id);

      // Launch hidden and open the Kuaishou main-site login page in a new tab.
      await profilesApi.launchKuaishouSub(profile.id, true);

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

  /**
   * Fallback when the hidden window yields no QR: relaunch the profile in a
   * visible window and let the user scan there. Close first — a Chromix profile
   * has one persistent context, so a second launch without a close would not
   * bring the off-screen window into view.
   */
  async function useVisibleWindow(): Promise<void> {
    const id = profileId;
    if (!id || switching) return;
    setSwitching(true);
    setError(null);
    try {
      await profilesApi.close(id).catch(() => {});
      await profilesApi.launchKuaishouSub(id, false);
      if (alive.current) setVisibleWindow(true);
    } catch (cause) {
      if (alive.current) setError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      if (alive.current) setSwitching(false);
    }
  }

  async function close(): Promise<void> {
    window.clearTimeout(closeTimer.current);
    const id = profileId;
    // A cancelled onboarding must not leave a stray "unnamed" environment. Only
    // discard it when it was created by this wizard run and the account was never
    // signed in / registered; a profile the user may have set up is never removed.
    // `saved` (a ref) is authoritative here because `close()` may run from the
    // auto-close timer, whose closure would otherwise see a stale `registeredName`.
    const discard = created.current && !registeredName && !saved.current;
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
      title={t("kuaishou.interactWizard.title")}
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
                  defaultUrl={SUB_HOME_URL}
                  hint={t("kuaishou.interactWizard.homeHint")}
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
                          ? t("kuaishou.wizard.fp.randomized", {
                              seed: fingerprintSeedOf(chromixOptions) as string,
                            })
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
                {step === "creating"
                  ? t("kuaishou.wizard.creating")
                  : t("kuaishou.interactWizard.opening")}
              </div>

              <div className="space-y-1.5">
                <p className="font-medium text-slate-200">
                  {t("kuaishou.interactWizard.waitingTitle")}
                </p>
                <p className="text-[12px] text-slate-400 leading-relaxed">
                  {visibleWindow
                    ? t("kuaishou.interactWizard.visibleHint")
                    : t("kuaishou.interactWizard.waitingHint")}
                </p>
              </div>

              <div
                data-testid="interact-wizard-qr"
                className="flex items-center justify-center rounded-xl border border-white/10 bg-white/[0.02] p-3"
                style={{ minHeight: 220 }}
              >
                {qr ? (
                  <img
                    src={`data:image/png;base64,${qr}`}
                    alt={t("kuaishou.interactWizard.waitingTitle")}
                    className="rounded-lg"
                    // The page's QR image is only ~125px; upscale with crisp
                    // (pixelated) rendering so it stays scannable.
                    style={{ width: 260, height: 260, imageRendering: "pixelated" }}
                  />
                ) : (
                  <span className="px-4 text-center text-[12px] text-slate-500">
                    {visibleWindow
                      ? t("kuaishou.interactWizard.visibleHint")
                      : t("kuaishou.interactWizard.qrMissHint")}
                  </span>
                )}
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  size="sm"
                  disabled={!profileId || switching || visibleWindow}
                  onClick={() => void useVisibleWindow()}
                  leftIcon={
                    switching ? (
                      <Loader2 size={11} className="animate-spin" />
                    ) : (
                      <Eye size={11} />
                    )
                  }
                >
                  {t("kuaishou.interactWizard.useVisible")}
                </Button>
                <Button
                  size="sm"
                  disabled={!profileId}
                  onClick={() => setQr(null)}
                  leftIcon={<RefreshCw size={11} />}
                >
                  {t("kuaishou.wizard.refreshQr")}
                </Button>
                <div className="flex-1" />
                {platformUserId ? (
                  <span className="text-[12px] text-emerald-300">
                    {t("kuaishou.interact.status.detected")}
                  </span>
                ) : (
                  <span className="text-[12px] text-slate-500">
                    {t("kuaishou.interact.status.detecting")}
                  </span>
                )}
              </div>

              {/* Detected identity preview: registration is automatic, so this is
                  a read-only confirmation of what was just registered. */}
              {platformUserId && (
                <div className="flex items-center gap-3 min-w-0 rounded-lg border border-white/10 bg-white/[0.02] p-3">
                  <IdentityAvatar snapshot={snapshot} size={36} />
                  <div className="min-w-0">
                    <p className="truncate text-[13px] font-semibold text-slate-100">
                      {snapshot?.nickname || snapshot?.platformUserId}
                    </p>
                    <p className="mono truncate text-[11px] text-slate-400">{platformUserId}</p>
                  </div>
                </div>
              )}

              {duplicateOf && (
                <p role="alert" className="text-[12px] text-amber-300 break-words">
                  {t("kuaishou.wizard.duplicate", { name: duplicateOf })}
                </p>
              )}
            </div>
          )}

          {step === "done" && (
            <div className="space-y-3">
              <div className="flex items-center gap-3 min-w-0">
                <IdentityAvatar snapshot={snapshot} size={44} />
                <div className="min-w-0">
                  <p className="text-[12px] text-emerald-300">
                    {t("kuaishou.interactWizard.registered")}
                  </p>
                  <p className="text-[14px] font-semibold text-slate-100 truncate">
                    {registeredName ?? snapshot?.nickname ?? snapshot?.platformUserId}
                  </p>
                  <p className="mono text-[11px] text-slate-400 truncate">
                    {snapshot?.platformUserId}
                  </p>
                </div>
              </div>
            </div>
          )}

          <div className="flex justify-end gap-2 pt-1">
            {step !== "done" && (
              <>
                <span className="flex-1 self-center text-[11px] text-slate-500">
                  {created.current && !registeredName ? t("kuaishou.interactWizard.cancelCreated") : ""}
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
