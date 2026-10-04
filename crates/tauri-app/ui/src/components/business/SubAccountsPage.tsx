import { useCallback, useEffect, useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { Button } from "../atoms/Button";
import { Pill } from "../atoms/Pill";
import { Confirm } from "../screens/Confirm";
import type {
  BusinessAccount,
  BusinessAccountKind,
} from "../../lib/businessAccounts";
import {
  subAccounts,
  type SubAccountInteraction,
  type SubAccountLoginResult,
} from "../../lib/subAccounts";

/**
 * 小号页 (PRD R4): 列表/新增/解绑(二次确认)、单登/批量登录、进直播间、
 * 发弹幕(二次确认+结果展示)、互动记录查询.
 */
export function SubAccountsPage(): JSX.Element {
  const t = useT();
  const [accounts, setAccounts] = useState<BusinessAccount[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [displayName, setDisplayName] = useState("");
  const [kind, setKind] = useState<BusinessAccountKind>("kuaishou-sub");
  const [platformUserId, setPlatformUserId] = useState("");
  const [profileId, setProfileId] = useState("");
  const [liveUrl, setLiveUrl] = useState("");
  const [danmaku, setDanmaku] = useState("");
  const [results, setResults] = useState<SubAccountLoginResult[]>([]);
  const [interactions, setInteractions] = useState<SubAccountInteraction[]>([]);
  const [interactAccount, setInteractAccount] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const [confirmUnbind, setConfirmUnbind] = useState<string | null>(null);
  const [confirmDanmaku, setConfirmDanmaku] = useState<string | null>(null);

  const fail = useCallback((e: unknown) => {
    const detail = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
    setError(detail);
    setStatus(null);
    if (detail.includes("Unhandled fixture IPC command") || detail.includes("not implemented")) {
      setUnavailable(true);
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      setAccounts(await subAccounts.list());
      setError(null);
    } catch (e) {
      fail(e);
    }
  }, [fail]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  function toggle(id: string): void {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function onCreate(): Promise<void> {
    const name = displayName.trim();
    if (!name || !profileId.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await subAccounts.save({
        profileId: profileId.trim(),
        kind,
        displayName: name,
        platformUserId: platformUserId.trim() || null,
      });
      setDisplayName("");
      setPlatformUserId("");
      setStatus(t("biz.sub.savedToast"));
      await refresh();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function onUnbind(id: string): Promise<void> {
    setConfirmUnbind(null);
    setBusy(true);
    setError(null);
    try {
      await subAccounts.unbind(id);
      setStatus(t("biz.sub.unboundToast"));
      await refresh();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function onLogin(accountId: string): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const r = await subAccounts.login(accountId);
      setResults([r]);
      setStatus(r.ok ? t("biz.sub.loginOk", { id: r.accountId }) : r.error ?? t("biz.sub.loginFail", { id: r.accountId }));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function onBatchLogin(): Promise<void> {
    const ids = [...selected];
    if (ids.length === 0 || busy) return;
    setBusy(true);
    setError(null);
    try {
      const list = await subAccounts.batchLogin(ids);
      setResults(list);
      const ok = list.filter((r) => r.ok).length;
      setStatus(t("biz.sub.batchDone", { ok: String(ok), n: String(list.length) }));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function onEnterRoom(accountId: string): Promise<void> {
    const url = liveUrl.trim();
    if (!url || busy) return;
    setBusy(true);
    setError(null);
    try {
      await subAccounts.enterLiveRoom(accountId, url);
      setStatus(t("biz.sub.enteredToast", { id: accountId }));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function onSendDanmaku(accountId: string): Promise<void> {
    const content = danmaku.trim();
    setConfirmDanmaku(null);
    if (!content || busy) return;
    setBusy(true);
    setError(null);
    try {
      const row = await subAccounts.sendDanmaku(accountId, content);
      setStatus(row.ok ? t("biz.sub.sentToast", { id: String(row.id) }) : (row.error ?? t("biz.sub.sendFail")));
      if (interactAccount === accountId) {
        setInteractions(await subAccounts.interactions(accountId));
      }
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function onQueryInteractions(accountId: string): Promise<void> {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      setInteractAccount(accountId);
      setInteractions(await subAccounts.interactions(accountId));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  if (unavailable) {
    return <p role="note">{t("biz.sub.unavailable")}</p>;
  }

  return (
    <div className="flex flex-col gap-4 p-6 overflow-y-auto" data-testid="sub-page">
      <h2 className="text-lg font-semibold">{t("biz.sub.title")}</h2>

      {error && (
        <div role="alert" className="text-sm text-red-300">
          {t("biz.sub.failedToast", { detail: error })}
        </div>
      )}
      {status && (
        <div role="status" className="text-sm text-emerald-300">
          {status}
        </div>
      )}

      <section aria-label={t("biz.sub.listTitle")}>
        <h3 className="text-sm font-medium mb-2">{t("biz.sub.listTitle")}</h3>
        {accounts.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.sub.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm" data-testid="sub-list">
            {accounts.map((a) => (
              <li key={a.id} className="flex flex-wrap gap-2 items-center">
                <input
                  type="checkbox"
                  aria-label={t("biz.sub.select", { name: a.displayName })}
                  checked={selected.has(a.id)}
                  onChange={() => toggle(a.id)}
                />
                <Pill kind="info">{a.kind}</Pill>
                <span>{a.displayName}</span>
                <span className="mono text-xs text-muted-foreground">{a.id}</span>
                <Button variant="secondary" size="sm" disabled={busy} onClick={() => void onLogin(a.id)}>
                  {t("biz.sub.login")}
                </Button>
                <Button variant="secondary" size="sm" disabled={busy || !liveUrl.trim()} onClick={() => void onEnterRoom(a.id)}>
                  {t("biz.sub.enterRoom")}
                </Button>
                <Button variant="secondary" size="sm" disabled={busy} onClick={() => void onQueryInteractions(a.id)}>
                  {t("biz.sub.interactions")}
                </Button>
                <Button variant="secondary" size="sm" disabled={busy || !danmaku.trim()} onClick={() => setConfirmDanmaku(a.id)}>
                  {t("biz.sub.send")}
                </Button>
                <Button variant="secondary" size="sm" disabled={busy} onClick={() => setConfirmUnbind(a.id)}>
                  {t("biz.sub.unbind")}
                </Button>
              </li>
            ))}
          </ul>
        )}
        <div className="mt-2">
          <Button variant="secondary" size="sm" disabled={selected.size === 0 || busy} onClick={() => void onBatchLogin()}>
            {t("biz.sub.batchLogin", { n: String(selected.size) })}
          </Button>
        </div>
      </section>

      <section aria-label={t("biz.sub.createTitle")} className="flex flex-wrap items-end gap-2">
        <h3 className="text-sm font-medium w-full">{t("biz.sub.createTitle")}</h3>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.sub.displayName")}
          <input
            aria-label={t("biz.sub.displayName")}
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            className="h-8 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.sub.kind")}
          <select
            aria-label={t("biz.sub.kind")}
            value={kind}
            onChange={(e) => setKind(e.target.value as BusinessAccountKind)}
            className="h-8 px-2 rounded-md bg-transparent border border-[var(--border)]"
          >
            <option value="kuaishou-sub">kuaishou-sub</option>
            <option value="kuaishou-live">kuaishou-live</option>
            <option value="kuaishou-mate">kuaishou-mate</option>
            <option value="kuaishou-shop">kuaishou-shop</option>
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.sub.profileId")}
          <input
            aria-label={t("biz.sub.profileId")}
            value={profileId}
            onChange={(e) => setProfileId(e.target.value)}
            placeholder="profile-id"
            className="h-8 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.sub.platformUserId")}
          <input
            aria-label={t("biz.sub.platformUserId")}
            value={platformUserId}
            onChange={(e) => setPlatformUserId(e.target.value)}
            className="h-8 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
        <Button variant="primary" disabled={!displayName.trim() || !profileId.trim() || busy} onClick={() => void onCreate()}>
          {t("biz.sub.create")}
        </Button>
      </section>

      <section aria-label={t("biz.sub.liveTitle")} className="flex flex-wrap items-end gap-2">
        <h3 className="text-sm font-medium w-full">{t("biz.sub.liveTitle")}</h3>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.sub.liveUrl")}
          <input
            aria-label={t("biz.sub.liveUrl")}
            value={liveUrl}
            onChange={(e) => setLiveUrl(e.target.value)}
            placeholder="https://live.kuaishou.com/…"
            className="h-8 w-72 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          {t("biz.sub.danmaku")}
          <input
            aria-label={t("biz.sub.danmaku")}
            value={danmaku}
            onChange={(e) => setDanmaku(e.target.value)}
            className="h-8 w-72 px-2 rounded-md bg-transparent border border-[var(--border)]"
          />
        </label>
      </section>

      {results.length > 0 && (
        <section aria-label={t("biz.sub.resultsTitle")}>
          <h3 className="text-sm font-medium mb-2">{t("biz.sub.resultsTitle")}</h3>
          <ul className="flex flex-col gap-1 text-sm" data-testid="sub-login-results">
            {results.map((r) => (
              <li key={r.accountId} className="flex gap-2 items-center">
                <Pill kind={r.ok ? "running" : "error"}>{r.ok ? t("biz.sub.ok") : t("biz.sub.fail")}</Pill>
                <span className="mono text-xs">{r.accountId}</span>
                {r.error && <span className="text-xs text-red-300">{r.error}</span>}
              </li>
            ))}
          </ul>
        </section>
      )}

      {interactAccount && (
        <section aria-label={t("biz.sub.interactionsTitle")}>
          <h3 className="text-sm font-medium mb-2">
            {t("biz.sub.interactionsTitle")} · <span className="mono text-xs">{interactAccount}</span>
          </h3>
          {interactions.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("biz.sub.interactionsEmpty")}</p>
          ) : (
            <ul className="flex flex-col gap-1 text-sm" data-testid="sub-interactions">
              {interactions.map((row) => (
                <li key={row.id} className="flex gap-2 items-center">
                  <Pill kind={row.ok ? "running" : "error"}>{row.action}</Pill>
                  <span className="truncate">{row.message ?? row.liveRoomUrl ?? `#${row.id}`}</span>
                  {row.error && <span className="text-xs text-red-300">{row.error}</span>}
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      <Confirm
        open={confirmUnbind !== null}
        title={t("biz.sub.unbindConfirmTitle")}
        description={t("biz.sub.unbindConfirmBody")}
        confirmLabel={t("biz.sub.unbindConfirm")}
        destructive
        onConfirm={() => {
          if (confirmUnbind) void onUnbind(confirmUnbind);
        }}
        onCancel={() => setConfirmUnbind(null)}
      />
      <Confirm
        open={confirmDanmaku !== null}
        title={t("biz.sub.danmakuConfirmTitle")}
        description={t("biz.sub.danmakuConfirmBody", { content: danmaku.trim() })}
        confirmLabel={t("biz.sub.send")}
        onConfirm={() => {
          if (confirmDanmaku) void onSendDanmaku(confirmDanmaku);
        }}
        onCancel={() => setConfirmDanmaku(null)}
      />
    </div>
  );
}
