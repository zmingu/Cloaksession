import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { Loader2, Plus, RefreshCw, Trash2 } from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import type { TranslationKey } from "../../i18n/en";
import { onJinniuAccountsChanged, onJinniuStatusChanged, jinniu } from "../../lib/jinniu";
import type { JinniuAccountWithStatus, JinniuStatePayload, JinniuStatus } from "../../types";
import { Button } from "../atoms/Button";
import { confirm } from "../atoms/Modal";
import { Pill, type PillKind } from "../atoms/Pill";

/**
 * Status wire value → localized label key. The backend keeps the jieger
 * five-state machine, but awaiting / connected both present as 已启动：
 * 选子户不再是可见阶段（后端仍在后台捕获子户信息）。
 */
const STATUS_KEY: Record<JinniuStatus, TranslationKey> = {
  disconnected: "jinniu.accounts.status.stopped",
  connecting: "jinniu.accounts.status.starting",
  "awaiting-sub-account": "jinniu.accounts.status.started",
  connected: "jinniu.accounts.status.started",
  error: "jinniu.accounts.status.error",
};

/** Status wire value → badge tone. */
const STATUS_PILL: Record<JinniuStatus, PillKind> = {
  disconnected: "idle",
  connecting: "pending",
  "awaiting-sub-account": "running",
  connected: "running",
  error: "error",
};

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 磁力金牛账号页（账户管理）。
 *
 * 订阅 `jinniu-status-changed` 与 `jinniu-accounts-changed` 实时刷新。
 *
 * 账户模型：一个账户只有 启动 / 停止 两个动作，无确认弹窗。启动打开金牛
 * 浏览器（复用已保存登录态或扫码），同时自动停止其它账户的会话并把本账户
 * 置为当前；扫码完成即识别右上角主账号（用户名 + 快手ID）并自动命名，
 * 选子户不是可见阶段（后端仍在后台捕获子户信息）。状态对外只有
 * 未启动 / 启动中 / 已启动 / 错误。
 *
 * 删除为破坏性操作，保留二次确认。
 */
export function JinniuAccountsPage(): JSX.Element {
  const t = useT();
  const [rows, setRows] = useState<JinniuAccountWithStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const fail = useCallback((cause: unknown) => {
    if (!mounted.current) return;
    setError(errorText(cause));
    setNotice(null);
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    try {
      const list = await jinniu.list();
      if (!mounted.current) return;
      setRows(list);
      setListError(null);
    } catch (cause) {
      if (mounted.current) setListError(errorText(cause));
    }
  }, []);

  // Initial load.
  useEffect(() => {
    let active = true;
    void refresh().finally(() => {
      if (active) setLoading(false);
    });
    return () => {
      active = false;
    };
  }, [refresh]);

  // Backend pushes: a status snapshot patches one row in place; a list change
  // re-reads the whole list. Both unlisten on unmount.
  useEffect(() => {
    let offStatus = (): void => {};
    let offAccounts = (): void => {};
    let active = true;
    void onJinniuStatusChanged((payload) => {
      if (!active) return;
      setRows((prev) => prev.map((row) => (row.id === payload.accountId ? mergeStatus(row, payload) : row)));
    }).then((fn) => {
      if (active) offStatus = fn;
      else fn();
    });
    void onJinniuAccountsChanged(() => {
      void refresh();
    }).then((fn) => {
      if (active) offAccounts = fn;
      else fn();
    });
    return () => {
      active = false;
      offStatus();
      offAccounts();
    };
  }, [refresh]);

  /**
   * 添加账户：不手输名称，先落占位行并直接进入识别登录——扫码完成后由后端
   * 识别右上角主账号并自动命名，无需先选子户。
   */
  async function onAdd(): Promise<void> {
    if (busyId !== null) return;
    setBusyId("add");
    setError(null);
    setNotice(null);
    try {
      const created = await jinniu.add();
      if (!mounted.current) return;
      setNotice(t("jinniu.accounts.addedToast"));
      await refresh();
      const payload = await jinniu.login(created.id);
      if (!mounted.current) return;
      setRows((prev) => prev.map((item) => (item.id === created.id ? mergeStatus(item, payload) : item)));
    } catch (cause) {
      fail(cause);
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  /** 启动账户：直接打开金牛浏览器（复用登录态或扫码），无确认弹窗。 */
  async function onStart(row: JinniuAccountWithStatus): Promise<void> {
    if (busyId !== null) return;
    setBusyId(row.id);
    setError(null);
    setNotice(null);
    try {
      const payload = await jinniu.login(row.id);
      if (!mounted.current) return;
      setRows((prev) => prev.map((item) => (item.id === row.id ? mergeStatus(item, payload) : item)));
    } catch (cause) {
      fail(cause);
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  async function onStop(row: JinniuAccountWithStatus): Promise<void> {
    if (busyId !== null) return;
    setBusyId(row.id);
    setError(null);
    setNotice(null);
    try {
      const payload = await jinniu.disconnect(row.id);
      if (!mounted.current) return;
      setRows((prev) => prev.map((item) => (item.id === row.id ? mergeStatus(item, payload) : item)));
      setNotice(t("jinniu.accounts.stoppedToast"));
    } catch (cause) {
      fail(cause);
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  async function onDelete(row: JinniuAccountWithStatus): Promise<void> {
    const ok = await confirm({
      title: t("jinniu.accounts.deleteConfirmTitle", { label: row.label }),
      body: t("jinniu.accounts.deleteConfirmBody"),
      confirmLabel: t("jinniu.accounts.deleteConfirmLabel"),
      destructive: true,
    });
    if (!ok) return;
    setBusyId(row.id);
    setError(null);
    setNotice(null);
    try {
      await jinniu.remove(row.id);
      if (!mounted.current) return;
      setRows((prev) => prev.filter((item) => item.id !== row.id));
      setNotice(t("jinniu.accounts.deletedToast"));
    } catch (cause) {
      fail(cause);
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  const busy = busyId !== null;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto px-6 py-4">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-[15px] font-semibold">{t("jinniu.accounts.title")}</h2>
        <div className="flex-1" />
        <span className="mono text-[11px] text-slate-600">{rows.length}</span>
        <Button
          size="sm"
          variant="secondary"
          disabled={busy}
          onClick={() => void refresh()}
          leftIcon={<RefreshCw size={12} />}
        >
          {t("jinniu.accounts.refresh")}
        </Button>
        <Button
          size="sm"
          variant="primary"
          disabled={busy}
          onClick={() => void onAdd()}
          leftIcon={
            busyId === "add" ? <Loader2 size={12} className="animate-spin" /> : <Plus size={12} />
          }
        >
          {busyId === "add" ? t("jinniu.accounts.adding") : t("jinniu.accounts.add")}
        </Button>
      </div>

      {listError && (
        <div role="alert" className="mt-3 text-[12px] text-amber-300">
          {t("jinniu.accounts.loadFailed", { error: listError })}
        </div>
      )}
      {error && (
        <div role="alert" className="mt-3 text-[12px] text-red-300">
          {t("jinniu.accounts.opFailed", { detail: error })}
        </div>
      )}
      {notice && (
        <div role="status" className="mt-3 text-[12px] text-emerald-300">
          {notice}
        </div>
      )}

      <section aria-label={t("jinniu.accounts.title")} className="mt-3">
        {loading ? (
          <p className="text-[13px] text-muted-foreground">{t("jinniu.accounts.loading")}</p>
        ) : rows.length === 0 ? null : (
          <ul data-testid="jinniu-list" className="flex flex-col gap-2">
            {rows.map((row) => (
              <li
                key={row.id}
                data-testid={`jinniu-row-${row.id}`}
                data-active={row.isActive ? "true" : "false"}
                className="flex flex-wrap items-center gap-2 rounded-xl px-3 py-2.5"
                style={{
                  background: "rgba(255,255,255,0.03)",
                  boxShadow: row.isActive
                    ? "inset 0 0 0 1px rgba(168,85,247,0.45)"
                    : "inset 0 0 0 1px rgba(255,255,255,0.06)",
                }}
              >
                <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-slate-100">
                  {row.label}
                </span>

                <span data-testid={`jinniu-status-${row.id}`}>
                  <Pill kind={STATUS_PILL[row.status]} dot>
                    {t(STATUS_KEY[row.status])}
                  </Pill>
                </span>

                {row.isActive && (
                  <span data-testid={`jinniu-active-${row.id}`}>
                    <Pill kind="info">{t("jinniu.accounts.active")}</Pill>
                  </span>
                )}

                <span className="min-w-0 max-w-[220px] truncate text-[11px] text-slate-500">
                  {row.currentSubAccountName ?? row.currentSubAccountId ?? "—"}
                </span>

                <div className="flex items-center gap-1.5">
                  {row.status === "connected" || row.status === "awaiting-sub-account" ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => void onStop(row)}
                    >
                      {t("jinniu.accounts.stop")}
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      variant="accent"
                      disabled={busy}
                      onClick={() => void onStart(row)}
                      leftIcon={
                        busyId === row.id ? <Loader2 size={10} className="animate-spin" /> : undefined
                      }
                    >
                      {busyId === row.id ? t("jinniu.accounts.starting") : t("jinniu.accounts.start")}
                    </Button>
                  )}
                  <Button
                    size="icon"
                    variant="danger"
                    disabled={busy}
                    title={t("jinniu.accounts.delete")}
                    aria-label={t("jinniu.accounts.delete")}
                    onClick={() => void onDelete(row)}
                    leftIcon={
                      busyId === row.id ? <Loader2 size={12} className="animate-spin" /> : <Trash2 size={12} />
                    }
                  />
                </div>

                {row.error && <p className="w-full text-[11px] text-red-300">{row.error}</p>}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

/**
 * Merge a pushed/returned status snapshot into one list row. The payload is an
 * authoritative snapshot for the connection state, so absent fields clear the
 * cached derived values (e.g. a disconnect wipes the captured master/sub).
 */
function mergeStatus(
  row: JinniuAccountWithStatus,
  payload: JinniuStatePayload,
): JinniuAccountWithStatus {
  return {
    ...row,
    status: payload.status,
    error: payload.error ?? null,
    targetAccountId: payload.targetAccountId ?? null,
    masterName: payload.master?.name ?? null,
    masterId: payload.master?.id ?? null,
    masterAvatarUrl: payload.master?.avatarUrl ?? null,
    currentSubAccountId: payload.currentSubAccountId ?? null,
    currentSubAccountName: payload.currentSubAccountName ?? null,
    balanceText: payload.balanceText ?? null,
  };
}
