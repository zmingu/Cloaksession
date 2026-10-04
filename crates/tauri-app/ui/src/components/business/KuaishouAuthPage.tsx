import { useEffect, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import { kuaishouAuth, onKuaishouAuthPhase } from "../../lib/kuaishouAuth";
import type { EnsureAuthResult, KuaishouAuthPhase } from "../../types";
import { Button } from "../atoms/Button";
import { Pill } from "../atoms/Pill";

function phaseKind(phase: KuaishouAuthPhase | null): "running" | "pending" | "idle" {
  if (phase === null) return "idle";
  return "running";
}

/** 小店扫码连接页 (R1/R5): 只读状态 + ensure 结果, 订阅 kuaishou-auth-phase. */
export function KuaishouAuthPage(): JSX.Element {
  const t = useT();
  const [profileId, setProfileId] = useState("");
  const [targetId, setTargetId] = useState("");
  const [phase, setPhase] = useState<KuaishouAuthPhase | null>(null);
  const [connected, setConnected] = useState<boolean | null>(null);
  const [ensure, setEnsure] = useState<EnsureAuthResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onKuaishouAuthPhase((e) => {
      if (active) setPhase(e.phase);
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  const ready = profileId.trim() !== "" && targetId.trim() !== "";

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

  return (
    <section aria-label={t("biz.auth.title")} className="flex flex-col gap-4 p-6 max-w-2xl">
      <div>
        <h2 className="text-[15px] font-bold text-slate-100">{t("biz.auth.title")}</h2>
        <p className="text-[12px] text-slate-500 mt-1">{t("biz.auth.hint")}</p>
      </div>

      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.auth.profileId")}
        <input
          value={profileId}
          onChange={(e) => setProfileId(e.target.value)}
          placeholder="profile-id"
          className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
        />
      </label>
      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.auth.targetId")}
        <input
          value={targetId}
          onChange={(e) => setTargetId(e.target.value)}
          placeholder="https://login.kwaixiaodian.com/"
          className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
        />
      </label>

      <div className="flex items-center gap-2 flex-wrap">
        <Button
          variant="secondary"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void run(async () => setConnected(await kuaishouAuth.connect(profileId, targetId)))}
        >
          {t("biz.auth.connect")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void run(async () => { await kuaishouAuth.login(profileId, targetId); })}
        >
          {t("biz.auth.login")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          disabled={!ready || busy}
          onClick={() => void run(async () => setEnsure(await kuaishouAuth.ensureAuth(profileId, targetId)))}
        >
          {t("biz.auth.ensure")}
        </Button>
        <Pill kind={phaseKind(phase)}>
          {phase ?? t("biz.auth.phaseIdle")}
        </Pill>
      </div>

      {connected !== null && (
        <div role="status" className="text-[12px] text-slate-300">
          {connected ? t("biz.auth.alreadyAuthed") : t("biz.auth.scanRequired")}
        </div>
      )}
      {ensure !== null && (
        <div role="status" className="text-[12px] text-slate-300">
          {ensure.ok
            ? ensure.scanned
              ? t("biz.auth.ensureScanned")
              : t("biz.auth.ensureReused")
            : t("biz.auth.ensureFailed", { detail: ensure.error ?? "" })}
        </div>
      )}
      {error !== null && (
        <div role="alert" className="text-[12px] text-red-300">
          {t("biz.auth.opFailed", { detail: error })}
        </div>
      )}
    </section>
  );
}
