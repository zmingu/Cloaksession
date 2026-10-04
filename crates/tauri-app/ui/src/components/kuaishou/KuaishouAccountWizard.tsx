import { useEffect, useRef, useState, type JSX } from "react";
import { Loader2, RefreshCw } from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import { cn } from "../../lib/cn";
import { fingerprint as fingerprintApi, profiles as profilesApi, proxy as proxyApi } from "../../lib/ipc";
import { kuaishouAuth } from "../../lib/kuaishouAuth";
import { kuaishouIdentity, type KuaishouIdentitySnapshot } from "../../lib/kuaishouIdentity";
import { useKuaishouIdentities } from "../../lib/KuaishouIdentityProvider";
import { parseProxyString } from "../../lib/parseProxy";
import type { FingerprintConfig, ProxyConfig } from "../../types";
import { Modal } from "../atoms";
import { Button } from "../atoms/Button";
import { IdentityAvatar } from "../profile/KuaishouIdentity";
import { ProxyTester } from "../profile/ProxyTester";

/** How often the wizard re-captures the login page while waiting for a scan. */
const QR_POLL_MS = 2000;

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

const CONTROL =
  "w-full min-w-0 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-2 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 小店账号建号向导（快手 › 小店 › 添加账号）。
 *
 * 账号即环境：向导创建的就是一个浏览器环境，列表里一个环境 = 一个小店账号。
 * 流程：命名 + 代理 → 创建环境（指纹自动随机、按代理出口对齐）→ 隐藏启动
 * （窗口移出屏幕）→ 向导内轮询截图二维码 → 自动读出快手ID/昵称/头像 → 完成。
 *
 * 初始化（主体采集 / 切片权限）由既有的「识别到有效账号后自动补做」机制处理，
 * 本向导不驱动。
 */
export function KuaishouAccountWizard({ open, onClose, onCreated }: Props): JSX.Element {
  const t = useT();
  const { entries } = useKuaishouIdentities();

  const [step, setStep] = useState<Step>("form");
  const [name, setName] = useState("");
  const [proxyDraft, setProxyDraft] = useState<DraftProxy>(EMPTY_PROXY);
  const [profileId, setProfileId] = useState<string | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  // The wizard drives detection itself rather than waiting on the app-wide
  // 15s poll, so the sign-in completes the moment the QR is scanned.
  const [snapshot, setSnapshot] = useState<KuaishouIdentitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);

  const alive = useRef(true);
  // A created profile must survive a close: we only hide/stop the browser.
  const created = useRef(false);

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
    setName("");
    setProxyDraft(EMPTY_PROXY);
    setProfileId(null);
    setQr(null);
    setSnapshot(null);
    setError(null);
    created.current = false;
  }, [open]);

  const platformUserId = snapshot?.platformUserId ?? null;

  // Move to "done" once a real identity is read for our profile.
  useEffect(() => {
    if (platformUserId) setStep("done");
  }, [platformUserId]);

  // While waiting, keep re-capturing the hidden page (for the QR) and asking
  // the backend whether the profile has signed in yet. Stops on "done", on
  // unmount, and when the wizard closes — so no timer or capture leaks.
  useEffect(() => {
    // `open` is part of the gate: the wizard component stays mounted (the
    // Modal only renders null), so closing must tear the timer down here.
    if (!open || step !== "waiting" || !profileId) return;
    let active = true;
    let timer = 0;
    const tick = async (): Promise<void> => {
      try {
        const image = await kuaishouAuth.loginQr(profileId);
        if (active && image) setQr(image);
      } catch {
        // A transient capture failure must not abort the wait; keep polling.
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
    if (!name.trim()) {
      setError(t("kuaishou.wizard.nameRequired"));
      return;
    }
    let built: ProxyConfig | undefined;
    try {
      built = buildProxy();
    } catch (cause) {
      setError(errorText(cause));
      return;
    }

    setError(null);
    setStep("creating");
    let createdId: string | null = null;
    try {
      // 1. Random-but-coherent fingerprint, aligned to the proxy's exit region
      //    when a proxy is set — this is the "指纹一致性" requirement.
      let fingerprint: FingerprintConfig = await fingerprintApi.generate();
      if (built) {
        const geo = await proxyApi.detectGeo(built);
        if (geo.country) {
          const localeId = await fingerprintApi.localeForCountry(geo.country);
          fingerprint = await fingerprintApi.reconcile(fingerprint, {
            ...(localeId ? { localeId } : {}),
            ...(geo.timezone ? { timezone: geo.timezone } : {}),
            country: geo.country,
          });
        }
      }

      // 2. Create the profile (the account itself).
      const profile = await profilesApi.create({
        name: name.trim(),
        proxy: built,
        fingerprint,
      });
      createdId = profile.id;
      created.current = true;
      setProfileId(profile.id);
      onCreated?.(profile.id);

      // 3. Launch hidden and open the shop login page in a new tab.
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
    // Stop the hidden browser so it does not linger; the account (profile) stays.
    if (id) await profilesApi.close(id).catch(() => {});
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
      width={560}
      onClose={() => void close()}
    >
      <div className="p-5 space-y-4 min-w-0 text-[13px] text-slate-300">
        {error && (
          <p role="alert" className="break-words text-[12px] text-red-300">
            {error}
          </p>
        )}

        {step === "form" && (
          <div className="space-y-3.5">
            <label className="block space-y-1.5">
              <span className="text-slate-400">{t("kuaishou.wizard.name")}</span>
              <input
                autoFocus
                className={CONTROL}
                value={name}
                onChange={(event) => {
                  setName(event.target.value);
                  setError(null);
                }}
                placeholder={t("kuaishou.wizard.namePlaceholder")}
              />
            </label>

            <div className="space-y-2">
              <label className="flex items-center gap-2 text-slate-300 cursor-pointer">
                <input
                  type="checkbox"
                  checked={proxyDraft.enabled}
                  onChange={(event) => {
                    setProxyDraft((current) => ({ ...current, enabled: event.target.checked }));
                    setError(null);
                  }}
                  className="w-3.5 h-3.5 rounded accent-[var(--accent)]"
                />
                {t("kuaishou.wizard.proxy")}
              </label>

              {proxyDraft.enabled && (
                <div className="space-y-2.5">
                  <div className="grid grid-cols-[110px_1fr_90px] gap-2.5">
                    <select
                      className={CONTROL}
                      value={proxyDraft.type}
                      onChange={(event) =>
                        setProxyDraft((current) => ({
                          ...current,
                          type: event.target.value as "http" | "socks5",
                        }))
                      }
                    >
                      <option value="http">HTTP</option>
                      <option value="socks5">SOCKS5</option>
                    </select>
                    <input
                      className={cn(CONTROL, "mono")}
                      value={proxyDraft.host}
                      onChange={(event) => {
                        setProxyDraft((current) => ({ ...current, host: event.target.value }));
                        setError(null);
                      }}
                      onPaste={(event) => {
                        const parsed = parseProxyString(event.clipboardData.getData("text"));
                        if (!parsed) return;
                        event.preventDefault();
                        setProxyDraft((current) => ({
                          ...current,
                          type: parsed.type ?? current.type,
                          host: parsed.host,
                          port: String(parsed.port),
                          username: parsed.username ?? "",
                          password: parsed.password ?? "",
                        }));
                      }}
                      placeholder="host or host:port:user:pass"
                    />
                    <input
                      className={cn(CONTROL, "mono")}
                      value={proxyDraft.port}
                      onChange={(event) =>
                        setProxyDraft((current) => ({ ...current, port: event.target.value }))
                      }
                      placeholder="8080"
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-2.5">
                    <input
                      className={cn(CONTROL, "mono")}
                      value={proxyDraft.username}
                      onChange={(event) =>
                        setProxyDraft((current) => ({ ...current, username: event.target.value }))
                      }
                      placeholder={t("profile.proxy.username")}
                    />
                    <input
                      type="password"
                      className={cn(CONTROL, "mono")}
                      value={proxyDraft.password}
                      onChange={(event) =>
                        setProxyDraft((current) => ({ ...current, password: event.target.value }))
                      }
                      placeholder={t("profile.proxy.password")}
                    />
                  </div>
                  <p className="text-[11px] text-slate-500">{t("kuaishou.wizard.proxyHint")}</p>
                  <ProxyTester proxy={proxyForTester} />
                </div>
              )}
            </div>
          </div>
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
                  className="max-h-[360px] max-w-full rounded-lg"
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
                <p className="text-[12px] text-emerald-300">{t("kuaishou.wizard.detected")}</p>
                <p className="text-[14px] font-semibold text-slate-100 truncate">
                  {snapshot?.nickname || name}
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
          </div>
        )}

        <div className="flex justify-end gap-2 pt-1">
          {step === "form" ? (
            <>
              <Button onClick={() => void close()}>{t("common.cancel")}</Button>
              <Button variant="primary" onClick={() => void submit()}>
                {t("kuaishou.wizard.create")}
              </Button>
            </>
          ) : step === "done" ? (
            <Button variant="primary" onClick={() => void close()}>
              {t("kuaishou.wizard.done")}
            </Button>
          ) : (
            <>
              <span className="flex-1 self-center text-[11px] text-slate-500">
                {created.current ? t("kuaishou.wizard.cancelCreated") : ""}
              </span>
              <Button onClick={() => void close()}>{t("common.cancel")}</Button>
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
