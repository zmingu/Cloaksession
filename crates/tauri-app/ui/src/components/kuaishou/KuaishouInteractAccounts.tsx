import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from "react";
import {
  DoorOpen,
  History,
  Loader2,
  LogIn,
  Play,
  Plus,
  RefreshCw,
  Search,
  Send,
  Square,
  Trash2,
  Unlink,
} from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import type { TranslationKey } from "../../i18n/en";
import { cn } from "../../lib/cn";
import type { BusinessAccount } from "../../lib/businessAccounts";
import { onRunningChanged, profiles as profilesApi } from "../../lib/ipc";
import { useKuaishouIdentities, useKuaishouIdentity } from "../../lib/KuaishouIdentityProvider";
import {
  subAccounts,
  type SubAccountInteraction,
  type SubAccountLoginResult,
} from "../../lib/subAccounts";
import type { ProfileSummary } from "../../types";
import { Avatar, Pill, confirm } from "../atoms";
import { Button } from "../atoms/Button";
import { IdentityAvatar } from "../profile/KuaishouIdentity";
import { DataTable, type DataTableColumn } from "../table/DataTable";
import { KuaishouInteractWizard } from "./KuaishouInteractWizard";

const CONTROL =
  "h-8 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

type RowOp = "launch" | "stop" | "relogin" | "login" | "enter" | "send" | "unbind" | "delete" | "history";

/** One list row: the interact record joined with its browser environment. */
interface InteractRow {
  record: BusinessAccount;
  /** The bound environment, or null when it was deleted / never bound. */
  profile: ProfileSummary | null;
}

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 互动账号管理（快手 › 互动账号）。
 *
 * 环境化列表：**账号即环境**。数据真相是两处 join——
 *  - 登记记录：`business_accounts(kind="kuaishou-sub")`（`subAccounts.list()`）；
 *  - 运行态：`profiles_list`（`isRunning`）；身份：后端身份检测 provider。
 *
 * 删除 = 删环境 + 删登记记录：`profiles_close` → `profiles_delete` →
 * `subAccounts.delete`（记录被彻底移除，行从列表消失；环境缺失行同样可删）。
 * 批量删除 = 对选中行**逐行**执行同一流程（关环境 → 删环境 → 删记录），
 * 单行失败只记录错误、不中断整批；环境缺失行只删登记记录。
 * 解绑（`subAccounts.unbind`）只把 `profile_id` 置空并保留记录，因此对
 * `profile_id` 已为空的行没有意义，按钮置灰。
 * 登录/扫码由 `KuaishouInteractWizard` 承担；行内「重新登录」走
 * `profiles_launch(entry="kuaishou-sub")` 打开可见登录页，不发任何平台写请求。
 *
 * 危险操作（发弹幕、解绑、删除）保留二次确认。
 */
export function KuaishouInteractAccounts(): JSX.Element {
  const t = useT();
  const { entries } = useKuaishouIdentities();

  const [records, setRecords] = useState<BusinessAccount[]>([]);
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [rowsError, setRowsError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [liveUrl, setLiveUrl] = useState("");
  const [danmaku, setDanmaku] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [results, setResults] = useState<SubAccountLoginResult[]>([]);
  const [interactions, setInteractions] = useState<SubAccountInteraction[]>([]);
  const [interactAccount, setInteractAccount] = useState("");
  const [roomMap, setRoomMap] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<Record<string, RowOp>>({});
  const [batchBusy, setBatchBusy] = useState(false);
  const [batchDeleteBusy, setBatchDeleteBusy] = useState(false);
  const [firstLoad, setFirstLoad] = useState(true);
  const [wizardOpen, setWizardOpen] = useState(false);
  const mounted = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refreshProfiles = useCallback(async (): Promise<void> => {
    try {
      const list = await profilesApi.list();
      if (mounted.current) setProfiles(list);
    } catch {
      // The record list is the primary source; a failed profile read degrades
      // the running state to "unknown" rather than blanking the table.
    }
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    try {
      const list = await subAccounts.list();
      if (mounted.current) {
        setRecords(list);
        setRowsError(null);
      }
    } catch (cause) {
      if (mounted.current) setRowsError(errorText(cause));
    }
  }, []);

  useEffect(() => {
    void (async () => {
      await Promise.all([refresh(), refreshProfiles()]);
      // Both readers swallow their own errors, so this always resolves: the
      // skeleton ends even when the backend is unreachable.
      if (mounted.current) setFirstLoad(false);
    })();
  }, [refresh, refreshProfiles]);

  // Any running-state change (including a window the user closed directly, or
  // the wizard's own launch) refreshes this page's copy of the profile list.
  useEffect(() => {
    let off = (): void => {};
    let active = true;
    void onRunningChanged(() => {
      void refreshProfiles();
    }).then((fn) => {
      if (active) off = fn;
    });
    return () => {
      active = false;
      off();
    };
  }, [refreshProfiles]);

  const profileMap = useMemo(() => {
    const map = new Map<string, ProfileSummary>();
    for (const profile of profiles) map.set(profile.id, profile);
    return map;
  }, [profiles]);

  // Join records with environments. A record whose environment is gone stays
  // visible (as "环境缺失") so the user can still delete it.
  const rows = useMemo<InteractRow[]>(
    () =>
      records.map((record) => ({
        record,
        profile: record.profileId ? (profileMap.get(record.profileId) ?? null) : null,
      })),
    [records, profileMap],
  );

  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return rows;
    return rows.filter((row) => {
      const profileId = row.record.profileId ?? "";
      const detected = profileId ? (entries.get(profileId)?.snapshot?.platformUserId ?? "") : "";
      const nickname = profileId ? (entries.get(profileId)?.snapshot?.nickname ?? "") : "";
      return [
        row.record.displayName,
        row.record.platformUserId ?? "",
        detected,
        nickname,
        row.record.id,
        profileId,
        row.profile?.name ?? "",
      ].some((value) => value.toLocaleLowerCase().includes(needle));
    });
  }, [rows, entries, query]);

  const allSelected = filtered.length > 0 && filtered.every((row) => selected.has(row.record.id));

  function toggle(id: string): void {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleAll(): void {
    setSelected((prev) => {
      if (filtered.every((row) => prev.has(row.record.id))) return new Set();
      return new Set(filtered.map((row) => row.record.id));
    });
  }

  function setRowBusy(id: string, op: RowOp): void {
    setBusy((prev) => ({ ...prev, [id]: op }));
  }

  function clearRowBusy(id: string): void {
    if (!mounted.current) return;
    setBusy((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }

  async function onLaunch(row: InteractRow, kind: "launch" | "stop"): Promise<void> {
    const profileId = row.record.profileId;
    if (!profileId) return;
    setRowBusy(row.record.id, kind);
    setActionError(null);
    try {
      if (kind === "launch") await profilesApi.launch(profileId);
      else await profilesApi.close(profileId);
      await refreshProfiles();
    } catch (cause) {
      if (mounted.current) {
        setActionError(
          kind === "launch"
            ? t("kuaishou.interact.launchFailed", { error: errorText(cause) })
            : t("kuaishou.interact.stopFailed", { error: errorText(cause) }),
        );
      }
    } finally {
      clearRowBusy(row.record.id);
    }
  }

  /** Re-open the Kuaishou main-site login page in a visible window. */
  async function askRelogin(row: InteractRow): Promise<void> {
    const profileId = row.record.profileId;
    if (!profileId) return;
    const ok = await confirm({
      title: t("kuaishou.interact.reloginTitle"),
      body: t("kuaishou.interact.reloginBody"),
      confirmLabel: t("kuaishou.interact.reloginConfirm"),
    });
    if (!ok) return;
    setRowBusy(row.record.id, "relogin");
    setActionError(null);
    try {
      // Visible window: a main-site QR is not reliably captured off-screen.
      await profilesApi.launchKuaishouSub(profileId);
      if (mounted.current) {
        setStatus(t("kuaishou.interact.reloginOpened", { name: row.record.displayName }));
      }
      await refreshProfiles();
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      clearRowBusy(row.record.id);
    }
  }

  async function onLogin(accountId: string): Promise<void> {
    setRowBusy(accountId, "login");
    setActionError(null);
    try {
      const r = await subAccounts.login(accountId);
      if (!mounted.current) return;
      setResults([r]);
      setStatus(r.ok ? t("kuaishou.interact.loginOk", { id: r.accountId }) : (r.error ?? t("kuaishou.interact.loginFail", { id: r.accountId })));
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      clearRowBusy(accountId);
    }
  }

  async function onBatchLogin(): Promise<void> {
    const ids = [...selected];
    if (ids.length === 0 || batchBusy) return;
    setBatchBusy(true);
    setActionError(null);
    try {
      const list = await subAccounts.batchLogin(ids);
      if (!mounted.current) return;
      setResults(list);
      const ok = list.filter((r) => r.ok).length;
      setStatus(t("kuaishou.interact.batchDone", { ok: String(ok), n: String(list.length) }));
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      if (mounted.current) setBatchBusy(false);
    }
  }

  /**
   * Batch delete = run the single-row delete flow (close → delete environment →
   * delete record) over every selected row, one row at a time. A failure on one
   * row is collected and reported but never aborts the rest of the batch.
   */
  async function onBatchDelete(): Promise<void> {
    const ids = [...selected];
    if (ids.length === 0 || batchDeleteBusy) return;
    const ok = await confirm({
      title: t("kuaishou.interact.batchDeleteTitle", { n: String(ids.length) }),
      body: t("kuaishou.interact.batchDeleteBody", { n: String(ids.length) }),
      confirmLabel: t("kuaishou.interact.batchDeleteConfirm"),
      destructive: true,
    });
    if (!ok) return;
    setBatchDeleteBusy(true);
    setActionError(null);
    try {
      // Read the full row list (not the filtered view) so a selected row hidden by
      // the search filter still resolves its bound environment.
      const profileByRecord = new Map<string, string | null>();
      for (const row of rows) profileByRecord.set(row.record.id, row.record.profileId);
      const failures: string[] = [];
      let okCount = 0;
      for (const id of ids) {
        try {
          const profileId = profileByRecord.get(id) ?? null;
          if (profileId) {
            await profilesApi.close(profileId).catch(() => {});
            await profilesApi.delete(profileId);
          }
          await subAccounts.delete(id);
          okCount += 1;
        } catch (cause) {
          failures.push(`${id}: ${errorText(cause)}`);
        }
      }
      if (!mounted.current) return;
      setSelected((prev) => {
        const next = new Set(prev);
        for (const id of ids) next.delete(id);
        return next;
      });
      setStatus(t("kuaishou.interact.batchDeleteDone", { ok: String(okCount), n: String(ids.length) }));
      if (failures.length > 0) {
        setActionError(t("kuaishou.interact.batchDeleteFailed", { error: failures.join("; ") }));
      }
      await Promise.all([refresh(), refreshProfiles()]);
    } finally {
      if (mounted.current) setBatchDeleteBusy(false);
    }
  }

  async function onEnterRoom(accountId: string): Promise<void> {
    const url = liveUrl.trim();
    if (!url) return;
    setRowBusy(accountId, "enter");
    setActionError(null);
    try {
      await subAccounts.enterLiveRoom(accountId, url);
      if (!mounted.current) return;
      setRoomMap((prev) => ({ ...prev, [accountId]: url }));
      setStatus(t("kuaishou.interact.enteredToast", { id: accountId }));
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      clearRowBusy(accountId);
    }
  }

  async function askSendDanmaku(accountId: string): Promise<void> {
    const content = danmaku.trim();
    if (!content) return;
    if (content.length > 500) {
      setActionError(t("kuaishou.interact.failedToast", { detail: t("kuaishou.interact.danmakuTooLong", { n: String(content.length) }) }));
      return;
    }
    const ok = await confirm({
      title: t("kuaishou.interact.sendConfirmTitle"),
      body: t("kuaishou.interact.sendConfirmBody", { content }),
      confirmLabel: t("kuaishou.interact.send"),
    });
    if (!ok) return;
    setRowBusy(accountId, "send");
    setActionError(null);
    try {
      const row = await subAccounts.sendDanmaku(accountId, content);
      if (!mounted.current) return;
      setStatus(row.ok ? t("kuaishou.interact.sentToast", { id: String(row.id) }) : (row.error ?? t("kuaishou.interact.sendFail")));
      if (interactAccount === accountId) {
        try {
          const list = await subAccounts.interactions(accountId);
          if (mounted.current) setInteractions(list);
        } catch {
          /* 历史刷新失败不覆盖发送结果 */
        }
      }
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      clearRowBusy(accountId);
    }
  }

  async function onQueryInteractions(accountId: string): Promise<void> {
    setRowBusy(accountId, "history");
    setActionError(null);
    try {
      const list = await subAccounts.interactions(accountId);
      if (!mounted.current) return;
      setInteractAccount(accountId);
      setInteractions(list);
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      clearRowBusy(accountId);
    }
  }

  async function askUnbind(row: InteractRow): Promise<void> {
    const ok = await confirm({
      title: t("kuaishou.interact.unbindConfirmTitle"),
      body: t("kuaishou.interact.unbindConfirmBody"),
      confirmLabel: t("kuaishou.interact.unbindConfirm"),
      destructive: true,
    });
    if (!ok) return;
    setRowBusy(row.record.id, "unbind");
    setActionError(null);
    try {
      await subAccounts.unbind(row.record.id);
      if (!mounted.current) return;
      setSelected((prev) => {
        const next = new Set(prev);
        next.delete(row.record.id);
        return next;
      });
      setStatus(t("kuaishou.interact.unboundToast"));
      await refresh();
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.failedToast", { detail: errorText(cause) }));
    } finally {
      clearRowBusy(row.record.id);
    }
  }

  /**
   * Delete = delete the environment (account-as-profile) **and** the interact
   * record: stop the hidden browser so no process keeps the data dir open,
   * remove the profile (cookies, login state, disk data), then drop the
   * registration entirely (`delete_sub_account`) so the row disappears from the
   * list. A record whose environment is already gone is deleted the same way.
   */
  async function askDelete(row: InteractRow): Promise<void> {
    const ok = await confirm({
      title: t("kuaishou.interact.deleteTitle"),
      body: t("kuaishou.interact.deleteBody", { name: row.record.displayName }),
      confirmLabel: t("kuaishou.interact.deleteConfirm"),
      destructive: true,
    });
    if (!ok) return;
    setRowBusy(row.record.id, "delete");
    setActionError(null);
    try {
      const profileId = row.record.profileId;
      if (profileId) {
        await profilesApi.close(profileId).catch(() => {});
        await profilesApi.delete(profileId);
      }
      // Remove the registration record itself — otherwise a profile_id=NULL row
      // would linger as an "environment missing" entry that never goes away.
      await subAccounts.delete(row.record.id);
      if (!mounted.current) return;
      setSelected((prev) => {
        const next = new Set(prev);
        next.delete(row.record.id);
        return next;
      });
      setStatus(t("kuaishou.interact.deletedToast"));
      await Promise.all([refresh(), refreshProfiles()]);
    } catch (cause) {
      if (mounted.current) setActionError(t("kuaishou.interact.deleteFailed", { error: errorText(cause) }));
    } finally {
      clearRowBusy(row.record.id);
    }
  }

  const columns: ReadonlyArray<DataTableColumn<InteractRow>> = useMemo(
    () => [
      {
        id: "account",
        header: (
          <span className="flex items-center gap-2">
            <input
              type="checkbox"
              aria-label={t("kuaishou.interact.selectAll")}
              checked={allSelected}
              onChange={toggleAll}
              onClick={(event) => event.stopPropagation()}
            />
            <span>{t("kuaishou.interact.col.account")}</span>
          </span>
        ),
        flex: true,
        cell: (row) => <AccountCell row={row} selected={selected.has(row.record.id)} onToggle={toggle} />,
      },
      {
        id: "kuaishouId",
        header: t("kuaishou.interact.col.kuaishouId"),
        width: 130,
        cell: (row) => {
          const detected = row.record.profileId
            ? entries.get(row.record.profileId)?.snapshot?.platformUserId
            : null;
          const id = row.record.platformUserId ?? detected ?? null;
          return (
            <span
              className={cn("mono block truncate text-[11px]", id ? "text-slate-300" : "text-slate-600")}
              title={id ?? undefined}
            >
              {id ?? "—"}
            </span>
          );
        },
      },
      {
        id: "status",
        header: t("kuaishou.interact.col.status"),
        width: 150,
        cell: (row) => <StatusCell row={row} />,
      },
      {
        id: "room",
        header: t("kuaishou.interact.col.room"),
        width: 150,
        showFrom: 960,
        cell: (row) => {
          const url = roomMap[row.record.id];
          return (
            <span
              className={cn("mono block truncate text-[11px]", url ? "text-slate-300" : "text-slate-600")}
              title={url ?? undefined}
            >
              {url ?? "—"}
            </span>
          );
        },
      },
      {
        id: "actions",
        header: t("kuaishou.interact.col.actions"),
        // Fits the whole row action set on one 36px line: launch/stop, enter
        // room, send, then five icon buttons (with slack for locale metrics).
        width: 460,
        align: "right",
        cell: (row) => {
          const op = busy[row.record.id];
          const pending = op !== undefined;
          const profileId = row.record.profileId;
          const running = !!row.profile?.isRunning;
          const liveEmpty = liveUrl.trim().length === 0;
          const dmEmpty = danmaku.trim().length === 0;
          return (
            <div className="flex items-center justify-end gap-1.5 whitespace-nowrap">
              {running ? (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={pending || !profileId}
                  title={t("kuaishou.interact.stop")}
                  aria-label={t("kuaishou.interact.stop")}
                  onClick={() => void onLaunch(row, "stop")}
                  leftIcon={
                    op === "stop" ? (
                      <Loader2 size={10} className="animate-spin" />
                    ) : (
                      <Square size={9} fill="currentColor" strokeWidth={0} />
                    )
                  }
                >
                  {t("kuaishou.interact.stop")}
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="accent"
                  disabled={pending || !profileId}
                  title={t("kuaishou.interact.launch")}
                  aria-label={t("kuaishou.interact.launch")}
                  onClick={() => void onLaunch(row, "launch")}
                  leftIcon={
                    op === "launch" ? (
                      <Loader2 size={10} className="animate-spin" />
                    ) : (
                      <Play size={10} fill="currentColor" strokeWidth={0} />
                    )
                  }
                >
                  {t("kuaishou.interact.launch")}
                </Button>
              )}
              <Button
                size="sm"
                variant="secondary"
                disabled={pending || liveEmpty}
                title={t("kuaishou.interact.enterRoom")}
                aria-label={t("kuaishou.interact.enterRoom")}
                onClick={() => void onEnterRoom(row.record.id)}
                leftIcon={op === "enter" ? <Loader2 size={10} className="animate-spin" /> : <DoorOpen size={10} />}
              >
                {t("kuaishou.interact.enterRoom")}
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={pending || dmEmpty}
                title={t("kuaishou.interact.send")}
                aria-label={t("kuaishou.interact.send")}
                onClick={() => void askSendDanmaku(row.record.id)}
                leftIcon={op === "send" ? <Loader2 size={10} className="animate-spin" /> : <Send size={10} />}
              >
                {t("kuaishou.interact.send")}
              </Button>
              <Button
                size="icon"
                variant="ghost"
                disabled={pending}
                title={t("kuaishou.interact.history")}
                aria-label={t("kuaishou.interact.history")}
                onClick={() => void onQueryInteractions(row.record.id)}
                leftIcon={op === "history" ? <Loader2 size={12} className="animate-spin" /> : <History size={12} />}
              />
              <Button
                size="icon"
                variant="ghost"
                disabled={pending || !profileId}
                title={t("kuaishou.interact.relogin")}
                aria-label={t("kuaishou.interact.relogin")}
                onClick={() => void askRelogin(row)}
                leftIcon={op === "relogin" ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
              />
              <Button
                size="icon"
                variant="ghost"
                disabled={pending}
                title={t("kuaishou.interact.login")}
                aria-label={t("kuaishou.interact.login")}
                onClick={() => void onLogin(row.record.id)}
                leftIcon={op === "login" ? <Loader2 size={12} className="animate-spin" /> : <LogIn size={12} />}
              />
              <Button
                size="icon"
                variant="danger"
                disabled={pending || !profileId}
                title={profileId ? t("kuaishou.interact.unbind") : t("kuaishou.interact.unbindUnavailable")}
                aria-label={t("kuaishou.interact.unbind")}
                onClick={() => void askUnbind(row)}
                leftIcon={op === "unbind" ? <Loader2 size={12} className="animate-spin" /> : <Unlink size={12} />}
              />
              <Button
                size="icon"
                variant="danger"
                disabled={pending}
                title={t("kuaishou.interact.delete")}
                aria-label={t("kuaishou.interact.delete")}
                onClick={() => void askDelete(row)}
                leftIcon={op === "delete" ? <Loader2 size={12} className="animate-spin" /> : <Trash2 size={12} />}
              />
            </div>
          );
        },
      },
    ],
    // Row cells read live toolbar state; refresh the memo with them like the shop page does.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t, busy, entries, roomMap, selected, allSelected, liveUrl, danmaku],
  );

  const error = rowsError ? t("kuaishou.interact.loadingFailed", { error: rowsError }) : actionError;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col px-6 pt-4" data-testid="interact-page">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-[15px] font-semibold">{t("kuaishou.tab.interact")}</h2>
        <div className="flex-1" />
        <span className="mono text-[11px] text-slate-600">{records.length}</span>
        <Button
          size="sm"
          variant="primary"
          onClick={() => setWizardOpen(true)}
          leftIcon={<Plus size={12} />}
        >
          {t("kuaishou.interact.create")}
        </Button>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <div className="relative">
          <Search
            size={13}
            strokeWidth={1.5}
            className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-slate-500"
          />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("kuaishou.interact.searchPlaceholder")}
            aria-label={t("kuaishou.interact.searchPlaceholder")}
            className={cn(CONTROL, "w-full max-w-[240px] pl-8")}
          />
        </div>
        <input
          value={liveUrl}
          onChange={(event) => setLiveUrl(event.target.value)}
          placeholder="https://live.kuaishou.com/…"
          aria-label={t("kuaishou.interact.liveUrl")}
          className={cn(CONTROL, "w-[220px]")}
        />
        <input
          value={danmaku}
          onChange={(event) => setDanmaku(event.target.value)}
          placeholder={t("kuaishou.interact.danmaku")}
          aria-label={t("kuaishou.interact.danmaku")}
          className={cn(CONTROL, "w-[220px]")}
        />
        <Button
          size="sm"
          variant="secondary"
          disabled={selected.size === 0 || batchBusy || batchDeleteBusy}
          onClick={() => void onBatchLogin()}
          leftIcon={batchBusy ? <Loader2 size={10} className="animate-spin" /> : undefined}
        >
          {t("kuaishou.interact.batchLogin", { n: String(selected.size) })}
        </Button>
        <Button
          size="sm"
          variant="danger"
          disabled={selected.size === 0 || batchBusy || batchDeleteBusy}
          onClick={() => void onBatchDelete()}
          leftIcon={batchDeleteBusy ? <Loader2 size={10} className="animate-spin" /> : <Trash2 size={12} />}
        >
          {t("kuaishou.interact.batchDelete", { n: String(selected.size) })}
        </Button>
      </div>

      {error && (
        <p role="alert" className="mt-2 text-[12px] text-amber-300">
          {error}
        </p>
      )}
      {status && (
        <p role="status" className="mt-2 text-[12px] text-emerald-300">
          {status}
        </p>
      )}

      <div className="mt-3 flex min-h-0 flex-1 flex-col">
        <DataTable
          ariaLabel={t("kuaishou.tab.interact")}
          columns={columns}
          rows={filtered}
          rowKey={(row) => row.record.id}
          loading={firstLoad}
          empty={
            <div className="py-16 text-center text-[13px] text-slate-500">
              {records.length === 0 ? t("kuaishou.interact.empty") : t("kuaishou.interact.emptyFiltered")}
            </div>
          }
        />
      </div>

      {(results.length > 0 || interactAccount) && (
        <div className="mt-3 flex max-h-[38%] flex-none flex-col gap-3 overflow-y-auto pb-4">
          {results.length > 0 && (
            <section aria-label={t("kuaishou.interact.resultsTitle")}>
              <h3 className="mb-1 text-[13px] font-semibold text-slate-200">{t("kuaishou.interact.resultsTitle")}</h3>
              <ul className="flex flex-col gap-1 text-sm" data-testid="interact-results">
                {results.map((r) => (
                  <li key={r.accountId} className="flex items-center gap-2">
                    <Pill kind={r.ok ? "running" : "error"}>{r.ok ? t("kuaishou.interact.ok") : t("kuaishou.interact.fail")}</Pill>
                    <span className="mono text-xs text-slate-300">{r.accountId}</span>
                    {r.error && <span className="text-xs text-red-300">{r.error}</span>}
                  </li>
                ))}
              </ul>
            </section>
          )}

          {interactAccount && (
            <section aria-label={t("kuaishou.interact.interactionsTitle")}>
              <h3 className="mb-1 text-[13px] font-semibold text-slate-200">
                {t("kuaishou.interact.interactionsTitle")} · <span className="mono text-xs">{interactAccount}</span>
              </h3>
              {interactions.length === 0 ? (
                <p className="text-[12px] text-slate-500">{t("kuaishou.interact.interactionsEmpty")}</p>
              ) : (
                <ul className="flex flex-col gap-1 text-sm" data-testid="interact-interactions">
                  {interactions.map((row) => (
                    <li key={row.id} className="flex items-center gap-2">
                      <Pill kind={row.ok ? "running" : "error"}>{row.action}</Pill>
                      <span className="truncate text-[12px] text-slate-300">
                        {row.message ?? row.liveRoomUrl ?? `#${row.id}`}
                      </span>
                      {row.error && <span className="text-xs text-red-300">{row.error}</span>}
                    </li>
                  ))}
                </ul>
              )}
            </section>
          )}
        </div>
      )}

      {/* 建号向导：创建环境 → 隐藏启动快手主站 → 扫码 → 登记互动账号。 */}
      <KuaishouInteractWizard
        open={wizardOpen}
        onClose={() => setWizardOpen(false)}
        onCreated={() => void refreshProfiles()}
        onRegistered={() => {
          void refresh();
          void refreshProfiles();
        }}
      />
    </div>
  );
}

/** Avatar (running ring) + display name + the interact record id. */
function AccountCell({
  row,
  selected,
  onToggle,
}: {
  row: InteractRow;
  selected: boolean;
  onToggle: (id: string) => void;
}): JSX.Element {
  const t = useT();
  const { entries } = useKuaishouIdentities();
  const snapshot = row.record.profileId ? (entries.get(row.record.profileId)?.snapshot ?? null) : null;
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <input
        type="checkbox"
        aria-label={t("kuaishou.interact.select", { name: row.record.displayName })}
        checked={selected}
        onChange={() => onToggle(row.record.id)}
        onClick={(event) => event.stopPropagation()}
      />
      <span
        className="flex shrink-0 rounded-[9px] p-[1.5px] transition-colors"
        style={{
          background: row.profile?.isRunning ? "rgba(52,211,153,0.6)" : "rgba(148,163,184,0.3)",
        }}
      >
        {snapshot?.platformUserId ? (
          <IdentityAvatar snapshot={snapshot} size={24} />
        ) : (
          <Avatar initials={row.record.displayName.slice(0, 2).toUpperCase() || "?"} size={24} />
        )}
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-slate-100" title={row.record.displayName}>
        {row.record.displayName}
      </span>
      <span className="mono max-w-[96px] shrink truncate text-[11px] text-slate-500" title={row.record.id}>
        {row.record.id}
      </span>
    </div>
  );
}

/** Detection state, localized here rather than from the provider (whose labels
 *  are Chinese-only) so the column follows the UI language. */
const STATUS_KEY: Record<string, TranslationKey> = {
  unknown: "kuaishou.interact.status.unknown",
  detected: "kuaishou.interact.status.detected",
  "not-detected": "kuaishou.interact.status.notDetected",
  conflict: "kuaishou.interact.status.conflict",
  closed: "kuaishou.interact.status.closed",
  skipped: "kuaishou.interact.status.skipped",
  error: "kuaishou.interact.status.error",
};

/** Running state + backend identity detection for one row. */
function StatusCell({ row }: { row: InteractRow }): JSX.Element {
  const t = useT();
  if (!row.record.profileId) {
    return <Pill kind="error">{t("kuaishou.interact.missingProfile")}</Pill>;
  }
  return <IdentityStatus profileId={row.record.profileId} />;
}

function IdentityStatus({ profileId }: { profileId: string }): JSX.Element {
  const t = useT();
  const identity = useKuaishouIdentity(profileId);
  if (!identity.profile) return <Pill kind="error">{t("kuaishou.interact.missingProfile")}</Pill>;
  if (identity.pending) return <Pill kind="pending">{t("kuaishou.interact.status.detecting")}</Pill>;
  if (identity.error) {
    return (
      <span title={identity.error} className="inline-flex min-w-0">
        <Pill kind="error">{t("kuaishou.interact.status.error")}</Pill>
      </span>
    );
  }
  const kind = identity.current ? "running" : identity.status === "conflict" ? "error" : "idle";
  return <Pill kind={kind}>{t(STATUS_KEY[identity.status] ?? STATUS_KEY.unknown)}</Pill>;
}
