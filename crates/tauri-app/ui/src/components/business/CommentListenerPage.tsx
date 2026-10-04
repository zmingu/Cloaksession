import { useCallback, useEffect, useRef, useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { Button } from "../atoms/Button";
import { Pill } from "../atoms/Pill";
import {
  commentListener,
  type CommentEvent,
  type ListenerStatus,
  type LiveEventRow,
} from "../../lib/commentListener";

/**
 * 评论监听页 (PRD R2).
 *
 * Event-stream strategy: **定时 `recent` 轮询** (3s, running 时), not
 * `comment_events_next` 长轮询. Rationale: `next()` blocks server-side up
 * to 30s per call, which complicates unmount/cancel and tab-switch races;
 * `recent()` polling returns the full newest-first list in one shot and
 * degrades cleanly when the backend build lacks the commands.
 */
export function CommentListenerPage(): JSX.Element {
  const t = useT();
  const [profileId, setProfileId] = useState("");
  const [targetId, setTargetId] = useState("");
  const [status, setStatus] = useState<ListenerStatus | null>(null);
  const [all, setAll] = useState<ListenerStatus[]>([]);
  const [events, setEvents] = useState<CommentEvent[]>([]);
  const [history, setHistory] = useState<LiveEventRow[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const fail = useCallback((e: unknown) => {
    const detail = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
    if (!alive.current) return;
    setError(detail);
    if (detail.includes("Unhandled fixture IPC command") || detail.includes("not implemented")) {
      setUnavailable(true);
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    const id = profileId.trim();
    if (!id) return;
    try {
      const [one, every] = await Promise.all([
        commentListener.status(id),
        commentListener.statusAll(),
      ]);
      if (!alive.current) return;
      setStatus(one);
      setAll(every);
      setError(null);
    } catch (e) {
      fail(e);
    }
  }, [profileId, fail]);

  const refreshEvents = useCallback(async () => {
    const id = profileId.trim();
    if (!id) return;
    try {
      const list = await commentListener.recent(id, 100);
      if (!alive.current) return;
      setEvents(list);
    } catch (e) {
      fail(e);
    }
  }, [profileId, fail]);

  // Initial status snapshot for the typed profile id (debounced by hand).
  useEffect(() => {
    const id = profileId.trim();
    if (!id) {
      setStatus(null);
      setEvents([]);
      return;
    }
    const timer = window.setTimeout(() => {
      void refreshStatus();
      void refreshEvents();
    }, 400);
    return () => window.clearTimeout(timer);
  }, [profileId, refreshStatus, refreshEvents]);

  // Live stream: poll `recent` while the listener runs.
  useEffect(() => {
    if (!status?.running) return;
    void refreshEvents();
    const timer = window.setInterval(() => {
      void refreshEvents();
    }, 3000);
    return () => window.clearInterval(timer);
  }, [status?.running, profileId, refreshEvents]);

  async function onStart(): Promise<void> {
    const id = profileId.trim();
    if (!id || busy) return;
    setBusy(true);
    setError(null);
    try {
      const next = await commentListener.start(id, targetId.trim() || undefined);
      if (!alive.current) return;
      setStatus(next);
      void refreshEvents();
    } catch (e) {
      fail(e);
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  async function onStop(): Promise<void> {
    const id = profileId.trim();
    if (!id || busy) return;
    setBusy(true);
    setError(null);
    try {
      await commentListener.stop(id);
      await refreshStatus();
    } catch (e) {
      fail(e);
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  async function onHistory(): Promise<void> {
    const id = profileId.trim();
    if (!id || busy) return;
    setBusy(true);
    setError(null);
    try {
      const rows = await commentListener.history(id, 100);
      if (!alive.current) return;
      setHistory(rows);
    } catch (e) {
      fail(e);
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  if (unavailable) {
    return <p role="note">{t("biz.comments.unavailable")}</p>;
  }

  const running = status?.running ?? false;

  return (
    <div className="flex flex-col gap-4 p-6 overflow-y-auto" data-testid="comments-page">
      <div className="flex items-center gap-3">
        <h2 className="text-lg font-semibold">{t("biz.comments.title")}</h2>
        <Pill kind={running ? "running" : "idle"}>
          {running ? t("biz.comments.running") : t("biz.comments.stopped")}
        </Pill>
        {status && (
          <span className="text-xs text-muted-foreground">
            {t("biz.comments.seenCount", { n: String(status.seenCount) })}
          </span>
        )}
      </div>

      {error && (
        <div role="alert" className="text-sm text-red-300">
          {t("biz.comments.failedToast", { detail: error })}
        </div>
      )}

      <div className="flex flex-wrap items-end gap-2">
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.comments.profileId")}
          <input
            aria-label={t("biz.comments.profileId")}
            value={profileId}
            onChange={(e) => setProfileId(e.target.value)}
            placeholder="profile-id"
            className="h-8 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.comments.targetId")}
          <input
            aria-label={t("biz.comments.targetId")}
            value={targetId}
            onChange={(e) => setTargetId(e.target.value)}
            placeholder={t("biz.comments.targetIdHint")}
            className="h-8 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
        <Button variant="primary" disabled={!profileId.trim() || busy || running} onClick={() => void onStart()}>
          {t("biz.comments.start")}
        </Button>
        <Button variant="secondary" disabled={!profileId.trim() || busy || !running} onClick={() => void onStop()}>
          {t("biz.comments.stop")}
        </Button>
        <Button variant="secondary" disabled={!profileId.trim() || busy} onClick={() => void refreshStatus()}>
          {t("biz.comments.refresh")}
        </Button>
      </div>

      {status?.lastError && (
        <p className="text-xs text-red-300">
          {t("biz.comments.lastError", { detail: status.lastError })}
        </p>
      )}

      <section aria-label={t("biz.comments.streamTitle")}>
        <h3 className="text-sm font-medium mb-2">{t("biz.comments.streamTitle")}</h3>
        {events.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.comments.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm" data-testid="comments-stream">
            {events.map((e) => (
              <li key={e.id} className="flex gap-2 items-baseline">
                <span className="mono text-[11px] text-muted-foreground shrink-0">{e.type}</span>
                <span className="truncate">
                  {e.nickname ?? e.user_id ?? e.account_id}: {e.text}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-label={t("biz.comments.historyTitle")}>
        <div className="flex items-center gap-2 mb-2">
          <h3 className="text-sm font-medium">{t("biz.comments.historyTitle")}</h3>
          <Button variant="secondary" size="sm" disabled={!profileId.trim() || busy} onClick={() => void onHistory()}>
            {t("biz.comments.loadHistory")}
          </Button>
        </div>
        {history.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.comments.historyEmpty")}</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm" data-testid="comments-history">
            {history.map((r) => (
              <li key={r.msg_id} className="flex gap-2 items-baseline">
                <span className="mono text-[11px] text-muted-foreground shrink-0">{r.type}</span>
                <span className="truncate">
                  {r.nickname ?? r.user_id ?? r.account_id}: {r.content}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      {all.length > 0 && (
        <section aria-label={t("biz.comments.allTitle")}>
          <h3 className="text-sm font-medium mb-2">{t("biz.comments.allTitle")}</h3>
          <ul className="flex flex-col gap-1 text-sm">
            {all.map((s) => (
              <li key={s.accountId} className="flex gap-2 items-center">
                <Pill kind={s.running ? "running" : "idle"}>{s.running ? t("biz.comments.running") : t("biz.comments.stopped")}</Pill>
                <span className="mono text-xs">{s.accountId}</span>
                <span className="text-xs text-muted-foreground">{s.mode} · {s.targetId}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
