import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import { liveLaunch, onLiveLaunchStateChanged } from "../../lib/liveLaunch";
import { mateLogin } from "../../lib/mateLogin";
import type {
  LiveLaunchStateChanged,
  MateAccount,
  PrerequisitesReport,
  StreamCredentials,
  StreamingState,
} from "../../types";
import { Button } from "../atoms/Button";
import { Pill, type PillKind } from "../atoms/Pill";
import { confirm } from "../atoms/Modal";

function statusPill(status: StreamingState["status"]): PillKind {
  switch (status) {
    case "streaming": return "running";
    case "starting":
    case "stopping": return "pending";
    case "error": return "error";
    default: return "idle";
  }
}

function errText(e: unknown): string {
  return typeof e === "string" ? e : (e as Error).message ?? String(e);
}

/**
 * 伴侣开播块（「直播 › 开播准备」）。
 *
 * 对齐 jieger `pages/live-launch` 的伴侣开播流程：**选择直播伴侣账号**
 * （数据源 `mate_accounts_list`）→ 获取推流码（`live_launch_mate_credentials`）
 * → 本地视频循环推流（`live_launch_mate_stream_start`）/ 停止关播
 * （`live_launch_mate_stream_stop`）。全程**不启浏览器**（mate 无 profile）。
 *
 * 推流启停需二次确认；状态取自命令返回值与 `live-launch-state-changed`
 * 事件（按所选账号的 `subjectKind === "mate"` + `profileId` 过滤）。
 *
 * 不再手输 profile-id / 中控页 URL——伴侣开播走 liveMate 直连取流。
 * 慧播开播是独立组件（`HuiboLivePage`），本页只负责伴侣块。
 *
 * **命名空间**：伴侣账号（`mate_accounts`，键 = `mateAccountId`）与浏览器环境
 * （`profile_id`）是**两套独立命名空间**，当前没有绑定关系。本块作为「开播准备」
 * 账号维度下的**独立子区块**保留，不消费左侧选中的 profile。
 */
export function LiveLaunchPage(): JSX.Element {
  const t = useT();
  const [accounts, setAccounts] = useState<MateAccount[]>([]);
  const [accountId, setAccountId] = useState("");
  const [videoPath, setVideoPath] = useState("");
  const [status, setStatus] = useState<StreamingState | null>(null);
  const [prereq, setPrereq] = useState<PrerequisitesReport | null>(null);
  const [creds, setCreds] = useState<StreamCredentials | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const selected = accounts.find((a) => a.id === accountId) ?? null;
  const loggedIn = selected !== null && selected.loginAt != null;
  const ready = accountId !== "" && loggedIn;
  const busyStatus = status?.status === "starting" || status?.status === "stopping";
  const streaming = status?.status === "streaming";

  const refreshAccounts = useCallback(async (): Promise<void> => {
    try {
      const list = await mateLogin.list();
      if (!mounted.current) return;
      setAccounts(list);
      setAccountId((prev) =>
        prev !== "" && list.some((a) => a.id === prev) ? prev : list[0]?.id ?? "",
      );
      setError(null);
    } catch (e) {
      if (mounted.current) setError(errText(e));
    }
  }, []);

  // Initial account list load.
  useEffect(() => {
    void refreshAccounts();
  }, [refreshAccounts]);

  // Keep the latest account id reachable from the (stable) event listener.
  const accountIdRef = useRef(accountId);
  useEffect(() => {
    accountIdRef.current = accountId;
    // Switching accounts clears the previous account's transient view state.
    setStatus(null);
    setCreds(null);
    setError(null);
  }, [accountId]);

  // Backend push: only accept mate-bucket events for the selected account.
  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onLiveLaunchStateChanged((s: LiveLaunchStateChanged) => {
      if (!active) return;
      if (s.subjectKind !== "mate" || s.profileId !== accountIdRef.current) return;
      setStatus((prev) => ({
        profileId: s.profileId,
        subjectKind: "mate",
        status: s.status,
        mode: s.mode ?? null,
        target: s.target ?? null,
        pid: s.pid ?? null,
        stderrTail: prev?.stderrTail ?? [],
        exitCode: s.exitCode ?? null,
        error: s.error ?? null,
        placeholderCredentials: s.placeholderCredentials,
        liveStreamId: s.liveStreamId ?? prev?.liveStreamId ?? null,
        startedAt: prev?.startedAt ?? null,
      }));
    }).then((fn) => {
      if (active) unlisten = fn;
      else fn();
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  async function run(fn: () => Promise<void>): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errText(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }

  async function confirmAndRun(message: string, fn: () => Promise<void>): Promise<void> {
    const ok = await confirm({ title: message, confirmLabel: t("common.confirm") });
    if (!ok) return;
    await run(fn);
  }

  const canStart = ready && videoPath.trim() !== "" && creds !== null && !busy && !busyStatus && !streaming;
  const canStop = ready && (streaming || busyStatus) && !busy;

  return (
    <section aria-label={t("biz.live.title")} className="flex flex-col gap-4 p-6 max-w-2xl">
      {/* 独立于浏览器环境：mate 账号（mate_accounts）与 profile 是两套命名空间。 */}
      <p className="text-[12px] text-amber-300/90">{t("live.prepare.mateNote")}</p>

      <div className="flex items-end gap-2">
        <label className="flex flex-col gap-1 text-[12px] text-slate-400 flex-1 min-w-0">
          {t("biz.live.mateAccount")}
          <select
            data-testid="live-mate-account"
            aria-label={t("biz.live.mateAccount")}
            value={accountId}
            disabled={busy || accounts.length === 0}
            onChange={(e) => setAccountId(e.target.value)}
            className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
            style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
          >
            {accounts.length === 0 ? (
              <option value="">{t("biz.live.mateAccountEmpty")}</option>
            ) : (
              accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.label}
                </option>
              ))
            )}
          </select>
        </label>
        {selected !== null && (
          <span data-testid="live-mate-login-state">
            <Pill kind={loggedIn ? "running" : "idle"} dot>
              {loggedIn ? t("biz.live.loggedIn") : t("biz.live.notLoggedIn")}
            </Pill>
          </span>
        )}
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          onClick={() => void run(refreshAccounts)}
        >
          {t("biz.live.refreshAccounts")}
        </Button>
      </div>

      {accounts.length === 0 && (
        <div role="alert" className="text-[12px] text-amber-300">
          {t("biz.live.mateAccountEmpty")}
        </div>
      )}
      {selected !== null && !loggedIn && (
        <div role="alert" className="text-[12px] text-amber-300">
          {t("biz.live.mateAccountNotLoggedIn")}
        </div>
      )}

      <div className="flex items-center gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          onClick={() => void run(async () => setPrereq(await liveLaunch.prerequisites()))}
        >
          {t("biz.live.checkPrereq")}
        </Button>
        {prereq !== null && (
          <span role="status" className="text-[12px] text-slate-300">
            {prereq.available
              ? t("biz.live.ffmpegOk", { path: prereq.ffmpegPath ?? "" })
              : t("biz.live.ffmpegMissing", { detail: prereq.error ?? "" })}
          </span>
        )}
      </div>

      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.live.videoPath")}
        <input
          value={videoPath}
          disabled={busy || streaming}
          onChange={(e) => setVideoPath(e.target.value)}
          placeholder={t("biz.live.videoPathPh")}
          className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
        />
      </label>

      <div className="flex items-center gap-2 flex-wrap">
        <Button
          variant="secondary"
          size="sm"
          disabled={!ready || busy || busyStatus || streaming}
          onClick={() => void run(async () => setCreds(await liveLaunch.mateCredentials(accountId)))}
        >
          {t("biz.live.fetchCreds")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          disabled={!canStart}
          onClick={() => void confirmAndRun(
            t("biz.live.streamStartConfirm"),
            async () => setStatus(await liveLaunch.mateStreamStart(accountId, videoPath)),
          )}
        >
          {t("biz.live.streamStart")}
        </Button>
        <Button
          variant="danger"
          size="sm"
          disabled={!canStop}
          onClick={() => void confirmAndRun(
            t("biz.live.streamStopConfirm"),
            async () => setStatus(await liveLaunch.mateStreamStop(accountId)),
          )}
        >
          {t("biz.live.streamStop")}
        </Button>
      </div>

      {status !== null && (
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-[12px] text-slate-400">{t("biz.live.status")}</span>
          <Pill kind={statusPill(status.status)}>{status.status}</Pill>
          {status.target != null && status.target !== "" && (
            <span className="text-[12px] text-slate-400">{status.target}</span>
          )}
          {status.liveStreamId != null && status.liveStreamId !== "" && (
            <span className="text-[12px] text-slate-500">
              {t("biz.live.liveStreamId")}: {status.liveStreamId}
            </span>
          )}
        </div>
      )}
      {creds !== null && (
        <div role="status" className="text-[12px] text-slate-300">
          {creds.placeholder
            ? t("biz.live.credsPlaceholder")
            : t("biz.live.credsOk", { server: creds.rtmpServer })}
        </div>
      )}
      {error !== null && (
        <div role="alert" className="text-[12px] text-red-300">
          {t("biz.live.opFailed", { detail: error })}
        </div>
      )}
    </section>
  );
}
