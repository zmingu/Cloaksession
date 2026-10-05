import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from "react";
import { ArrowLeftRight, Loader2, Play, Plus, Search, Square, Trash2 } from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import type { TranslationKey } from "../../i18n/en";
import { cn } from "../../lib/cn";
import { onRunningChanged, profiles as profilesApi } from "../../lib/ipc";
import { useKuaishouIdentities, useKuaishouIdentity } from "../../lib/KuaishouIdentityProvider";
import {
  kuaishouSubject,
  type KuaishouInitErrorCode,
  type KuaishouInitStepRecord,
} from "../../lib/kuaishouSubject";
import { businessAccounts } from "../../lib/businessAccounts";
import type { ProfileSummary } from "../../types";
import { Avatar, Pill, confirm } from "../atoms";
import { Button } from "../atoms/Button";
import { IdentityAvatar } from "../profile/KuaishouIdentity";
import { DataTable, type DataTableColumn } from "../table/DataTable";
import { KuaishouConvertInteractDialog } from "./KuaishouConvertInteractDialog";

const CONTROL =
  "h-8 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 小店账号管理（快手 › 小店）。
 *
 * 第一版只做账号列表：把浏览器环境当作小店账号视角，展示快手头像、昵称、
 * 快手ID、检测状态，并提供启动/停止。
 *
 * 数据来源与「浏览器配置」同一份（profiles_list + 身份检测 provider），
 * 因此不会出现两套账号真相。加货架 / 上下车 / 自动发言 / 慧播开播属于
 * 后续批次，不在此页。
 */
export function KuaishouShopAccounts({ onAddAccount }: { onAddAccount?: () => void }): JSX.Element {
  const t = useT();
  const { entries, listError } = useKuaishouIdentities();

  const [rows, setRows] = useState<ProfileSummary[]>([]);
  const [rowsError, setRowsError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<Record<string, "launch" | "stop" | "delete">>({});
  const [actionError, setActionError] = useState<string | null>(null);
  const [firstLoad, setFirstLoad] = useState(true);
  // Which environment is being converted into an interact account, if any.
  const [convert, setConvert] = useState<{ id: string; name: string } | null>(null);
  const mounted = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    try {
      const list = await profilesApi.list();
      if (!mounted.current) return;
      // 小店列表排除已被其它账号视角登记的环境：互动账号（kind=kuaishou-sub）
      // 和金牛账户（kind=jinniu）各有专属列表。读共享的 business_accounts 表
      // （list_sub_accounts 只返回互动 kind，金牛会漏进小店列表）。过滤失败时
      // 降级为不过滤，绝不阻塞主列表。
      let takenProfileIds = new Set<string>();
      try {
        const records = await businessAccounts.list();
        if (!mounted.current) return;
        takenProfileIds = new Set(
          records
            .filter(
              (record) =>
                (record.kind === "kuaishou-sub" || record.kind === "jinniu") &&
                record.profileId,
            )
            .map((record) => record.profileId as string),
        );
      } catch {
        takenProfileIds = new Set();
      }
      if (!mounted.current) return;
      setRows(
        takenProfileIds.size === 0
          ? list
          : list.filter((profile) => !takenProfileIds.has(profile.id)),
      );
      setRowsError(null);
    } catch (cause) {
      if (mounted.current) setRowsError(errorText(cause));
    }
  }, []);

  useEffect(() => {
    let active = true;
    void refresh().finally(() => {
      if (active) setFirstLoad(false);
    });
    return () => {
      active = false;
    };
  }, [refresh]);

  // Any running-state change (including a Chromium window the user closed
  // directly) refreshes this page's copy of the profile list, so the row's
  // launch/stop control converges without a manual action. The page keeps its
  // own `rows` state, so it cannot ride on App's global refresh.
  useEffect(() => {
    let off = (): void => {};
    let active = true;
    void onRunningChanged(() => {
      void refresh();
    }).then((fn) => {
      if (active) off = fn;
    });
    return () => {
      active = false;
      off();
    };
  }, [refresh]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle) return rows;
    return rows.filter((row) => {
      const id = entries.get(row.id)?.snapshot?.platformUserId ?? "";
      const nickname = entries.get(row.id)?.snapshot?.nickname ?? "";
      return [row.name, id, nickname, ...row.tags].some((value) =>
        value.toLocaleLowerCase().includes(needle),
      );
    });
  }, [rows, entries, query]);

  async function act(row: ProfileSummary, kind: "launch" | "stop"): Promise<void> {
    setBusy((prev) => ({ ...prev, [row.id]: kind }));
    setActionError(null);
    try {
      if (kind === "launch") {
        await profilesApi.launch(row.id);
      } else {
        await profilesApi.close(row.id);
      }
      const list = await profilesApi.list();
      if (mounted.current) setRows(list);
    } catch (cause) {
      setActionError(
        kind === "launch"
          ? t("kuaishou.shop.launchFailed", { error: errorText(cause) })
          : t("kuaishou.shop.stopFailed", { error: errorText(cause) }),
      );
    } finally {
      if (mounted.current) {
        setBusy((prev) => {
          const next = { ...prev };
          delete next[row.id];
          return next;
        });
      }
    }
  }

  async function remove(row: ProfileSummary): Promise<void> {
    const ok = await confirm({
      title: t("kuaishou.shop.deleteTitle"),
      body: t("kuaishou.shop.deleteBody", { name: row.name }),
      confirmLabel: t("kuaishou.shop.deleteConfirm"),
      destructive: true,
    });
    if (!ok) return;
    setBusy((prev) => ({ ...prev, [row.id]: "delete" }));
    setActionError(null);
    try {
      // Stop the hidden browser first so no process keeps the profile's data
      // dir open, then remove the profile (cookies, login state, disk data).
      await profilesApi.close(row.id).catch(() => {});
      await profilesApi.delete(row.id);
      const list = await profilesApi.list();
      if (mounted.current) setRows(list);
    } catch (cause) {
      setActionError(t("kuaishou.shop.deleteFailed", { error: errorText(cause) }));
    } finally {
      if (mounted.current) {
        setBusy((prev) => {
          const next = { ...prev };
          delete next[row.id];
          return next;
        });
      }
    }
  }

  const columns: ReadonlyArray<DataTableColumn<ProfileSummary>> = useMemo(
    () => [
      {
        id: "account",
        header: t("kuaishou.shop.col.account"),
        flex: true,
        cell: (row) => <AccountCell row={row} />,
      },
      {
        id: "kuaishouId",
        header: t("kuaishou.shop.col.kuaishouId"),
        width: 130,
        cell: (row) => {
          const id = entries.get(row.id)?.snapshot?.platformUserId;
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
        id: "init",
        header: t("kuaishou.shop.col.init"),
        width: 170,
        showFrom: 820,
        cell: (row) => <InitCell row={row} />,
      },
      {
        id: "status",
        header: t("kuaishou.shop.col.status"),
        width: 150,
        cell: (row) => <StatusCell row={row} />,
      },
      {
        id: "tags",
        header: t("kuaishou.shop.col.tags"),
        width: 150,
        showFrom: 960,
        cell: (row) => (
          <div className="flex gap-1 overflow-hidden">
            {row.tags.slice(0, 2).map((tag) => (
              <span
                key={tag}
                className="mz-pill mono max-w-[80px] truncate text-slate-400"
                style={{
                  background: "rgba(255,255,255,0.04)",
                  boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.05)",
                }}
              >
                {tag}
              </span>
            ))}
            {row.tags.length > 2 && (
              <span className="mz-pill mono shrink-0 text-slate-600">+{row.tags.length - 2}</span>
            )}
          </div>
        ),
      },
      {
        id: "actions",
        header: "",
        width: 190,
        align: "right",
        cell: (row) => (
          <ActionCell
            row={row}
            busy={busy[row.id]}
            onAct={act}
            onDelete={remove}
            onConvert={(target) => setConvert({ id: target.id, name: target.name })}
          />
        ),
      },
    ],
    // `busy` / `act` are read inside cells; the memo intentionally refreshes
    // with them so the row button reflects the in-flight state.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [t, entries, busy],
  );

  const error = rowsError ? t("kuaishou.shop.loadingFailed", { error: rowsError }) : actionError;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col px-6 pt-4">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-[15px] font-semibold">{t("kuaishou.tab.shop")}</h2>
        <div className="flex-1" />
        <span className="mono text-[11px] text-slate-600">{rows.length}</span>
        {onAddAccount && (
          <Button size="sm" variant="primary" onClick={onAddAccount} leftIcon={<Plus size={12} />}>
            {t("kuaishou.shop.add")}
          </Button>
        )}
      </div>

      <div className="relative mt-3">
        <Search
          size={13}
          strokeWidth={1.5}
          className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-slate-500"
        />
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("kuaishou.shop.searchPlaceholder")}
          aria-label={t("kuaishou.shop.searchPlaceholder")}
          className={cn(CONTROL, "w-full max-w-[360px] pl-8")}
        />
      </div>

      {listError && <p role="alert" className="mt-2 text-[12px] text-amber-300">{listError}</p>}
      {error && <p role="alert" className="mt-2 text-[12px] text-amber-300">{error}</p>}

      <div className="mt-3 flex min-h-0 flex-1 flex-col">
        <DataTable
          ariaLabel={t("kuaishou.tab.shop")}
          columns={columns}
          rows={filtered}
          rowKey={(row) => row.id}
          loading={firstLoad}
          empty={
            <div className="py-16 text-center text-[13px] text-slate-500">
              {rows.length === 0 ? t("kuaishou.shop.empty") : t("kuaishou.shop.emptyFiltered")}
            </div>
          }
        />
      </div>

      {convert && (
        <KuaishouConvertInteractDialog
          open
          profileId={convert.id}
          profileName={convert.name}
          onClose={() => {
            setConvert(null);
            // A cancelled run unbinds the record again, so re-read to bring the
            // environment back into the list; a successful run re-reads too.
            void refresh();
          }}
          onConverted={() => void refresh()}
        />
      )}
    </div>
  );
}

/** Avatar + profile name, with the running state as a ring on the avatar.
 *  The avatar/name block is a real button that opens the shared Kuaishou
 *  identity dialog (mounted once at the app root). */
function AccountCell({ row }: { row: ProfileSummary }): JSX.Element {
  const t = useT();
  const { entries, openDetails } = useKuaishouIdentities();
  const snapshot = entries.get(row.id)?.snapshot ?? null;
  const label = t("kuaishou.shop.viewDetails", { name: row.name });
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <button
        type="button"
        aria-label={label}
        title={label}
        onClick={() => openDetails(row.id)}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 rounded-md text-left outline-none hover:bg-white/[0.04] focus-visible:ring-1 focus-visible:ring-purple-400/60"
      >
        <span
          className="flex shrink-0 rounded-[9px] p-[1.5px] transition-colors"
          style={{
            background: row.isRunning ? "rgba(52,211,153,0.6)" : "rgba(148,163,184,0.3)",
          }}
        >
          {snapshot?.platformUserId ? (
            <IdentityAvatar snapshot={snapshot} size={24} />
          ) : (
            <Avatar initials={row.name.slice(0, 2).toUpperCase()} size={24} />
          )}
        </span>
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-slate-100 hover:underline">
          {row.name}
        </span>
      </button>
      {/* The wizard renames the environment from the detected nickname, so the two
       *  strings are usually identical. Only surface the nickname when it adds
       *  information — otherwise the row would show the same name twice. */}
      {snapshot?.nickname && snapshot.nickname !== row.name && (
        <span className="min-w-0 max-w-[40%] shrink truncate text-[11px] text-slate-500" title={snapshot.nickname}>
          {snapshot.nickname}
        </span>
      )}
    </div>
  );
}

const INIT_STATE_KEY: Record<KuaishouInitStepRecord["state"], TranslationKey> = {
  pending: "kuaishou.init.state.pending",
  running: "kuaishou.init.state.running",
  done: "kuaishou.init.state.done",
  failed: "kuaishou.init.state.failed",
};

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

const INIT_POLL_MS = 5000;

/**
 * Per-row initialization status (subject profile + live-slice off).
 *
 * Cost control: the cell is mounted per row, so it must never start a request
 * for a row without a `platformUserId` — those render a dim dash and do no IPC.
 * It reads once when an id appears, then polls at a low frequency *only* while
 * a step is still `pending`/`running`. Two `done` steps, a terminal `failed`,
 * an unmount, or an id change all stop the timer. Reads go to the read-only
 * `kuaishou_init_steps` command, so this works for a stopped environment too
 * (it then shows the persisted, historical state).
 */
function InitCell({ row }: { row: ProfileSummary }): JSX.Element {
  const t = useT();
  const snapshot = useKuaishouIdentities().entries.get(row.id)?.snapshot ?? null;
  const platformUserId = snapshot?.platformUserId ?? null;
  // Backend observation marker, compared for equality only (never to a local clock).
  const observation = snapshot?.checkedAt ?? null;
  const [steps, setSteps] = useState<KuaishouInitStepRecord[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setSteps(null);
    setFailed(false);
    if (!platformUserId) return;
    let active = true;
    let timer: number | undefined;
    const stop = (): void => {
      if (timer !== undefined) {
        window.clearTimeout(timer);
        timer = undefined;
      }
    };
    const load = (): void => {
      void kuaishouSubject.steps(platformUserId).then(
        (value) => {
          if (!active) return;
          setSteps(value);
          setFailed(false);
          if (value.some((step) => step.state === "pending" || step.state === "running")) {
            timer = window.setTimeout(load, INIT_POLL_MS);
          }
        },
        () => {
          if (active) setFailed(true);
        },
      );
    };
    load();
    return () => {
      active = false;
      stop();
    };
  }, [platformUserId, observation]);

  if (!platformUserId) {
    return (
      <span
        data-testid={`init-cell-${row.id}`}
        data-state="absent"
        className="block truncate text-[11px] text-slate-600"
        title={t("kuaishou.init.state.none")}
      >
        —
      </span>
    );
  }
  if (failed) {
    return (
      <span
        data-testid={`init-cell-${row.id}`}
        data-state="error"
        className="block truncate text-[11px] text-amber-300"
      >
        {t("kuaishou.init.state.failed")}
      </span>
    );
  }
  if (steps === null) {
    return (
      <span data-testid={`init-cell-${row.id}`} data-state="loading" className="block truncate text-[11px] text-slate-500">
        …
      </span>
    );
  }
  const subject = steps.find((step) => step.step === "subject");
  const slice = steps.find((step) => step.step === "slice");
  const complete = subject?.state === "done" && slice?.state === "done";
  const running = steps.some((step) => step.state === "running");
  const broken = steps.some((step) => step.state === "failed");
  const tone = complete
    ? "text-emerald-300"
    : broken
      ? "text-amber-300"
      : running
        ? "text-sky-300"
        : "text-slate-400";
  const state = steps.length === 0 ? "none" : complete ? "done" : broken ? "failed" : running ? "running" : "pending";
  const summary = steps.length === 0
    ? t("kuaishou.init.state.none")
    : complete
      ? t("kuaishou.init.state.done")
      : broken
        ? t("kuaishou.init.state.failed")
        : running
          ? t("kuaishou.init.state.running")
          : t("kuaishou.init.state.pending");
  const errorCode = [...steps].reverse().find((step) => step.lastErrorCode)?.lastErrorCode ?? null;
  const detail = [
    `${t("kuaishou.init.step.subject")}: ${subject ? t(INIT_STATE_KEY[subject.state]) : t("kuaishou.init.state.none")}`,
    `${t("kuaishou.init.step.slice")}: ${slice ? t(INIT_STATE_KEY[slice.state]) : t("kuaishou.init.state.none")}`,
    ...(errorCode ? [t(INIT_ERROR_KEY[errorCode])] : []),
  ].join(" · ");
  return (
    <span
      data-testid={`init-cell-${row.id}`}
      data-state={state}
      className={cn("block truncate text-[11px]", tone)}
      title={detail}
    >
      {summary}
    </span>
  );
}

/** Detection state, localized here rather than from the provider (whose labels
 *  are Chinese-only) so the column follows the UI language. */
const STATUS_KEY: Record<string, TranslationKey> = {
  unknown: "kuaishou.shop.status.unknown",
  detected: "kuaishou.shop.status.detected",
  "not-detected": "kuaishou.shop.status.notDetected",
  conflict: "kuaishou.shop.status.conflict",
  closed: "kuaishou.shop.status.closed",
  skipped: "kuaishou.shop.status.skipped",
  error: "kuaishou.shop.status.error",
};

function StatusCell({ row }: { row: ProfileSummary }): JSX.Element {
  const t = useT();
  const identity = useKuaishouIdentity(row.id);
  if (identity.pending) return <Pill kind="pending">{t("kuaishou.shop.status.pending")}</Pill>;
  if (identity.error) {
    return (
      <span title={identity.error} className="inline-flex min-w-0">
        <Pill kind="error">{t("kuaishou.shop.status.error")}</Pill>
      </span>
    );
  }
  const kind = identity.current ? "running" : identity.status === "conflict" ? "error" : "idle";
  return <Pill kind={kind}>{t(STATUS_KEY[identity.status] ?? STATUS_KEY.unknown)}</Pill>;
}

/** Launch / stop plus a destructive delete, mirrored from the profiles list;
 *  the writes go to the same backend. Deleting the account = deleting the
 *  browser profile (account-as-profile), behind a confirm dialog.
 *  「转为互动账号」opens the conversion dialog for this environment. */
function ActionCell({
  row,
  busy,
  onAct,
  onDelete,
  onConvert,
}: {
  row: ProfileSummary;
  busy: "launch" | "stop" | "delete" | undefined;
  onAct: (row: ProfileSummary, kind: "launch" | "stop") => Promise<void>;
  onDelete: (row: ProfileSummary) => Promise<void>;
  onConvert: (row: ProfileSummary) => void;
}): JSX.Element {
  const t = useT();
  const pending = busy !== undefined;
  const deleting = busy === "delete";
  return (
    <div className="flex items-center justify-end gap-1.5">
      {row.isRunning ? (
        <Button
          size="sm"
          variant="secondary"
          disabled={pending}
          onClick={() => void onAct(row, "stop")}
          leftIcon={
            pending ? (
              <Loader2 size={10} className="animate-spin" />
            ) : (
              <Square size={9} fill="currentColor" strokeWidth={0} />
            )
          }
        >
          {t("kuaishou.shop.stop")}
        </Button>
      ) : (
        <Button
          size="sm"
          variant="accent"
          disabled={pending}
          onClick={() => void onAct(row, "launch")}
          leftIcon={
            pending ? (
              <Loader2 size={10} className="animate-spin" />
            ) : (
              <Play size={10} fill="currentColor" strokeWidth={0} />
            )
          }
        >
          {t("kuaishou.shop.launch")}
        </Button>
      )}
      <Button
        size="icon"
        variant="secondary"
        disabled={pending}
        title={t("kuaishou.shop.convert")}
        aria-label={t("kuaishou.shop.convert")}
        onClick={() => onConvert(row)}
        leftIcon={<ArrowLeftRight size={12} />}
      />
      <Button
        size="icon"
        variant="danger"
        disabled={pending}
        title={t("kuaishou.shop.delete")}
        aria-label={t("kuaishou.shop.delete")}
        onClick={() => void onDelete(row)}
        leftIcon={
          deleting ? <Loader2 size={12} className="animate-spin" /> : <Trash2 size={12} />
        }
      />
    </div>
  );
}
