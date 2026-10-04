import { useEffect, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import { liveRoomMonitor, onLiveRoomMonitorStateChanged } from "../../lib/liveRoomMonitor";
import type { LiveRoomMonitorState } from "../../types";
import { Button } from "../atoms/Button";
import { Pill, type PillKind } from "../atoms/Pill";

function statusPill(enabled: boolean, status: LiveRoomMonitorState["status"]): PillKind {
  if (!enabled) return "idle";
  switch (status) {
    case "live":
    case "triggered": return "running";
    case "checking":
    case "triggering": return "pending";
    case "error": return "error";
    default: return "idle";
  }
}

/** 直播间监控页 (R4/R5): 配置表单 + 状态 + 启停. */
export function LiveRoomMonitorPage(): JSX.Element {
  const t = useT();
  const [profileId, setProfileId] = useState("");
  const [liveRoomUrl, setLiveRoomUrl] = useState("");
  const [sceneId, setSceneId] = useState("");
  const [groupId, setGroupId] = useState("");
  const [productScriptId, setProductScriptId] = useState("");
  const [productScriptAccountId, setProductScriptAccountId] = useState("");
  const [autoExit, setAutoExit] = useState(false);
  const [state, setState] = useState<LiveRoomMonitorState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onLiveRoomMonitorStateChanged((s) => {
      if (active) setState(s);
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    void liveRoomMonitor.state().then(
      (s) => { if (active) setState(s); },
      () => {},
    );
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  const ready = profileId.trim() !== "" && liveRoomUrl.trim() !== "";
  const numOrNull = (v: string): number | null => {
    const s = v.trim();
    if (s === "") return null;
    const n = Number(s);
    return Number.isInteger(n) ? n : null;
  };

  async function onStart(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const s = await liveRoomMonitor.start(profileId, {
        liveRoomUrl: liveRoomUrl.trim(),
        sceneId: numOrNull(sceneId),
        groupId: groupId.trim() === "" ? null : groupId.trim(),
        productScriptId: numOrNull(productScriptId),
        productScriptAccountId: productScriptAccountId.trim() === "" ? null : productScriptAccountId.trim(),
        autoExitSubAccounts: autoExit,
      });
      setState(s);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onStop(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setState(await liveRoomMonitor.stop());
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onRefresh(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      setState(await liveRoomMonitor.state());
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  const inputCls =
    "h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none";

  return (
    <section aria-label={t("biz.monitor.title")} className="flex flex-col gap-4 p-6 max-w-2xl">
      <div>
        <h2 className="text-[15px] font-bold text-slate-100">{t("biz.monitor.title")}</h2>
        <p className="text-[12px] text-slate-500 mt-1">{t("biz.monitor.hint")}</p>
      </div>

      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.monitor.profileId")}
        <input value={profileId} onChange={(e) => setProfileId(e.target.value)} placeholder="profile-id" className={inputCls}
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }} />
      </label>
      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.monitor.liveRoomUrl")}
        <input value={liveRoomUrl} onChange={(e) => setLiveRoomUrl(e.target.value)} placeholder="https://live.kuaishou.com/…" className={inputCls}
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }} />
      </label>
      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1 text-[12px] text-slate-400">
          {t("biz.monitor.sceneId")}
          <input value={sceneId} onChange={(e) => setSceneId(e.target.value)} inputMode="numeric" placeholder={t("biz.monitor.optional")} className={inputCls}
            style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }} />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-400">
          {t("biz.monitor.groupId")}
          <input value={groupId} onChange={(e) => setGroupId(e.target.value)} placeholder={t("biz.monitor.optional")} className={inputCls}
            style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }} />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-400">
          {t("biz.monitor.productScriptId")}
          <input value={productScriptId} onChange={(e) => setProductScriptId(e.target.value)} inputMode="numeric" placeholder={t("biz.monitor.optional")} className={inputCls}
            style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }} />
        </label>
        <label className="flex flex-col gap-1 text-[12px] text-slate-400">
          {t("biz.monitor.productScriptAccountId")}
          <input value={productScriptAccountId} onChange={(e) => setProductScriptAccountId(e.target.value)} placeholder={t("biz.monitor.optional")} className={inputCls}
            style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }} />
        </label>
      </div>
      <label className="flex items-center gap-2 text-[12px] text-slate-300">
        <input type="checkbox" checked={autoExit} onChange={(e) => setAutoExit(e.target.checked)} />
        {t("biz.monitor.autoExit")}
      </label>

      <div className="flex items-center gap-2">
        <Button variant="primary" size="sm" disabled={!ready || busy} onClick={() => void onStart()}>
          {t("biz.monitor.start")}
        </Button>
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => void onStop()}>
          {t("biz.monitor.stop")}
        </Button>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => void onRefresh()}>
          {t("biz.monitor.refresh")}
        </Button>
        {state !== null && (
          <Pill kind={statusPill(state.enabled, state.status)}>
            {state.enabled ? state.status : t("biz.monitor.disabled")}
          </Pill>
        )}
      </div>

      {state !== null && (
        <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-[12px]">
          <dt className="text-slate-500">{t("biz.monitor.liveStatus")}</dt>
          <dd className="text-slate-300">{state.liveStatus}</dd>
          {state.lastCheckedAt != null && (
            <>
              <dt className="text-slate-500">{t("biz.monitor.lastChecked")}</dt>
              <dd className="text-slate-300">{new Date(state.lastCheckedAt).toLocaleString()}</dd>
            </>
          )}
          {state.error != null && state.error !== "" && (
            <>
              <dt className="text-slate-500">{t("biz.monitor.errorField")}</dt>
              <dd className="text-red-300">{state.error}</dd>
            </>
          )}
        </dl>
      )}
      {error !== null && (
        <div role="alert" className="text-[12px] text-red-300">
          {t("biz.monitor.opFailed", { detail: error })}
        </div>
      )}
    </section>
  );
}
