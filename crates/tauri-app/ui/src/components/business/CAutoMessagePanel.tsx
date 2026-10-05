import { useEffect, useState, type JSX } from "react";

import { Button } from "../atoms/Button";
import { useT } from "../../i18n/LanguageProvider";
import { autoMessage, onAutoMessageProgress, onAutoMessageStopped } from "../../lib/autoMessage";
import type { AutoMessageLine, AutoMessageState } from "../../types";

interface Props {
  /** 账号键（浏览器环境 = 快手小店账号）。由「直播互动」工作台传入。 */
  accountId: string;
}

/**
 * C-group: 主播互动（时间轴弹幕）.
 *
 * Draft timeline lines locally, then `start` (two-step confirmed) fires
 * them at `started_at + offset_sec` against each line's `account_id`
 * session. `stop` is confirmed too. Progress arrives on
 * `auto-message:progress`; termination on `auto-message:stopped`.
 * Random-space injection is not offered anywhere in this panel.
 */
export function CAutoMessagePanel({ accountId }: Props): JSX.Element {
  const t = useT();
  const [message, setMessage] = useState("");
  const [offsetSec, setOffsetSec] = useState("5");
  const [nickname, setNickname] = useState("");
  const [anchor, setAnchor] = useState("");
  const [lines, setLines] = useState<AutoMessageLine[]>([]);
  const [runId, setRunId] = useState<string | null>(null);
  const [state, setState] = useState<AutoMessageState | null>(null);
  const [confirming, setConfirming] = useState<"start" | "stop" | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let unlistenProgress = (): void => {};
    let unlistenStopped = (): void => {};
    void onAutoMessageProgress((snapshot) => {
      if (active) setState(snapshot);
    }).then((fn) => {
      if (active) unlistenProgress = fn;
    });
    void onAutoMessageStopped((stopped) => {
      if (!active) return;
      setRunId((current) => (current === stopped.run_id ? null : current));
      setNotice(t("biz.msg.stoppedState", { reason: stopped.reason }));
    }).then((fn) => {
      if (active) unlistenStopped = fn;
    });
    return () => {
      active = false;
      unlistenProgress();
      unlistenStopped();
    };
  }, [t]);

  const offsetValue = Number(offsetSec);
  const hasAccount = accountId.trim() !== "";
  const canAdd =
    hasAccount && message.trim() !== "" && Number.isFinite(offsetValue) && offsetValue >= 0;

  function addLine(): void {
    if (!canAdd) return;
    setLines((prev) => [
      ...prev,
      { offset_sec: offsetValue, message: message.trim(), account_id: accountId.trim() },
    ]);
    setMessage("");
  }

  async function doStart(): Promise<void> {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const started = await autoMessage.start(lines, {
        nickname: nickname.trim() === "" ? null : nickname.trim(),
        anchor: anchor.trim() === "" ? null : anchor.trim(),
      });
      setRunId(started.run_id);
      setNotice(t("biz.msg.startedToast", { n: String(started.scheduled_count) }));
    } catch (e) {
      setError(t("biz.msg.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  }

  async function doStop(): Promise<void> {
    if (!runId) return;
    setBusy(true);
    setError(null);
    try {
      await autoMessage.stop(runId);
      setRunId(null);
      setNotice(t("biz.msg.stoppedToast"));
    } catch (e) {
      setError(t("biz.msg.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  }

  async function refreshStatus(): Promise<void> {
    if (!runId) return;
    setBusy(true);
    setError(null);
    try {
      setState(await autoMessage.status(runId));
    } catch (e) {
      setError(t("biz.msg.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label={t("biz.msg.title")} className="space-y-4">
      <p className="text-[12px] text-slate-400">{t("biz.msg.desc")}</p>

      <p className="text-[12px] text-slate-400">{t("live.accounts.current", { id: accountId })}</p>
      {!hasAccount && (
        <p role="note" className="text-[12px] text-amber-300">
          {t("live.accounts.selectFirst")}
        </p>
      )}

      <div className="grid grid-cols-2 gap-2">
        <label className="flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.msg.form.offset")}
          <input
            aria-label={t("biz.msg.form.offset")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={offsetSec}
            inputMode="decimal"
            onChange={(e) => setOffsetSec(e.target.value)}
          />
        </label>
        <label className="col-span-2 flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.msg.form.message")}
          <input
            aria-label={t("biz.msg.form.message")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.msg.form.nickname")}
          <input
            aria-label={t("biz.msg.form.nickname")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={nickname}
            onChange={(e) => setNickname(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-300">
          {t("biz.msg.form.anchor")}
          <input
            aria-label={t("biz.msg.form.anchor")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-slate-100"
            value={anchor}
            onChange={(e) => setAnchor(e.target.value)}
          />
        </label>
      </div>
      <Button variant="secondary" disabled={!canAdd} onClick={addLine}>
        {t("biz.msg.line.add")}
      </Button>

      <div aria-label={t("biz.msg.lines.title")}>
        {lines.length === 0 ? (
          <p className="text-[12px] text-slate-500">{t("biz.msg.lines.empty")}</p>
        ) : (
          <ul className="space-y-1">
            {lines.map((line, idx) => (
              <li key={`${line.account_id}-${idx}`} className="flex flex-wrap items-center gap-2 text-[12px] text-slate-300">
                <span className="min-w-0 flex-1 break-all">
                  [{line.offset_sec}s] {line.account_id}: {line.message}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={t("biz.msg.line.removeAria", { n: String(idx + 1) })}
                  onClick={() => setLines((prev) => prev.filter((_, i) => i !== idx))}
                >
                  {t("biz.msg.line.remove")}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="flex items-center gap-2">
        {!confirming && (
          <>
            <Button variant="primary" disabled={busy || !hasAccount || lines.length === 0 || runId !== null} onClick={() => setConfirming("start")}>
              {t("biz.msg.start")}
            </Button>
            <Button variant="danger" disabled={busy || runId === null} onClick={() => setConfirming("stop")}>
              {t("biz.msg.stop")}
            </Button>
            <Button variant="ghost" disabled={busy || runId === null} onClick={() => void refreshStatus()}>
              {t("biz.msg.status.refresh")}
            </Button>
          </>
        )}
      </div>

      {confirming === "start" && (
        <div role="group" aria-label={t("biz.msg.confirm.startTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
          <p className="text-[12px] text-slate-300">{t("biz.msg.confirm.startBody", { n: String(lines.length) })}</p>
          <div className="flex gap-2">
            <Button variant="primary" size="sm" disabled={busy} onClick={() => void doStart()}>
              {t("biz.msg.confirm.confirm")}
            </Button>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirming(null)}>
              {t("biz.msg.confirm.cancel")}
            </Button>
          </div>
        </div>
      )}
      {confirming === "stop" && (
        <div role="group" aria-label={t("biz.msg.confirm.stopTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
          <p className="text-[12px] text-slate-300">{t("biz.msg.confirm.stopBody")}</p>
          <div className="flex gap-2">
            <Button variant="danger" size="sm" disabled={busy} onClick={() => void doStop()}>
              {t("biz.msg.confirm.confirm")}
            </Button>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirming(null)}>
              {t("biz.msg.confirm.cancel")}
            </Button>
          </div>
        </div>
      )}

      {runId && <p className="text-[12px] text-slate-400">{t("biz.msg.run.label", { id: runId })}</p>}
      {state && (
        <p className="text-[12px] text-slate-400">
          {t("biz.msg.progress.label", { sent: String(state.sent_count), total: String(state.total_count) })}
        </p>
      )}
      {notice && <p role="status" className="text-[12px] text-emerald-300">{notice}</p>}
      {error && <p role="alert" className="text-[12px] text-red-300">{error}</p>}
    </section>
  );
}
