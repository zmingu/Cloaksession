import { useEffect, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import { liveLaunch, onLiveLaunchStateChanged } from "../../lib/liveLaunch";
import type {
  LiveLaunchStateChanged,
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

/** 开播页 (R3/R5): 前置检查 → 取流 → 心跳启停 → 推流启停; 推流/开播需二次确认. */
export function LiveLaunchPage(): JSX.Element {
  const t = useT();
  const [profileId, setProfileId] = useState("");
  const [videoPath, setVideoPath] = useState("");
  const [controlUrl, setControlUrl] = useState("");
  const [status, setStatus] = useState<StreamingState | null>(null);
  const [prereq, setPrereq] = useState<PrerequisitesReport | null>(null);
  const [creds, setCreds] = useState<StreamCredentials | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onLiveLaunchStateChanged((s: LiveLaunchStateChanged) => {
      if (!active) return;
      setStatus((prev) => {
        if (prev !== null && prev.profileId !== s.profileId) return prev;
        return {
          profileId: s.profileId,
          status: s.status,
          mode: s.mode ?? null,
          target: s.target ?? null,
          pid: s.pid ?? null,
          stderrTail: prev?.stderrTail ?? [],
          exitCode: s.exitCode ?? null,
          error: s.error ?? null,
          placeholderCredentials: s.placeholderCredentials,
          startedAt: prev?.startedAt ?? null,
        };
      });
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  const ready = profileId.trim() !== "";
  const ctrl = controlUrl.trim() === "" ? undefined : controlUrl.trim();

  async function run(fn: () => Promise<void>): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  async function confirmAndRun(message: string, fn: () => Promise<void>): Promise<void> {
    const ok = await confirm({ title: message, confirmLabel: t("common.confirm") });    if (!ok) return;
    await run(fn);
  }

  return (
    <section aria-label={t("biz.live.title")} className="flex flex-col gap-4 p-6 max-w-2xl">
      <div>
        <h2 className="text-[15px] font-bold text-slate-100">{t("biz.live.title")}</h2>
        <p className="text-[12px] text-slate-500 mt-1">{t("biz.live.hint")}</p>
      </div>

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
        {t("biz.live.profileId")}
        <input
          value={profileId}
          onChange={(e) => setProfileId(e.target.value)}
          placeholder="profile-id"
          className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
        />
      </label>
      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.live.controlUrl")}
        <input
          value={controlUrl}
          onChange={(e) => setControlUrl(e.target.value)}
          placeholder={t("biz.live.controlUrlPh")}
          className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
        />
      </label>
      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.live.videoPath")}
        <input
          value={videoPath}
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
          disabled={!ready || busy}
          onClick={() => void run(async () => setStatus(await liveLaunch.status(profileId)))}
        >
          {t("biz.live.refreshStatus")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void run(async () => setCreds(await liveLaunch.credentials(profileId, ctrl)))}
        >
          {t("biz.live.fetchCreds")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void run(async () => setStatus(await liveLaunch.heartbeatStart(profileId, ctrl)))}
        >
          {t("biz.live.hbStart")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void run(async () => setStatus(await liveLaunch.heartbeatStop(profileId)))}
        >
          {t("biz.live.hbStop")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          disabled={!ready || busy || videoPath.trim() === ""}
          onClick={() => void confirmAndRun(
            t("biz.live.streamStartConfirm"),
            async () => setStatus(await liveLaunch.streamStart(profileId, videoPath, ctrl)),
          )}
        >
          {t("biz.live.streamStart")}
        </Button>
        <Button
          variant="danger"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void confirmAndRun(
            t("biz.live.streamStopConfirm"),
            async () => setStatus(await liveLaunch.streamStop(profileId)),
          )}
        >
          {t("biz.live.streamStop")}
        </Button>
      </div>

      {status !== null && (
        <div className="flex items-center gap-2">
          <Pill kind={statusPill(status.status)}>{status.status}</Pill>
          {status.target != null && status.target !== "" && (
            <span className="text-[12px] text-slate-400">{status.target}</span>
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
