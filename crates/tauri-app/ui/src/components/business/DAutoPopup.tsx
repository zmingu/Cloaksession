import { useEffect, useState, type JSX } from "react";
import {
  autoPopup,
  onAutoPopupEvent,
  onAutoPopupState,
} from "../../lib/autoPopup";
import type {
  AutoPopUpEvent,
  AutoPopUpStatus,
  PopupGoodsInfo,
  PopupScanReport,
  ShortcutRegisterResult,
} from "../../types";
import { Button } from "../atoms/Button";
import { useT } from "../../i18n/LanguageProvider";

const CONTROL =
  "w-full min-w-0 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-2 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

function errDetail(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

interface Props {
  profileId: string;
}

/**
 * 自动弹窗页面（D 组）。
 *
 * 覆盖 10 个 `auto_popup_*` 命令 + 订阅 `auto-popup:state` / `auto-popup:event`。
 * 停止与单次讲解是写操作：走内联二次确认。
 */
export function DAutoPopup({ profileId }: Props): JSX.Element {
  const t = useT();
  const [status, setStatus] = useState<AutoPopUpStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [confirmStop, setConfirmStop] = useState(false);
  const [confirmExplainId, setConfirmExplainId] = useState<string | null>(null);
  const [events, setEvents] = useState<AutoPopUpEvent[]>([]);

  // Start-config form.
  const [goodsIdsText, setGoodsIdsText] = useState("001");
  const [minSec, setMinSec] = useState("30");
  const [maxSec, setMaxSec] = useState("60");
  const [random, setRandom] = useState(false);
  const [maxRetries, setMaxRetries] = useState("3");
  const [retryDelay, setRetryDelay] = useState("1000");

  // Goods / scan / explain / shortcuts.
  const [goods, setGoods] = useState<PopupGoodsInfo[]>([]);
  const [report, setReport] = useState<PopupScanReport | null>(null);
  const [explainId, setExplainId] = useState("");
  const [bindingsText, setBindingsText] = useState("CommandOrControl+1=001");
  const [accelerator, setAccelerator] = useState("");
  const [shortcutResult, setShortcutResult] = useState<ShortcutRegisterResult | null>(null);

  useEffect(() => {
    let offState = (): void => {};
    let offEvent = (): void => {};
    let active = true;
    void onAutoPopupState((s) => {
      if (active && s.profileId === profileId) setStatus(s);
    }).then((fn) => {
      if (active) offState = fn;
    });
    void onAutoPopupEvent((e) => {
      if (active && e.profileId === profileId) {
        setEvents((prev) => [...prev.slice(-19), e]);
      }
    }).then((fn) => {
      if (active) offEvent = fn;
    });
    return () => {
      active = false;
      offState();
      offEvent();
    };
  }, [profileId]);

  async function refresh(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setStatus(await autoPopup.status(profileId));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  function buildConfig(): { ok: boolean; config?: Parameters<typeof autoPopup.start>[1] } {
    const min = Number(minSec);
    const max = Number(maxSec);
    if (!Number.isFinite(min) || !Number.isFinite(max) || min < 1 || max < min) {
      setError(t("biz.popup.loadFailed", { detail: `[${minSec}, ${maxSec}]` }));
      return { ok: false };
    }
    const goodsIds = goodsIdsText.split(/[\r\n,，]+/).map((s) => s.trim()).filter(Boolean);
    return {
      ok: true,
      config: {
        goodsIds: goodsIds.length > 0 ? goodsIds : null,
        interval: [min, max],
        random,
        retry: {
          maxRetries: maxRetries.trim() ? Number(maxRetries) : null,
          retryDelayMs: retryDelay.trim() ? Number(retryDelay) : null,
        },
      },
    };
  }

  async function start(): Promise<void> {
    const built = buildConfig();
    if (!built.ok || !built.config) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      setStatus(await autoPopup.start(profileId, built.config));
      setMessage(t("biz.popup.startedToast"));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function stop(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setStatus(await autoPopup.stop(profileId, "manual"));
      setConfirmStop(false);
      setMessage(t("biz.popup.stoppedToast"));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function loadGoods(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setGoods(await autoPopup.goods(profileId));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function scan(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setReport(await autoPopup.scan(profileId));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function explain(goodsId: string): Promise<void> {
    if (!goodsId.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await autoPopup.explainOnce(profileId, goodsId.trim());
      setConfirmExplainId(null);
      setMessage(t("biz.popup.explainedToast", { id: goodsId.trim() }));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  function parseBindings(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const line of bindingsText.split(/[\r\n]+/)) {
      const i = line.indexOf("=");
      if (i <= 0) continue;
      const k = line.slice(0, i).trim();
      const v = line.slice(i + 1).trim();
      if (k && v) out[k] = v;
    }
    return out;
  }

  async function register(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setShortcutResult(await autoPopup.registerShortcuts(profileId, parseBindings()));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function unregister(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      await autoPopup.unregisterShortcuts(profileId);
      setShortcutResult(null);
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function trigger(): Promise<void> {
    if (!accelerator.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const goodsId = await autoPopup.triggerShortcut(profileId, accelerator.trim());
      setMessage(t("biz.popup.explainedToast", { id: goodsId }));
    } catch (cause) {
      setError(t("biz.popup.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label={t("biz.popup.title")} aria-busy={busy} className="min-w-0 space-y-3 text-[12px]">
      <div className="space-y-1.5">
        <h3 className="font-medium text-slate-200">{t("biz.popup.title")}</h3>
        <p className="text-slate-400 leading-relaxed">{t("biz.popup.subtitle")}</p>
      </div>

      {error && <p role="alert" className="break-words text-red-300">{error}</p>}
      {message && <p role="status" className="text-emerald-300">{message}</p>}

      <div className="rounded-lg border border-white/10 p-3 space-y-1.5" data-testid="popup-status">
        <p className="font-medium text-slate-200">{t("biz.popup.status")}</p>
        <p className="text-slate-400">
          {status ? (status.running ? t("biz.popup.running") : t("biz.popup.stopped")) : "—"}
          {status && ` · ${t("biz.popup.queueLen")}: ${status.queueLen}`}
          {status?.lastGoodsId && ` · ${t("biz.popup.lastGoods")}: ${status.lastGoodsId}`}
        </p>
        {status?.lastError && (
          <p className="text-amber-300 break-words">{t("biz.popup.lastError")}: {status.lastError}</p>
        )}
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void refresh()} disabled={busy}>{t("biz.popup.refresh")}</Button>
        </div>
      </div>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          void start();
        }}
        className="rounded-lg border border-white/10 p-3 space-y-2.5"
      >
        <label className="block space-y-1.5 text-slate-400">
          <span>{t("biz.popup.goodsIdsLabel")}</span>
          <textarea
            className={CONTROL}
            rows={3}
            value={goodsIdsText}
            onChange={(e) => setGoodsIdsText(e.target.value)}
            placeholder={t("biz.popup.goodsIdsPlaceholder")}
          />
          <span className="block text-[11px]">{t("biz.popup.goodsIdsHint")}</span>
        </label>
        <div className="grid grid-cols-2 gap-2.5">
          <label className="block space-y-1.5 text-slate-400">
            <span>min (sec)</span>
            <input className={CONTROL} value={minSec} onChange={(e) => setMinSec(e.target.value)} />
          </label>
          <label className="block space-y-1.5 text-slate-400">
            <span>max (sec)</span>
            <input className={CONTROL} value={maxSec} onChange={(e) => setMaxSec(e.target.value)} />
          </label>
        </div>
        <label className="flex items-center gap-2 text-slate-400 cursor-pointer">
          <input
            type="checkbox"
            checked={random}
            onChange={(e) => setRandom(e.target.checked)}
            className="w-3.5 h-3.5 rounded accent-[var(--accent)]"
          />
          {t("biz.popup.randomLabel")}
        </label>
        <div className="grid grid-cols-2 gap-2.5">
          <label className="block space-y-1.5 text-slate-400">
            <span>{t("biz.popup.maxRetriesLabel")}</span>
            <input className={CONTROL} value={maxRetries} onChange={(e) => setMaxRetries(e.target.value)} />
          </label>
          <label className="block space-y-1.5 text-slate-400">
            <span>{t("biz.popup.retryDelayLabel")}</span>
            <input className={CONTROL} value={retryDelay} onChange={(e) => setRetryDelay(e.target.value)} />
          </label>
        </div>
        <div className="flex flex-wrap gap-2 items-center">
          <Button type="submit" variant="primary" disabled={busy}>{t("biz.popup.start")}</Button>
          {confirmStop ? (
            <div role="group" aria-label={t("biz.popup.confirmStopTitle")} className="flex flex-wrap gap-2 items-center">
              <span className="text-amber-200">{t("biz.popup.confirmStopBody")}</span>
              <Button size="sm" variant="danger" onClick={() => void stop()} disabled={busy}>
                {t("common.confirm")}
              </Button>
              <Button size="sm" onClick={() => setConfirmStop(false)} disabled={busy}>
                {t("common.cancel")}
              </Button>
            </div>
          ) : (
            <Button variant="danger" onClick={() => setConfirmStop(true)} disabled={busy}>
              {t("biz.popup.stop")}
            </Button>
          )}
        </div>
      </form>

      <div className="rounded-lg border border-white/10 p-3 space-y-2" data-testid="popup-goods">
        <p className="font-medium text-slate-200">{t("biz.popup.goodsList")}</p>
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void loadGoods()} disabled={busy}>{t("biz.popup.refresh")}</Button>
          <Button size="sm" onClick={() => void scan()} disabled={busy}>{t("biz.popup.scan")}</Button>
        </div>
        {goods.map((g) => (
          <p key={g.serial} className="text-slate-400 break-words">
            {g.serial}{g.title ? ` · ${g.title}` : ""}{g.price ? ` · ${g.price}` : ""}
          </p>
        ))}
        {report && (
          <p className="text-slate-500 break-words">
            {report.diffs.length === 0
              ? t("biz.popup.scanEmpty", { n: report.scannedCount })
              : report.diffs.map((d) => `${d.goodsId}/${d.field}`).join(", ")}
          </p>
        )}
      </div>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          setConfirmExplainId(explainId.trim() || null);
        }}
        className="rounded-lg border border-white/10 p-3 space-y-2.5"
      >
        <label className="block space-y-1.5 text-slate-400">
          <span>{t("biz.popup.goodsIdLabel")}</span>
          <input className={CONTROL} value={explainId} onChange={(e) => setExplainId(e.target.value)} />
        </label>
        {confirmExplainId ? (
          <div role="group" aria-label={t("biz.popup.confirmExplainTitle")} className="flex flex-wrap gap-2 items-center">
            <span className="text-amber-200">
              {t("biz.popup.confirmExplainBody", { id: confirmExplainId })}
            </span>
            <Button size="sm" variant="primary" onClick={() => void explain(confirmExplainId)} disabled={busy}>
              {t("common.confirm")}
            </Button>
            <Button size="sm" onClick={() => setConfirmExplainId(null)} disabled={busy}>
              {t("common.cancel")}
            </Button>
          </div>
        ) : (
          <Button type="submit" disabled={!explainId.trim() || busy}>{t("biz.popup.explainOnce")}</Button>
        )}
      </form>

      <div className="rounded-lg border border-white/10 p-3 space-y-2.5">
        <p className="font-medium text-slate-200">{t("biz.popup.shortcutsTitle")}</p>
        <p className="text-[11px] text-slate-500">{t("biz.popup.bindingsHint")}</p>
        <textarea className={CONTROL} rows={2} value={bindingsText} onChange={(e) => setBindingsText(e.target.value)} />
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void register()} disabled={busy}>{t("biz.popup.register")}</Button>
          <Button size="sm" onClick={() => void unregister()} disabled={busy}>{t("biz.popup.unregister")}</Button>
        </div>
        {shortcutResult && (
          <p className="text-slate-400 break-words">
            ok: {String(shortcutResult.ok)} · {shortcutResult.registered.join(", ")}
            {shortcutResult.failed.map((f) => ` · ${f.accelerator}: ${f.error}`).join("")}
          </p>
        )}
        <div className="flex flex-wrap gap-2 items-end">
          <label className="block space-y-1.5 text-slate-400 flex-1 min-w-[160px]">
            <span>{t("biz.popup.acceleratorLabel")}</span>
            <input className={CONTROL} value={accelerator} onChange={(e) => setAccelerator(e.target.value)} />
          </label>
          <Button size="sm" onClick={() => void trigger()} disabled={!accelerator.trim() || busy}>
            {t("biz.popup.trigger")}
          </Button>
        </div>
      </div>

      {events.length > 0 && (
        <div className="rounded-lg border border-white/10 p-3 space-y-1.5" data-testid="popup-events">
          {events.map((e, i) => (
            <p key={`${e.kind}-${i}`} className="text-slate-500 break-words">
              {e.kind}{e.goodsId ? ` · ${e.goodsId}` : ""}{e.reason ? ` · ${e.reason}` : ""}
            </p>
          ))}
        </div>
      )}
    </section>
  );
}
