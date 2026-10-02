import { useEffect, useRef, useState, type JSX } from "react";
import { Button } from "../atoms/Button";
import { kuaishouSubject, canConfirm, subjectCopy, validationLabels, type KuaishouSubjectArchive, type KuaishouSubjectDetail, type KuaishouInitStepRecord } from "../../lib/kuaishouSubject";
import { useKuaishouIdentity } from "../../lib/KuaishouIdentityProvider";

const stateLabels = { pending: "待执行", running: "执行中", done: "已完成", failed: "未完成" };
const errorLabels: Record<string, string> = { "interrupted-needs-verification": "上次中断，需重新验证", "context-changed": "账号或会话已改变", "timed-out": "执行超时", "page-unsupported": "页面结构不支持", "page-crashed": "浏览器页面已崩溃并重建", "attachment-unavailable": "照片不可用", "ocr-unavailable": "中文 OCR 不可用", "ocr-failed": "识别失败", "validation-failed": "自动校验未通过", "persistence-unverified": "平台保存尚未验证" };
export function InitSteps({ steps }: { steps: KuaishouInitStepRecord[] }): JSX.Element {
  return <div aria-label="持久初始化状态" className="space-y-1">
    {(["subject", "slice"] as const).map(key => {
      const s = steps.find(item => item.step === key);
      return <p key={key} data-testid={`init-${key}`} className={s?.state === "done" ? "text-emerald-300" : "text-amber-200"}>
        {key === "subject" ? "主体采集" : "切片权限"}：{s ? stateLabels[s.state] : "尚未初始化"}
        {s && <> · 尝试 {s.attempts} 次{s.lastErrorCode && ` · ${errorLabels[s.lastErrorCode] ?? "需重试"}`}{s.nextRetryAt && ` · 下次重试 ${s.nextRetryAt}`}</>}
      </p>;
    })}
    <p className="text-slate-500">身份识别不等于初始化完成；采集完成不等于人工已核对。</p>
  </div>;
}

/** Lightweight state only. No document/attachment prefetch and no detect/write timer. */
export function InitSummary({ platformUserId, observation }: { platformUserId: string; observation: string | null }): JSX.Element {
  const [steps, setSteps] = useState<KuaishouInitStepRecord[]>([]);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    void kuaishouSubject.steps(platformUserId).then(value => { if (active) { setSteps(value); setFailed(false); } }, () => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [platformUserId, observation]);
  return failed ? <p className="text-amber-200">初始化状态读取失败，请在详情刷新</p> : <InitSteps steps={steps} />;
}

function SubjectPhoto({ attachmentKey, index }: { attachmentKey: string; index: number }): JSX.Element {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const request = useRef<{ key: string; promise: Promise<string> } | null>(null);
  useEffect(() => {
    let active = true;
    setSrc(null);
    setFailed(false);
    // StrictMode replays setup/cleanup on the same instance. Reuse the request,
    // not its old subscription; the replay's subscription must receive the result.
    // No global document cache: explicit refresh remounts this component via
    // photoVersion, and closing the panel releases this instance and its promise.
    if (!request.current || request.current.key !== attachmentKey) {
      request.current = { key: attachmentKey, promise: kuaishouSubject.attachment(attachmentKey) };
    }
    void request.current.promise.then(value => { if (active) setSrc(value); }, () => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [attachmentKey]);
  return <figure className="min-w-0 rounded-lg border border-white/10 p-2">
    {src && !failed ? <img className="w-full max-h-80 object-contain" src={src} alt={`主体证件照片 ${index + 1}`} onError={() => { setSrc(null); setFailed(true); }} /> : <p>{failed ? "照片读取失败，请刷新档案" : "照片加载中…"}</p>}
    <figcaption className="text-slate-500 text-xs mt-1">证件照片 {index + 1} · 本地附件</figcaption>
  </figure>;
}

export function InitRetry({ profileId, disabled, onRetry }: { profileId: string; disabled: boolean; onRetry: () => Promise<void> }): JSX.Element {
  const identity = useKuaishouIdentity(profileId);
  return <div className="space-y-2 rounded-lg border border-amber-400/20 p-3">
    <p className="text-xs text-amber-200">执行会读取主体资料，并关闭切片全自动发布的四项主权限（平台写操作）。检测到有效账号后，后台自动执行未完成项并在失败后有限重试；此按钮用于手动补做，不重置已完成项；后端重新核对当前账号和会话。</p>
    <Button disabled={disabled || !identity.current || !identity.running} onClick={() => void onRetry()}>执行 / 补做初始化</Button>
    {(!identity.current || !identity.running) && <p className="text-xs text-slate-400">需运行中的环境与新鲜身份；重新检测仅为只读检测。</p>}
  </div>;
}

/** Parent must key by account ID. All async completions are scoped to this mounted panel. */
export function SubjectPanel({ platformUserId, profileId }: { platformUserId: string; profileId?: string }): JSX.Element {
  const [detail, setDetail] = useState<KuaishouSubjectDetail | null>(null);
  const [steps, setSteps] = useState<KuaishouInitStepRecord[]>([]);
  const [name, setName] = useState("");
  const [card, setCard] = useState("");
  const [busy, setBusy] = useState(true);
  const [message, setMessage] = useState("");
  const [photoVersion, setPhotoVersion] = useState(0);
  const [ocr, setOcr] = useState<{ available: boolean; message: string } | null>(null);
  const alive = useRef(false);
  const lock = useRef(false);
  const version = useRef(0);
  function apply(a: KuaishouSubjectArchive): void {
    setDetail(previous => previous ? { ...previous, archive: a } : previous);
    setName(a.realName); setCard(a.idCard);
  }
  async function load(): Promise<void> {
    const ticket = ++version.current;
    const [d, s] = await Promise.all([kuaishouSubject.detail(platformUserId), kuaishouSubject.steps(platformUserId)]);
    if (!alive.current || ticket !== version.current) return;
    setDetail(d); setSteps(s); setName(d?.archive.realName ?? ""); setCard(d?.archive.idCard ?? "");
    setPhotoVersion(value => value + 1); // Explicit refresh retries failed reads of unchanged immutable keys.
  }
  useEffect(() => {
    alive.current = true;
    void load().catch(() => { if (alive.current) setMessage("档案读取失败，请刷新重试。"); }).finally(() => { if (alive.current) setBusy(false); });
    void kuaishouSubject.availability().then(value => { if (alive.current) setOcr(value); }, () => { if (alive.current) setOcr({ available: false, message: "本地中文 OCR 状态不可用，请检查系统语言资源；不会自动下载或上传。" }); });
    return () => { alive.current = false; ++version.current; };
  }, [platformUserId]);
  async function action(kind: "save" | "confirm" | "ocr" | "refresh" | "retry"): Promise<void> {
    if (lock.current || busy) return;
    lock.current = true; setBusy(true); setMessage("");
    try {
      const a = detail?.archive;
      if (kind === "refresh") await load();
      else if (kind === "retry" && profileId) {
        await kuaishouSubject.retry(profileId);
        if (!alive.current) return;
        await load();
        if (alive.current) setMessage("补做请求已提交，请刷新查看持久状态；不代表执行完成。");
      } else if (a) {
        let next: KuaishouSubjectArchive;
        if (kind === "save") next = await kuaishouSubject.correct({ platformUserId, expectedRevision: a.revision, realName: name, idCard: card });
        else if (kind === "confirm") {
          // Dirty candidates must be saved and backend-validated before this button enables.
          if (name !== a.realName || card !== a.idCard || !canConfirm(a)) return;
          next = await kuaishouSubject.confirm({ platformUserId, expectedRevision: a.revision });
        } else if (kind === "ocr") next = await kuaishouSubject.reocr(platformUserId, a.revision);
        else return;
        if (!alive.current) return;
        apply(next);
        setMessage(kind === "save" ? "修正已保存并重新校验，请对照照片后确认。" : kind === "ocr" ? "重新识别已完成，仍需人工核对。" : "已核对");
      }
    } catch (error) {
      if (!alive.current) return;
      // Never render arbitrary backend exceptions containing document text.
      const conflict = /revision|版本|过期|冲突/i.test(String(error));
      setMessage(conflict ? "版本冲突：档案已被更新，正在刷新；请重新核对后操作。" : "操作未完成，未强制确认。请刷新并检查校验、照片或本地 OCR 资源后重试。");
      try { await load(); } catch { if (alive.current) setMessage("操作未完成，刷新也失败；请稍后重试，不要重复确认。"); }
    } finally { lock.current = false; if (alive.current) setBusy(false); }
  }
  const a = detail?.archive;
  const dirty = !!a && (name !== a.realName || card !== a.idCard);
  return <section aria-label="主体档案" data-testid="subject-panel" className="space-y-4 border-t border-white/10 pt-4 min-w-0">
    <h2 className="text-base font-semibold">主体档案与账号初始化</h2>
    <InitSteps steps={steps} />
    <Button disabled={busy} onClick={() => void action("refresh")}>刷新档案</Button>
    {busy && <p>处理中…</p>}
    {message && <p role="alert" className="text-amber-200">{message}</p>}
    {a ? <>
      <p className="text-slate-300">{detail.nickname ?? "未读出昵称"} · 快手ID：{a.platformUserId}</p>
      <p>{detail.profiles.length ? `历史关联环境：${detail.profiles.map(p => p.name).join("、")}` : "无关联环境 / 环境已删除"}</p>
      <p data-testid="subject-review" className={a.reviewStatus === "confirmed" ? "text-emerald-300" : "text-amber-200"}>{a.reviewStatus === "confirmed" ? "已核对" : "待核对（未核对）"} · 版本 {a.revision}</p>
      <p>来源：{{ "main-tab": "主体信息明文", "talent-tab-plaintext": "达人主体明文", ocr: "本地 OCR" }[a.source]} · 更新 {a.updatedAt}</p>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
        {a.attachments.map((attachment, index) => <SubjectPhoto key={`${platformUserId}:${attachment.key}:${photoVersion}`} attachmentKey={attachment.key} index={index} />)}
      </div>
      {!a.attachments.length && <p className="text-amber-200">尚无可用照片，不能确认。</p>}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <label>姓名<input aria-label="主体姓名" className="block w-full min-w-0 bg-white/5 border border-white/10 rounded p-2" value={name} disabled={busy} onChange={e => setName(e.target.value)} autoComplete="off" /></label>
        <label>身份证号<input aria-label="主体身份证号" className="block w-full min-w-0 bg-white/5 border border-white/10 rounded p-2" value={card} disabled={busy} onChange={e => setCard(e.target.value)} autoComplete="off" /></label>
      </div>
      <div aria-label="自动校验六项" className="grid grid-cols-1 sm:grid-cols-2 gap-1">
        {(Object.keys(validationLabels) as Array<keyof typeof validationLabels>).map(key => <p key={key} className={a.validation[key] === "failed" ? "text-red-300" : "text-slate-300"}>{validationLabels[key]}：{{ passed: "通过", failed: "失败", unavailable: "无法比对" }[a.validation[key]]}</p>)}
      </div>
      <p className="text-xs text-slate-400">自动校验不是法律身份真实性证明。请对照全部照片；无法比对不等于通过。编辑后须先保存修正，使用新版本的校验结果确认。</p>
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy || !dirty} onClick={() => void action("save")}>保存修正并校验</Button>
        <Button disabled={busy || dirty || !canConfirm(a) || a.reviewStatus === "confirmed"} onClick={() => void action("confirm")}>确认已核对</Button>
        <Button disabled={busy || !ocr?.available} onClick={() => void action("ocr")}>重新识别（本地 OCR）</Button>
        <Button disabled={busy || dirty} onClick={() => { void navigator.clipboard.writeText(subjectCopy(a)).then(() => { if (alive.current) setMessage(a.reviewStatus === "confirmed" ? "主体资料已复制" : "主体资料已复制，包含未核对标记"); }, () => { if (alive.current) setMessage("复制失败，请手动选择复制。"); }); }}>复制主体资料</Button>
      </div>
    </> : !busy && <p>暂无主体档案；身份已识别不代表资料已采集。</p>}
    <p className="text-xs text-slate-400">OCR：{ocr ? `${ocr.available ? "可用" : "不可用"} · ${ocr.message}` : "正在读取本地语言资源状态…"}</p>
    {profileId && <InitRetry profileId={profileId}
      disabled={busy || steps.some(s => s.state === "running") || (steps.length === 2 && steps.every(s => s.state === "done"))}
      onRetry={() => action("retry")} />}
  </section>;
}
