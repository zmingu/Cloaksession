import { useEffect, useRef, useState, type JSX } from "react";
import { Eye, Loader2 } from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import { profiles as profilesApi } from "../../lib/ipc";
import { kuaishouAuth } from "../../lib/kuaishouAuth";
import { kuaishouIdentity, type KuaishouIdentitySnapshot } from "../../lib/kuaishouIdentity";
import { subAccounts } from "../../lib/subAccounts";
import { Modal } from "../atoms";
import { Button } from "../atoms/Button";
import { IdentityAvatar } from "../profile/KuaishouIdentity";

/** How often the dialog re-captures the login page while waiting for a scan. */
const QR_POLL_MS = 2000;

/** How long the success state stays on screen before the dialog auto-closes. */
const DONE_CLOSE_MS = 1200;

type Step = "binding" | "waiting" | "done" | "failed";

interface Props {
  open: boolean;
  profileId: string;
  profileName: string;
  onClose: () => void;
  /** Fired after the interact registration has been written (auto-close). */
  onConverted: () => void;
}

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 小店账号 → 互动账号 转换弹窗（快手 › 小店 › 转为互动账号）。
 *
 * 一个已有环境（小店视角）在这里原地变成互动账号，**不新建环境、不删除环境**：
 *  0. 先关闭该环境（业务账号守卫要求「已停止」才能绑定：`require_stopped`）；
 *  1. 再写互动登记 `save_sub_account(kind="kuaishou-sub")`——绑定后身份读取口径
 *     从 `Both`（小店页+主站页）收窄为 `ViewerOnly`（只读 www.kuaishou.com 主站），
 *     因此先绑定可避开环境里残留的小店页身份冲突；
 *  2. 隐藏启动 `profiles_launch(entry="kuaishou-sub")` 打开快手主站登录页；
 *  3. 轮询二维码（`kuaishou_login_qr`）与主站身份（`kuaishou_identity_detect`）；
 *  4. 识别到 `platformUserId` 后回写登记（补 platformUserId + 用昵称命名），
 *     展示「已转为互动账号」并自动关闭，回调 `onConverted()` 让列表刷新。
 *
 * 取消（取消按钮 / X / ESC / 点击遮罩）只做两件事：关闭浏览器
 * （`profiles_close`）+ 回退本次绑定（`unbind_sub_account`）。**绝不**
 * `profiles_delete`——这是用户既有的小店环境。转换成功后关闭不再解绑。
 *
 * Modal 关闭时不卸载子组件，因此所有定时器/异步都在 `open` 的清理函数里收口。
 */
export function KuaishouConvertInteractDialog({
  open,
  profileId,
  profileName,
  onClose,
  onConverted,
}: Props): JSX.Element {
  const t = useT();
  const tRef = useRef(t);
  tRef.current = t;

  const [step, setStep] = useState<Step>("binding");
  const [qr, setQr] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<KuaishouIdentitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [visibleWindow, setVisibleWindow] = useState(false);
  const [switching, setSwitching] = useState(false);

  const mounted = useRef(false);
  // Valid only while the dialog is open; guards every async continuation.
  const alive = useRef(false);
  // The profile id this run is bound to. Used to tell a genuine (re)open from a
  // StrictMode effect replay, so the replay never re-binds or re-launches.
  const runKey = useRef<string | null>(null);
  // The interact record created by this run (bind on open, unbind on cancel).
  const recordId = useRef<string | null>(null);
  // In-flight guards: one bind+launch at a time, one terminal action at a time.
  const binding = useRef(false);
  const finishing = useRef(false);
  const closed = useRef(false);
  const converted = useRef(false);
  const closeTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (closeTimer.current !== undefined) window.clearTimeout(closeTimer.current);
    };
  }, []);

  /** Bind (once) then launch the hidden main-site window. Retry-safe. */
  async function begin(): Promise<void> {
    if (binding.current || finishing.current) return;
    binding.current = true;
    setError(null);
    setStep("binding");
    try {
      // The business-account guard refuses to bind while the profile is running
      // (`business_guard::require_stopped`), so stop any running session first —
      // the dialog then relaunches it hidden on the main site below.
      await profilesApi.close(profileId).catch(() => {});
      if (!recordId.current) {
        // Bind first so the identity read scope narrows to the main site before
        // the browser touches www.kuaishou.com.
        const record = await subAccounts.save({
          profileId,
          kind: "kuaishou-sub",
          displayName: profileName,
          platformUserId: null,
        });
        recordId.current = record.id;
      }
      await profilesApi.launchKuaishouSub(profileId, true);
      if (alive.current) setStep("waiting");
    } catch (cause) {
      if (alive.current) {
        setError(tRef.current("kuaishou.convert.failed", { error: errorText(cause) }));
        setStep("failed");
      }
    } finally {
      binding.current = false;
    }
  }
  const beginRef = useRef(begin);
  beginRef.current = begin;

  // Reset + start on open. `open` is a dependency because the Modal keeps this
  // component mounted (it only renders null), so closing must tear the run down.
  useEffect(() => {
    if (!open) {
      // A genuine close lets the same profile start a fresh run later.
      runKey.current = null;
      alive.current = false;
      return;
    }
    // StrictMode replays setup/cleanup on the same instance. Re-assert `alive`
    // (cleanup just cleared it) but skip the restart so the replay never
    // double-binds or re-launches the environment.
    alive.current = true;
    if (runKey.current === profileId) return;
    runKey.current = profileId;
    setStep("binding");
    setQr(null);
    setSnapshot(null);
    setError(null);
    setVisibleWindow(false);
    setSwitching(false);
    recordId.current = null;
    binding.current = false;
    finishing.current = false;
    closed.current = false;
    converted.current = false;
    if (closeTimer.current !== undefined) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = undefined;
    }
    void beginRef.current();
    return () => {
      alive.current = false;
    };
  }, [open, profileId]);

  // While waiting, keep re-reading the page (for the QR) and asking the backend
  // whether the profile has signed in yet. Stops on close and on unmount.
  useEffect(() => {
    if (!open || step !== "waiting" || !profileId) return;
    let active = true;
    let timer = 0;
    const tick = async (): Promise<void> => {
      try {
        // A `null` means "no valid QR right now" (not rendered yet, or the page
        // auto-refreshed an expired one) — clear the frame instead of keeping a
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

  // Once a main-site identity is read, finalize the registration and auto-close.
  useEffect(() => {
    if (!open) return;
    const platformUserId = snapshot?.platformUserId;
    if (!platformUserId || finishing.current) return;
    finishing.current = true;
    void (async () => {
      try {
        const record = await subAccounts.save({
          id: recordId.current ?? undefined,
          profileId,
          kind: "kuaishou-sub",
          displayName: (snapshot?.nickname || platformUserId).trim(),
          platformUserId,
        });
        recordId.current = record.id;
        converted.current = true;
        if (!alive.current) return;
        setStep("done");
        closeTimer.current = window.setTimeout(() => {
          if (!alive.current) return;
          onConverted();
          onClose();
        }, DONE_CLOSE_MS);
      } catch (cause) {
        finishing.current = false;
        if (alive.current) {
          setError(tRef.current("kuaishou.convert.failed", { error: errorText(cause) }));
        }
      }
    })();
    // `snapshot` is the trigger; refs/`t` are read inside and must not re-run it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, snapshot, profileId]);

  /**
   * Fallback when the hidden window yields no QR: relaunch in a visible window.
   * Close first — a Chromix profile has one persistent context, so a second
   * launch without a close would not bring the off-screen window into view.
   */
  async function useVisibleWindow(): Promise<void> {
    if (switching) return;
    setSwitching(true);
    setError(null);
    try {
      await profilesApi.close(profileId).catch(() => {});
      await profilesApi.launchKuaishouSub(profileId, false);
      if (alive.current) setVisibleWindow(true);
    } catch (cause) {
      if (alive.current) {
        setError(tRef.current("kuaishou.convert.failed", { error: errorText(cause) }));
      }
    } finally {
      if (alive.current) setSwitching(false);
    }
  }

  /**
   * Cancel / close: close the browser and roll back this run's bind. The profile
   * itself is never deleted — it is a pre-existing shop environment.
   */
  async function cancel(): Promise<void> {
    if (closed.current) return;
    closed.current = true;
    if (closeTimer.current !== undefined) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = undefined;
    }
    await profilesApi.close(profileId).catch(() => {});
    if (!converted.current && recordId.current) {
      await subAccounts.unbind(recordId.current).catch(() => {});
    }
    onClose();
  }

  const platformUserId = snapshot?.platformUserId ?? null;

  return (
    <Modal
      open={open}
      title={t("kuaishou.convert.title")}
      subtitle={profileName}
      width={520}
      onClose={() => void cancel()}
    >
      <div className="space-y-4 p-5 text-[13px] text-slate-300">
        {error && (
          <p role="alert" className="break-words text-[12px] text-red-300">
            {error}
          </p>
        )}

        <p className="text-[12px] leading-relaxed text-slate-400">{t("kuaishou.convert.hint")}</p>

        {(step === "binding" || step === "waiting") && (
          <div className="space-y-3">
            <div className="flex items-center gap-2 text-[12px] text-accent-foreground">
              <Loader2 size={14} className="animate-spin" />
              {step === "binding" ? t("kuaishou.wizard.creating") : t("kuaishou.convert.waiting")}
            </div>

            <div
              data-testid="convert-qr"
              className="flex items-center justify-center rounded-xl border border-white/10 bg-white/[0.02] p-3"
              style={{ minHeight: 220 }}
            >
              {qr ? (
                <img
                  src={`data:image/png;base64,${qr}`}
                  alt={t("kuaishou.convert.waiting")}
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
                disabled={switching || visibleWindow}
                onClick={() => void useVisibleWindow()}
                leftIcon={switching ? <Loader2 size={11} className="animate-spin" /> : <Eye size={11} />}
              >
                {t("kuaishou.convert.useVisible")}
              </Button>
              <div className="flex-1" />
              <span className="text-[12px] text-slate-500">
                {t("kuaishou.interact.status.detecting")}
              </span>
            </div>
          </div>
        )}

        {step === "done" && (
          <div className="flex items-center gap-3 min-w-0">
            <IdentityAvatar snapshot={snapshot} size={40} />
            <div className="min-w-0">
              <p className="text-[12px] text-emerald-300">{t("kuaishou.convert.done")}</p>
              <p className="truncate text-[14px] font-semibold text-slate-100">
                {snapshot?.nickname || platformUserId}
              </p>
            </div>
          </div>
        )}

        {step === "failed" && (
          <div className="flex items-center justify-end gap-2">
            <Button size="sm" variant="primary" onClick={() => void begin()}>
              {t("kuaishou.wizard.initRetry")}
            </Button>
          </div>
        )}

        {step !== "done" && (
          <div className="flex justify-end gap-2">
            <Button onClick={() => void cancel()}>{t("common.cancel")}</Button>
          </div>
        )}
      </div>
    </Modal>
  );
}
