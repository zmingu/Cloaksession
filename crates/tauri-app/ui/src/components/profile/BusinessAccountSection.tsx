import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { Loader2 } from "lucide-react";
import {
  businessAccounts,
  type BusinessAccount,
  type BusinessAccountKind,
  type BusinessProfileState,
  type SaveBusinessAccountInput,
} from "../../lib/businessAccounts";
import { Button } from "../atoms/Button";
import { RegistrationIdentityHint } from "./KuaishouIdentity";

const KINDS: Array<{ value: BusinessAccountKind; label: string }> = [
  { value: "kuaishou-shop", label: "快手小店" },
  { value: "kuaishou-live", label: "快手直播" },
  { value: "kuaishou-mate", label: "快手直播伴侣" },
  { value: "kuaishou-sub", label: "快手子账号" },
  { value: "jinniu", label: "磁力金牛" },
];
const CONTROL = "w-full min-w-0 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-2 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

interface Snapshot {
  state: BusinessProfileState;
  accounts: BusinessAccount[];
}
interface Draft {
  kind: BusinessAccountKind;
  displayName: string;
  platformUserId: string;
}
function toDraft(account: BusinessAccount | null, scope: BusinessProfileState["scope"]): Draft {
  return {
    kind: account?.kind ?? (scope === "jinniu" ? "jinniu" : "kuaishou-shop"),
    displayName: account?.displayName ?? "",
    platformUserId: account?.platformUserId ?? "",
  };
}
function allowsKind(scope: BusinessProfileState["scope"], kind: BusinessAccountKind): boolean {
  return scope === null || (scope === "jinniu" ? kind === "jinniu" : kind !== "jinniu");
}
async function readSnapshot(profileId: string): Promise<Snapshot> {
  const [state, accounts] = await Promise.all([
    businessAccounts.profileState(profileId), businessAccounts.list(),
  ]);
  return { state, accounts };
}

interface Props {
  profileId: string;
  active: boolean;
}

/** Keep the draft and busy state across rail switches; never reuse them for another profile. */
export function BusinessAccountSection(props: Props): JSX.Element {
  return <AccountEditor key={props.profileId} {...props} />;
}

function AccountEditor({ profileId, active }: Props): JSX.Element {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [draft, setDraft] = useState<Draft>(() => toDraft(null, null));
  const [mode, setMode] = useState<"new" | "existing">("new");
  const [existingId, setExistingId] = useState("");
  const [busy, setBusy] = useState<"loading" | "saving" | "unbinding" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [confirmUnbind, setConfirmUnbind] = useState(false);
  const mounted = useRef(false);
  const started = useRef(false);
  const inFlight = useRef(false);
  const request = useRef(0);

  const applySnapshot = useCallback((next: Snapshot) => {
    setSnapshot(next);
    setDraft(toDraft(next.state.account, next.state.scope));
    setMode("new");
    setExistingId("");
    setConfirmUnbind(false);
  }, []);

  const reload = useCallback(async (): Promise<void> => {
    if (inFlight.current) return;
    inFlight.current = true;
    const ticket = ++request.current;
    setBusy("loading");
    setError(null);
    setMessage(null);
    try {
      const next = await readSnapshot(profileId);
      if (mounted.current && ticket === request.current) applySnapshot(next);
    } catch (cause) {
      if (mounted.current && ticket === request.current) {
        // Unknown state must never look like a confirmed, unbound profile.
        setSnapshot(null);
        setError(`无法加载账号登记：${cause instanceof Error ? cause.message : String(cause)}`);
      }
    } finally {
      if (mounted.current && ticket === request.current) {
        inFlight.current = false;
        setBusy(null);
      }
    }
  }, [profileId, applySnapshot]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      started.current = false;
      inFlight.current = false;
      ++request.current;
    };
  }, []);
  useEffect(() => {
    if (active && !started.current) {
      started.current = true;
      void reload();
    }
  }, [active, reload]);

  const account = snapshot?.state.account ?? null;
  const scope = snapshot?.state.scope ?? null;
  const available = snapshot?.accounts.filter((item) => item.profileId === null) ?? [];
  const selected = available.find((item) => item.id === existingId) ?? null;
  const target = account ?? (mode === "existing" ? selected : null);
  const kind = target?.kind ?? draft.kind;
  const canSave = !!snapshot && !busy && !confirmUnbind
    && !!draft.displayName.trim() && allowsKind(scope, kind)
    && (!!account || mode === "new" || !!selected);

  function editDraft(patch: Partial<Draft>): void {
    setDraft((current) => ({ ...current, ...patch }));
    setMessage(null);
    setError(null);
  }

  async function mutate(action: "save" | "unbind"): Promise<void> {
    if (inFlight.current || !snapshot) return;
    if (action === "save" && !canSave) return;
    if (action === "unbind" && (!account || !confirmUnbind)) return;
    // Capture all fields before awaiting anything. No autosave / parent refresh.
    const input: SaveBusinessAccountInput = {
      ...(target ? { id: target.id } : {}), profileId, kind,
      displayName: draft.displayName.trim(),
      platformUserId: draft.platformUserId.trim() || null,
    };
    const accountId = account?.id;
    inFlight.current = true;
    const ticket = ++request.current;
    const current = (): boolean => mounted.current && ticket === request.current;
    let committed = false;
    setBusy(action === "save" ? "saving" : "unbinding");
    setError(null);
    setMessage(null);
    try {
      if (action === "unbind" && accountId) await businessAccounts.unbind(accountId);
      else await businessAccounts.save(input);
      committed = true;
      if (!current()) return;
      // Do not invent scope after save/unbind. If refresh fails, block another create.
      setSnapshot(null);
      const next = await readSnapshot(profileId);
      if (!current()) return;
      applySnapshot(next);
      setMessage(action === "save"
        ? "账号登记已保存；这不代表已登录平台。"
        : "已解绑。登记记录已保留，未清除 Cookie；环境专用用途仍保留。");
    } catch (cause) {
      if (current()) {
        const detail = cause instanceof Error ? cause.message : String(cause);
        setError(committed
          ? `操作已完成，但最新状态读取失败，请重新加载，不要重复提交：${detail}`
          : `操作失败：${detail}`);
      }
    } finally {
      if (current()) {
        inFlight.current = false;
        setBusy(null);
      }
    }
  }

  return (
    <section hidden={!active} aria-label="业务账号登记" aria-busy={busy !== null} className="min-w-0 space-y-3 text-[12px] mt-4 border-t border-white/10 pt-4">
      <div className="space-y-1.5">
        <h3 className="font-medium text-slate-200">业务账号登记</h3>
        <p className="text-slate-400 leading-relaxed">
          账号信息未验证，仅手工登记类型、别名与平台用户 ID，不代表已登录或免登录，不共享 Cookie。
          每个 Profile 同时只能关联一个登记记录，不会自动覆盖、移动或合并账号。
        </p>
        <p className="text-slate-500">本分区需单独保存；切换分区保留草稿，关闭编辑页会丢弃未保存的登记修改。</p>
      </div>

      {busy && <p role="status" className="flex items-center gap-2 text-purple-300"><Loader2 size={14} className="animate-spin" />{busy === "loading" ? "正在加载账号登记…" : busy === "saving" ? "正在保存账号登记…" : "正在解绑账号…"}</p>}
      {error && <p role="alert" className="break-words text-red-300">{error}</p>}
      {message && <p role="status" className="text-emerald-300">{message}</p>}
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => void reload()} disabled={!!busy}>{error ? "重新加载 / 重试" : "重新加载状态"}</Button>
        <span className="text-[11px] text-slate-500">重新加载会丢弃未保存的登记修改。</span>
      </div>

      {snapshot && <>
        <div className="rounded-lg border border-white/10 p-3 space-y-1.5 text-slate-400">
          <p className="font-medium text-slate-200">{account ? "当前已关联登记记录" : "当前未关联登记记录"}</p>
          {scope === "jinniu" ? (
            <p>此 Profile 为磁力金牛专用环境，只能登记金牛账号，必须与快手环境独立；解绑后仍保留金牛专用标记。</p>
          ) : scope === "kuaishou" ? (
            <p>此 Profile 已用于快手业务，不能登记磁力金牛。金牛需要独立 Profile；解绑不会清除已有专用用途。</p>
          ) : (
            <p>磁力金牛必须使用独立 Profile。首次绑定后会保留该环境的业务用途，解绑不会重置用途。</p>
          )}
          <p>绑定、修改和解绑前必须停用 Profile；运行状态仅供提示，最终由后台校验。</p>
        </div>

        <RegistrationIdentityHint profileId={profileId} manualId={draft.platformUserId} kind={kind} />

        <form onSubmit={(event) => { event.preventDefault(); void mutate("save"); }}>
          <fieldset disabled={!!busy || confirmUnbind} className="min-w-0 space-y-3">
            {!account && <>
              <label className="block space-y-1.5 text-slate-400">
                <span>登记方式</span>
                <select className={CONTROL} value={mode} onChange={(event) => {
                  setMode(event.target.value as "new" | "existing");
                  setExistingId("");
                  setDraft(toDraft(null, scope));
                  setError(null);
                  setMessage(null);
                }}>
                  <option value="new">新建登记并绑定</option>
                  <option value="existing">绑定已有未关联记录</option>
                </select>
              </label>
              {mode === "existing" && <label className="block space-y-1.5 text-slate-400">
                <span>已有未关联记录</span>
                <select className={CONTROL} value={existingId} onChange={(event) => {
                  const next = available.find((item) => item.id === event.target.value) ?? null;
                  setExistingId(next?.id ?? "");
                  setDraft(toDraft(next, scope));
                  setError(null);
                  setMessage(null);
                }}>
                  <option value="">请选择未关联记录</option>
                  {available.filter((item) => scope !== "jinniu" || item.kind === "jinniu").map((item) => (
                    <option key={item.id} value={item.id} disabled={!allowsKind(scope, item.kind)}>
                      {item.displayName} · {KINDS.find((entry) => entry.value === item.kind)?.label} · {item.platformUserId ?? item.id}{!allowsKind(scope, item.kind) ? "（需要独立环境）" : ""}
                    </option>
                  ))}
                </select>
                <span className="block text-[11px]">包含删除环境后保留的登记；已关联其他 Profile 的记录不会出现在这里。</span>
              </label>}
            </>}
            <label className="block space-y-1.5 text-slate-400">
              <span>账号类型</span>
              <select className={CONTROL} value={kind} disabled={!!target || mode === "existing"} onChange={(event) => editDraft({ kind: event.target.value as BusinessAccountKind })}>
                {KINDS.filter((item) => scope !== "jinniu" || item.value === "jinniu").map((item) => (
                  <option key={item.value} value={item.value} disabled={!allowsKind(scope, item.value)}>{item.label}{!allowsKind(scope, item.value) ? "（需要独立环境）" : ""}</option>
                ))}
              </select>
              <span className="block text-[11px]">账号类型创建后不可修改；修改别名或 ID 不会验证平台身份。</span>
            </label>
            {!account && kind === "jinniu" && scope === null && <p className="text-amber-300">保存后此 Profile 将标记为磁力金牛专用，解绑仍保留标记。请勿使用已登录快手业务的环境。</p>}
            <label className="block space-y-1.5 text-slate-400">
              <span>账号别名（必填）</span>
              <input className={CONTROL} value={draft.displayName} required onChange={(event) => editDraft({ displayName: event.target.value })} />
            </label>
            <label className="block space-y-1.5 text-slate-400">
              <span>平台ID（可选，手填不代表已登录）</span>
              <input className={CONTROL} value={draft.platformUserId} onChange={(event) => editDraft({ platformUserId: event.target.value })} />
            </label>
            <div className="flex flex-wrap gap-2">
              <Button type="submit" variant="primary" disabled={!canSave}>{account ? "保存登记修改" : mode === "existing" ? "绑定已有登记" : "创建并绑定登记"}</Button>
              {account && <Button variant="danger" onClick={() => { setConfirmUnbind(true); setMessage(null); setError(null); }}>解绑登记</Button>}
            </div>
          </fieldset>
        </form>
        {confirmUnbind && account && <div role="group" aria-label="确认解绑登记" className="rounded-lg border border-amber-400/25 p-3 space-y-2">
          <p className="text-amber-200 break-words">确认解绑“{account.displayName}”？仅解除关联，保留登记记录，不清除 Cookie、不退出平台登录；环境专用用途（包括金牛标记）仍保留。</p>
          <div className="flex flex-wrap gap-2">
            <Button variant="danger" disabled={!!busy} onClick={() => void mutate("unbind")}>确认解绑</Button>
            <Button disabled={!!busy} onClick={() => setConfirmUnbind(false)}>取消</Button>
          </div>
        </div>}
      </>}
    </section>
  );
}
