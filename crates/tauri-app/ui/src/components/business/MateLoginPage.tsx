import { useEffect, useRef, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import { mateLogin, onMateLoginStateChanged } from "../../lib/mateLogin";
import type { MateLoginState } from "../../types";
import { MATE_LOGIN_TERMINAL_STAGES } from "../../types";
import { Button } from "../atoms/Button";
import { Pill, type PillKind } from "../atoms/Pill";

const POLL_MS = 2000;

function stagePill(stage: MateLoginState["stage"]): PillKind {
  switch (stage) {
    case "success": return "running";
    case "awaiting-scan":
    case "awaiting-confirm":
    case "starting":
    case "receiving": return "pending";
    case "expired":
    case "cancelled":
    case "error": return "error";
    default: return "idle";
  }
}

/** 伴侣扫码登录页 (R2/R5): 二维码 + 阶段机, 轮询到终态, 订阅 state-changed. */
export function MateLoginPage(): JSX.Element {
  const t = useT();
  const [accountId, setAccountId] = useState("");
  const [state, setState] = useState<MateLoginState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pollTimer = useRef<number | null>(null);

  const stopPolling = (): void => {
    if (pollTimer.current !== null) {
      window.clearInterval(pollTimer.current);
      pollTimer.current = null;
    }
  };

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onMateLoginStateChanged((s) => {
      if (!active) return;
      setState((prev) => (prev !== null && prev.accountId !== s.accountId ? prev : s));
      if ((MATE_LOGIN_TERMINAL_STAGES as readonly string[]).includes(s.stage)) stopPolling();
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
      stopPolling();
    };
  }, []);

  useEffect(() => () => stopPolling(), []);

  const ready = accountId.trim() !== "";

  async function pollUntilTerminal(id: string): Promise<void> {
    stopPolling();
    pollTimer.current = window.setInterval(() => {
      void (async () => {
        try {
          const s = await mateLogin.state(id);
          setState(s);
          if ((MATE_LOGIN_TERMINAL_STAGES as readonly string[]).includes(s.stage)) stopPolling();
        } catch (e) {
          stopPolling();
          setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
        }
      })();
    }, POLL_MS);
  }

  async function onStart(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const s = await mateLogin.start(accountId);
      setState(s);
      if (!(MATE_LOGIN_TERMINAL_STAGES as readonly string[]).includes(s.stage)) {
        await pollUntilTerminal(accountId);
      }
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onCancel(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const s = await mateLogin.cancel(accountId);
      setState(s);
      stopPolling();
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label={t("biz.mate.title")} className="flex flex-col gap-4 p-6 max-w-2xl">
      <div>
        <h2 className="text-[15px] font-bold text-slate-100">{t("biz.mate.title")}</h2>
        <p className="text-[12px] text-slate-500 mt-1">{t("biz.mate.hint")}</p>
      </div>

      <label className="flex flex-col gap-1 text-[12px] text-slate-400">
        {t("biz.mate.accountId")}
        <input
          value={accountId}
          onChange={(e) => setAccountId(e.target.value)}
          placeholder="account-id"
          className="h-8 px-2.5 rounded-md bg-white/[0.04] text-slate-200 text-[12px] outline-none"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" }}
        />
      </label>

      <div className="flex items-center gap-2">
        <Button variant="primary" size="sm" disabled={!ready || busy} onClick={() => void onStart()}>
          {t("biz.mate.start")}
        </Button>
        <Button variant="secondary" size="sm" disabled={!ready || busy} onClick={() => void onCancel()}>
          {t("biz.mate.cancel")}
        </Button>
        {state !== null && <Pill kind={stagePill(state.stage)}>{state.stage}</Pill>}
      </div>

      {state?.qrImageDataUrl != null && state.qrImageDataUrl !== "" && (
        <img
          src={state.qrImageDataUrl}
          alt={t("biz.mate.qrAlt")}
          className="w-48 h-48 rounded-lg"
          style={{ boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.1)" }}
        />
      )}
      {state?.user != null && (
        <div role="status" className="text-[12px] text-slate-300">
          {t("biz.mate.loggedInAs", { name: state.user.userName })}
        </div>
      )}
      {state?.errorMessage != null && state.errorMessage !== "" && (
        <div role="alert" className="text-[12px] text-red-300">
          {t("biz.mate.opFailed", { detail: state.errorMessage })}
        </div>
      )}
      {error !== null && (
        <div role="alert" className="text-[12px] text-red-300">
          {t("biz.mate.opFailed", { detail: error })}
        </div>
      )}
    </section>
  );
}
