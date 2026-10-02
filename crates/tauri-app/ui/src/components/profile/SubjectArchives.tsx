import { useEffect, useState, type JSX } from "react";
import { Modal } from "../atoms";
import { Button } from "../atoms/Button";
import { kuaishouSubject, type KuaishouSubjectPage } from "../../lib/kuaishouSubject";
import { SubjectPanel } from "./KuaishouSubject";

/** Independent account archives: no fake Profile, browser launch or attachment prefetch. */
export function SubjectArchives({ search, profilesRevision }: { search: string; profilesRevision: string }): JSX.Element {
  const [offset, setOffset] = useState(0);
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<KuaishouSubjectPage | null>(null);
  const [busy, setBusy] = useState(true);
  const [failed, setFailed] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    setBusy(true); setFailed(false); setResult(null);
    const timer = window.setTimeout(() => {
      void kuaishouSubject.list(search, offset).then(value => {
        if (!active) return;
        // The final row can disappear from a page while the query is in flight.
        if (!value.items.length && offset > 0 && offset >= value.total) { setOffset(0); return; }
        setResult(value); setBusy(false);
      }, () => { if (active) { setFailed(true); setBusy(false); } });
    }, 250);
    return () => { active = false; window.clearTimeout(timer); };
  }, [search, offset, revision, profilesRevision]);
  return <section aria-label="独立账号档案" className="mt-5 space-y-3 border-t border-white/10 pt-4 text-sm text-slate-300">
    <div className="flex flex-wrap items-center gap-3">
      <h2 className="font-semibold">账号档案{result && ` · ${result.total}`}</h2>
      <Button size="sm" disabled={busy} onClick={() => setRevision(v => v + 1)}>刷新档案列表</Button>
    </div>
    <p className="text-xs text-slate-500">按姓名、快手ID、身份证号搜索；删除环境仍保留档案。环境筛选不影响档案结果。</p>
    {busy && <p>正在查询档案…</p>}
    {failed && <p role="alert">档案查询失败，请刷新重试。</p>}
    {result && !result.items.length && <p>没有匹配的账号档案。</p>}
    {result?.items.map(item => <div key={item.platformUserId} data-testid={`archive-${item.platformUserId}`} className="rounded-lg border border-white/10 p-3 space-y-1 break-words">
      <p>{item.realName} · 快手ID：{item.platformUserId} · {item.nickname ?? "未读出昵称"}</p>
      <p>{item.maskedIdCard} · {item.reviewStatus === "confirmed" ? "已核对" : "待核对（未核对）"}</p>
      <p>{item.profiles.length ? `历史关联环境：${item.profiles.map(p => p.name).join("、")}` : "无关联环境 / 环境已删除"}</p>
      <Button size="sm" onClick={() => setSelected(item.platformUserId)}>查看账号档案</Button>
    </div>)}
    {result && result.total > result.limit && <div className="flex gap-3 items-center">
      <Button size="sm" disabled={busy || offset === 0} onClick={() => setOffset(Math.max(0, offset - result.limit))}>上一页档案</Button>
      <span>{offset + 1}–{Math.min(offset + result.limit, result.total)} / {result.total}</span>
      <Button size="sm" disabled={busy || offset + result.limit >= result.total} onClick={() => setOffset(offset + result.limit)}>下一页档案</Button>
    </div>}
    <Modal open={selected !== null} onClose={() => { setSelected(null); setRevision(v => v + 1); }} title="账号档案" width={760}>
      {selected && <div className="p-5"><SubjectPanel key={selected} platformUserId={selected} /></div>}
    </Modal>
  </section>;
}
