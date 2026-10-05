import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { Loader2, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";

import { useT } from "../../i18n/LanguageProvider";
import type { TranslationKey } from "../../i18n/en";
import { mateLogin, onMateLoginStateChanged } from "../../lib/mateLogin";
import type { MateAccount, MateLoginStage, MateLoginState } from "../../types";
import { Button } from "../atoms/Button";
import { confirm, Modal } from "../atoms/Modal";
import { Pill, type PillKind } from "../atoms/Pill";

/**
 * 9 阶段 → 中文/英文文案键（对齐 jieger `MateLoginCard.tsx` 的 `STAGE_LABEL`）。
 * 阶段取值是后端 `#[serde(rename_all = "kebab-case")]` 的 `MateLoginStage`。
 */
const STAGE_KEY: Record<MateLoginStage, TranslationKey> = {
  idle: "biz.mate.stage.idle",
  starting: "biz.mate.stage.starting",
  "awaiting-scan": "biz.mate.stage.awaitingScan",
  "awaiting-confirm": "biz.mate.stage.awaitingConfirm",
  receiving: "biz.mate.stage.receiving",
  success: "biz.mate.stage.success",
  expired: "biz.mate.stage.expired",
  cancelled: "biz.mate.stage.cancelled",
  error: "biz.mate.stage.error",
};

/** 阶段 → 徽章色调。 */
const STAGE_PILL: Record<MateLoginStage, PillKind> = {
  idle: "idle",
  starting: "pending",
  "awaiting-scan": "pending",
  "awaiting-confirm": "pending",
  receiving: "pending",
  success: "running",
  expired: "error",
  cancelled: "error",
  error: "error",
};

function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

/**
 * 直播伴侣账号管理页（快手账号 › 直播伴侣）。
 *
 * 对齐 jieger `src/pages/live-launch/MateLoginCard.tsx`：账号列表（别名 + 阶段
 * 徽章 + 已登录时的 `昵称 · UID`）/ 添加 / 重命名 / 删除（二次确认），每行可发起
 * 「扫码登录 / 重新登录」，扫码走 QR 弹窗（`qrImageDataUrl`）并显示阶段推进。
 *
 * 「添加账号」不再要求先填别名：直接建行（后端落占位别名）并立即出二维码，首次
 * 扫码成功后由后端以平台昵称命名。若扫码失败或用户中途关闭弹窗，本页会自动删除
 * 这次新建的占位行（`pendingAddId`），不留「未命名伴侣」残渣；早先已存在的账号
 * 不受影响。
 *
 * 数据流：进入页拉 `mate_accounts_list`，逐个拉 `mate_login_state`；订阅
 * `mate-login-state-changed` 实时推进阶段（成功时重拉列表以同步真实昵称）。
 * 组件卸载时清理监听。伴侣账号不绑浏览器环境，因此本页不涉及环境/开播。
 *
 * 无 props，可被 `KuaishouAccountsPage` 直接挂载。
 */
export function MateLoginPage(): JSX.Element {
  const t = useT();
  const [accounts, setAccounts] = useState<MateAccount[]>([]);
  const [stateMap, setStateMap] = useState<Record<string, MateLoginState>>({});
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [renameTarget, setRenameTarget] = useState<MateAccount | null>(null);
  const [renameLabel, setRenameLabel] = useState("");
  const [qrAccountId, setQrAccountId] = useState<string | null>(null);
  /**
   * 由「添加账号」新建、尚未登录成功的占位行 id（ref 保存，不参与渲染）。
   *
   * 该行一旦扫码失败、过期，或用户直接关闭二维码弹窗，就会被自动删除，避免留下
   * 「未命名伴侣」残渣；登录成功后清空（此时后端已用平台昵称命名该行）。
   * 非空即代表当前这次添加还「未完成」。
   */
  const pendingAddIdRef = useRef<string | null>(null);

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

  /** Re-read the account list and every account's login snapshot. */
  const refresh = useCallback(async (): Promise<void> => {
    try {
      const list = await mateLogin.list();
      if (!mounted.current) return;
      setAccounts(list);
      setListError(null);
      const snapshots = await Promise.all(
        list.map(async (account) => {
          try {
            return [account.id, await mateLogin.state(account.id)] as const;
          } catch {
            return null;
          }
        }),
      );
      if (!mounted.current) return;
      const next: Record<string, MateLoginState> = {};
      for (const entry of snapshots) {
        if (entry !== null) next[entry[0]] = entry[1];
      }
      setStateMap(next);
    } catch (cause) {
      if (mounted.current) setListError(errorText(cause));
    }
  }, []);

  /**
   * 丢弃一次「添加账号」新建、但始终未登录成功的占位行。
   *
   * 本地立即摘除该行（列表 / 阶段快照 / 若正显示它的二维码弹窗一并清理），随后
   * 尽力通知后端删除；后端若已无此行（例如已被用户手动删除）则静默忽略。重复调用
   * 是安全的（`pendingAddIdRef` 只在匹配时清空，删除失败被吞掉）。
   */
  const cleanupPendingAdd = useCallback((id: string): void => {
    if (pendingAddIdRef.current !== id) return;
    pendingAddIdRef.current = null;
    setAccounts((prev) => prev.filter((item) => item.id !== id));
    setStateMap((prev) => {
      if (!(id in prev)) return prev;
      const next = { ...prev };
      delete next[id];
      return next;
    });
    setQrAccountId((prev) => (prev === id ? null : prev));
    void mateLogin.remove(id).catch(() => {
      // Best-effort: the row may already be gone (or the store is unavailable).
    });
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

  // Backend push: patch the matching snapshot; a success also re-reads the list
  // so the freshly persisted nickname / UID show up, and confirms the pending
  // add (the row now has a real name and must be kept). A failed scan for the
  // pending row drops it instead, so no placeholder row is left behind.
  // Unlisten on unmount.
  useEffect(() => {
    let off = (): void => {};
    let active = true;
    void onMateLoginStateChanged((payload) => {
      if (!active) return;
      setStateMap((prev) => ({ ...prev, [payload.accountId]: payload }));
      if (payload.stage === "success") {
        if (pendingAddIdRef.current === payload.accountId) pendingAddIdRef.current = null;
        void refresh();
        return;
      }
      if (
        (payload.stage === "expired" || payload.stage === "error") &&
        pendingAddIdRef.current === payload.accountId
      ) {
        cleanupPendingAdd(payload.accountId);
      }
    }).then((fn) => {
      if (active) off = fn;
      else fn();
    });
    return () => {
      active = false;
      off();
    };
  }, [refresh, cleanupPendingAdd]);

  /**
   * 添加账号：不传别名直接建行（后端落占位别名），随即发起扫码；首次扫码成功后
   * 后端会用平台昵称覆盖该行别名，`success` 事件触发的 `refresh()` 会刷新列表。
   *
   * 该行在扫码成功前一直记为「待添加」（`pendingAddIdRef`）：中途关闭弹窗、扫码
   * 失败或过期都会被自动删除。建行或启动扫码失败时同样回滚，不留空壳行。
   */
  async function onAdd(): Promise<void> {
    if (busyId !== null) return;
    setBusyId("add");
    setError(null);
    setNotice(null);
    let created: MateAccount | null = null;
    try {
      const account = await mateLogin.add();
      created = account;
      if (!mounted.current) {
        // Unmounted mid-flight: drop the row we just created, best-effort.
        void mateLogin.remove(account.id).catch(() => {});
        return;
      }
      pendingAddIdRef.current = account.id;
      setAccounts((prev) => [...prev, account]);
      setQrAccountId(account.id);
      const snapshot = await mateLogin.start(account.id);
      if (!mounted.current) return;
      setStateMap((prev) => ({ ...prev, [account.id]: snapshot }));
    } catch (cause) {
      if (created !== null) cleanupPendingAdd(created.id);
      if (mounted.current) {
        setQrAccountId(null);
        fail(cause);
      }
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  async function onRename(): Promise<void> {
    const target = renameTarget;
    if (target === null) return;
    const label = renameLabel.trim();
    if (!label || busyId !== null) return;
    setBusyId(target.id);
    setError(null);
    setNotice(null);
    try {
      const renamed = await mateLogin.rename(target.id, label);
      if (!mounted.current) return;
      setRenameTarget(null);
      setRenameLabel("");
      setAccounts((prev) => prev.map((row) => (row.id === renamed.id ? renamed : row)));
      setNotice(t("biz.mate.renamedToast", { label: renamed.label }));
    } catch (cause) {
      fail(cause);
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  async function onDelete(row: MateAccount): Promise<void> {
    if (busyId !== null) return;
    const ok = await confirm({
      title: t("biz.mate.deleteConfirmTitle", { label: row.label }),
      body: t("biz.mate.deleteConfirmBody"),
      confirmLabel: t("biz.mate.deleteConfirmLabel"),
      destructive: true,
    });
    if (!ok) return;
    setBusyId(row.id);
    setError(null);
    setNotice(null);
    try {
      await mateLogin.remove(row.id);
      if (!mounted.current) return;
      // A manual delete supersedes any pending-add bookkeeping for this row.
      if (pendingAddIdRef.current === row.id) pendingAddIdRef.current = null;
      setQrAccountId((prev) => (prev === row.id ? null : prev));
      setAccounts((prev) => prev.filter((item) => item.id !== row.id));
      setNotice(t("biz.mate.deletedToast"));
    } catch (cause) {
      fail(cause);
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  async function onStartLogin(row: MateAccount): Promise<void> {
    if (busyId !== null) return;
    // A fresh scan supersedes any earlier abandoned add: that placeholder row is
    // no longer the one being completed, so stop tracking (and never auto-drop) it.
    pendingAddIdRef.current = null;
    setBusyId(row.id);
    setError(null);
    setNotice(null);
    setQrAccountId(row.id);
    try {
      const snapshot = await mateLogin.start(row.id);
      if (!mounted.current) return;
      setStateMap((prev) => ({ ...prev, [row.id]: snapshot }));
    } catch (cause) {
      if (mounted.current) {
        setQrAccountId(null);
        fail(cause);
      }
    } finally {
      if (mounted.current) setBusyId(null);
    }
  }

  async function cancelFlow(id: string): Promise<void> {
    try {
      const snapshot = await mateLogin.cancel(id);
      if (!mounted.current) return;
      setStateMap((prev) => ({ ...prev, [id]: snapshot }));
    } catch {
      // Closing the dialog must never surface an error for a best-effort cancel.
    }
  }

  function closeQr(): void {
    const id = qrAccountId;
    setQrAccountId(null);
    if (id === null) return;
    void cancelFlow(id);
    // Closing the QR dialog for a not-yet-named "add" row abandons that add:
    // drop the placeholder row instead of leaving it in the list.
    cleanupPendingAdd(id);
  }

  const busy = busyId !== null;
  const qrAccount = qrAccountId === null ? null : accounts.find((a) => a.id === qrAccountId) ?? null;
  const qrState = qrAccountId === null ? null : stateMap[qrAccountId] ?? null;

  return (
    <section
      aria-label={t("biz.mate.title")}
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto px-6 py-4"
    >
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-0">
          <h2 className="text-[15px] font-semibold text-slate-100">{t("biz.mate.title")}</h2>
        </div>
        <div className="flex-1" />
        <span className="mono text-[11px] text-slate-600">{accounts.length}</span>
        <Button
          size="sm"
          variant="secondary"
          disabled={busy}
          onClick={() => void refresh()}
          leftIcon={<RefreshCw size={12} />}
        >
          {t("biz.mate.refresh")}
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
          {t("biz.mate.add")}
        </Button>
      </div>

      {listError && (
        <div role="alert" className="mt-3 text-[12px] text-amber-300">
          {t("biz.mate.loadFailed", { error: listError })}
        </div>
      )}
      {error && (
        <div role="alert" className="mt-3 text-[12px] text-red-300">
          {t("biz.mate.opFailed", { detail: error })}
        </div>
      )}
      {notice && (
        <div role="status" className="mt-3 text-[12px] text-emerald-300">
          {notice}
        </div>
      )}

      <div className="mt-3">
        {loading ? (
          <p className="text-[13px] text-muted-foreground">{t("biz.mate.loading")}</p>
        ) : accounts.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">{t("biz.mate.empty")}</p>
        ) : (
          <ul data-testid="mate-list" className="flex flex-col gap-2">
            {accounts.map((row) => {
              // An in-memory `idle` snapshot only means "no flow running"; the
              // persisted `loginAt` is the source of truth for logged-in state.
              const snapshot = stateMap[row.id];
              const stage: MateLoginStage =
                snapshot != null && snapshot.stage !== "idle"
                  ? snapshot.stage
                  : row.loginAt != null
                    ? "success"
                    : "idle";
              const loggedIn = row.loginAt != null && stage !== "expired" && stage !== "error";
              return (
                <li
                  key={row.id}
                  data-testid={`mate-row-${row.id}`}
                  className="flex flex-wrap items-center gap-2 rounded-xl px-3 py-2.5"
                  style={{
                    background: "rgba(255,255,255,0.03)",
                    boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.06)",
                  }}
                >
                  <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-slate-100">
                    {row.label}
                  </span>

                  <span data-testid={`mate-status-${row.id}`}>
                    <Pill kind={STAGE_PILL[stage]} dot>
                      {t(STAGE_KEY[stage])}
                    </Pill>
                  </span>

                  <span className="min-w-0 max-w-[240px] truncate text-[11px] text-slate-500">
                    {loggedIn && row.userName != null
                      ? t("biz.mate.identity", {
                          name: row.userName,
                          id: row.platformUserId ?? "—",
                        })
                      : t("biz.mate.notLoggedIn")}
                  </span>

                  <div className="flex items-center gap-1.5">
                    <Button
                      size="sm"
                      variant="accent"
                      disabled={busy}
                      onClick={() => void onStartLogin(row)}
                      leftIcon={
                        busyId === row.id ? <Loader2 size={10} className="animate-spin" /> : undefined
                      }
                    >
                      {loggedIn ? t("biz.mate.relogin") : t("biz.mate.login")}
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy}
                      title={t("biz.mate.rename")}
                      aria-label={t("biz.mate.rename")}
                      onClick={() => {
                        setRenameTarget(row);
                        setRenameLabel(row.label);
                      }}
                      leftIcon={<Pencil size={12} />}
                    >
                      {t("biz.mate.rename")}
                    </Button>
                    <Button
                      size="icon"
                      variant="danger"
                      disabled={busy}
                      title={t("biz.mate.delete")}
                      aria-label={t("biz.mate.delete")}
                      onClick={() => void onDelete(row)}
                      leftIcon={
                        busyId === row.id ? (
                          <Loader2 size={12} className="animate-spin" />
                        ) : (
                          <Trash2 size={12} />
                        )
                      }
                    />
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <Modal
        open={renameTarget !== null}
        onClose={() => {
          if (busyId !== null) return;
          setRenameTarget(null);
        }}
        title={t("biz.mate.renameTitle", { label: renameTarget?.label ?? "" })}
        subtitle={t("biz.mate.renameBody")}
        width={460}
        footer={
          <>
            <Button
              size="sm"
              variant="secondary"
              disabled={busy}
              onClick={() => setRenameTarget(null)}
            >
              {t("common.cancel")}
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={!renameLabel.trim() || busy}
              onClick={() => void onRename()}
            >
              {t("biz.mate.renameConfirm")}
            </Button>
          </>
        }
      >
        <div className="px-5 py-4">
          <label className="flex flex-col gap-1.5 text-[12px] text-slate-400">
            {t("biz.mate.labelField")}
            <input
              data-autofocus
              value={renameLabel}
              onChange={(event) => setRenameLabel(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void onRename();
              }}
              placeholder={t("biz.mate.labelPlaceholder")}
              aria-label={t("biz.mate.labelField")}
              className="h-8 rounded-md bg-white/[0.04] px-2.5 text-[12px] text-slate-200 outline-none"
              style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
            />
          </label>
        </div>
      </Modal>

      <Modal
        open={qrAccountId !== null}
        onClose={closeQr}
        title={
          qrAccount !== null && qrAccount.loginAt == null
            ? t("biz.mate.qrTitleNew")
            : t("biz.mate.qrTitle", { label: qrAccount?.label ?? "" })
        }
        subtitle={qrState !== null ? t(STAGE_KEY[qrState.stage]) : t("biz.mate.qrPreparing")}
        width={400}
        footer={
          <>
            {(qrState?.stage === "expired" || qrState?.stage === "error") && (
              <Button
                size="sm"
                variant="primary"
                disabled={busy || qrAccount === null}
                onClick={() => qrAccount !== null && void onStartLogin(qrAccount)}
                leftIcon={<RefreshCw size={12} />}
              >
                {t("biz.mate.qrRefresh")}
              </Button>
            )}
            <Button size="sm" variant="secondary" onClick={closeQr}>
              {t("biz.mate.qrClose")}
            </Button>
          </>
        }
      >
        <div className="flex flex-col items-center gap-3 px-5 py-4">
          {qrState?.qrImageDataUrl != null && qrState.qrImageDataUrl !== "" ? (
            <img
              src={qrState.qrImageDataUrl}
              alt={t("biz.mate.qrAlt")}
              className="h-56 w-56 rounded-md"
              style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.1)" }}
            />
          ) : (
            <div
              className="flex h-56 w-56 items-center justify-center rounded-md"
              style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
            >
              <Loader2 className="h-6 w-6 animate-spin text-slate-500" />
            </div>
          )}
          {qrAccount !== null && qrAccount.loginAt == null && (
            <div className="text-[12px] text-slate-500">{t("biz.mate.qrWillName")}</div>
          )}
          {qrState?.user != null && (
            <div className="text-[12px] text-slate-400">
              {t("biz.mate.qrScanned", {
                name: qrState.user.userName,
                id: qrState.user.userId,
              })}
            </div>
          )}
          {qrState?.errorMessage != null && qrState.errorMessage !== "" && (
            <div role="alert" className="text-[12px] text-red-300">
              {qrState.errorMessage}
            </div>
          )}
          {qrState?.stage === "success" && (
            <div role="status" className="text-[12px] text-emerald-300">
              {t("biz.mate.qrSuccess")}
            </div>
          )}
        </div>
      </Modal>
    </section>
  );
}
