import { useEffect, useRef, useState, type JSX } from "react";
import { Copy, RefreshCw } from "lucide-react";
import type { KuaishouIdentitySnapshot } from "../../lib/kuaishouIdentity";
import { useKuaishouIdentities, useKuaishouIdentity } from "../../lib/KuaishouIdentityProvider";
import { Modal } from "../atoms";
import { Button } from "../atoms/Button";
import { profiles } from "../../lib/ipc";

function AvatarImage({ avatarKey, size }: { avatarKey: string | null; size: number }): JSX.Element {
  const { avatar } = useKuaishouIdentities();
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    if (avatarKey) void avatar(avatarKey).then((value) => { if (active) setSrc(value); });
    return () => { active = false; };
  }, [avatarKey, avatar]);
  return src ? <img src={src} alt="快手平台头像" width={size} height={size}
    className="rounded-full shrink-0 object-cover" style={{ width: size, height: size }}
    onError={() => setSrc(null)} />
    : <span aria-label="平台头像不可用" className="rounded-full shrink-0 inline-flex items-center justify-center bg-orange-400/10 text-orange-200 text-[11px]"
      style={{ width: size, height: size }}>快</span>;
}

function IdentityAvatar({ snapshot, size = 26 }: { snapshot: KuaishouIdentitySnapshot | null; size?: number }): JSX.Element | null {
  if (!snapshot?.platformUserId) return null;
  // Remount before paint when either identity or key changes; never show a previous ID's photo.
  return <AvatarImage key={JSON.stringify([snapshot.profileId, snapshot.platformUserId, snapshot.avatarKey])} avatarKey={snapshot.avatarKey} size={size} />;
}

function DetectButton({ profileId }: { profileId: string }): JSX.Element {
  const identity = useKuaishouIdentity(profileId);
  return <Button size="sm" onClick={() => void identity.detect()} disabled={!identity.running || identity.pending}
    title={!identity.running ? "浏览器已关闭或正在关闭；运行状态最终由后台判定" : "请后台重新检测当前小店页面身份"}
    leftIcon={<RefreshCw size={11} className={identity.pending ? "animate-spin" : undefined} />}>
    {identity.pending ? "检测中…" : "重新检测"}
  </Button>;
}

function ShopLoginButton({ profileId }: { profileId: string }): JSX.Element {
  const { closingIds } = useKuaishouIdentities();
  const busy = useRef(false);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  async function open(): Promise<void> {
    if (busy.current || closingIds?.has(profileId)) return;
    busy.current = true;
    setPending(true);
    setMessage("");
    try {
      await profiles.launchKuaishou(profileId);
      setMessage("已打开小店登录页，请在浏览器中扫码；此操作不代表已登录。");
    } catch (error) {
      setMessage(`小店扫码入口未完成：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      busy.current = false;
      setPending(false);
    }
  }
  return <div className="space-y-1">
    <Button size="sm" disabled={pending || closingIds?.has(profileId)} onClick={() => void open()}
      title="启动此环境并新开小店扫码页；不改起始页或登录数据，金牛环境不可用">
      {pending ? "正在打开小店…" : "快手小店扫码"}
    </Button>
    {message && <p role="status" className="text-amber-200 break-words">{message}</p>}
  </div>;
}

export function KuaishouIdentitySummary({ profileId }: { profileId: string }): JSX.Element {
  const identity = useKuaishouIdentity(profileId);
  return <div data-testid={`kuaishou-summary-${profileId}`} data-status={identity.status}
    className="min-w-0 space-y-1.5 rounded-lg border border-white/[0.06] p-2 text-[11px]"
    onClick={(event) => event.stopPropagation()}>
    <div className="flex items-center gap-2 min-w-0">
      <IdentityAvatar snapshot={identity.snapshot} />
      <div className="min-w-0 flex-1">
        <div className={identity.current ? "text-emerald-300" : identity.status === "conflict" || identity.error ? "text-amber-300" : "text-slate-400"}>
          快手 · {identity.label}{identity.history && " · 上次识别"}
        </div>
        {identity.snapshot?.platformUserId && <div className="text-slate-300 truncate" title={`${identity.snapshot.nickname ?? ""} · ${identity.snapshot.platformUserId}`}>
          {identity.snapshot.nickname && <span>{identity.snapshot.nickname} · </span>}
          快手ID：{identity.snapshot.platformUserId}
        </div>}
      </div>
    </div>
    <div className="flex flex-wrap gap-1.5">
      <Button size="sm" onClick={identity.openDetails}>快手详情</Button>
      <DetectButton profileId={profileId} />
    </div>
    <ShopLoginButton profileId={profileId} />
  </div>;
}

/** This reads the global snapshot only. It never invokes detect on a timer. */
export function KuaishouIdentityToolbar(): JSX.Element {
  const { refresh, loading, listError } = useKuaishouIdentities();
  return <div className="px-6 pb-2 text-[11px] text-slate-500 space-y-1">
    <div className="flex flex-wrap gap-2 items-center">
      <Button size="sm" disabled={loading} onClick={() => void refresh()}>刷新检测状态</Button>
      <span>读取 Rust 后台结果 · 不代表已登录或初始化完成</span>
    </div>
    {listError && <p role="alert" className="text-amber-300 break-words">{listError}；保留上次识别信息，可重试读取。</p>}
  </div>;
}

export function RegistrationIdentityHint({ profileId, manualId, kind }: { profileId: string; manualId: string; kind: string }): JSX.Element | null {
  const identity = useKuaishouIdentity(profileId);
  if (!kind.startsWith("kuaishou") || !identity.snapshot?.platformUserId) return null;
  const conflict = !!manualId.trim() && manualId.trim() !== identity.snapshot.platformUserId;
  return <div className="rounded-lg border border-white/10 p-2 space-y-1 text-slate-400">
    <p>{identity.history ? "上次识别" : "本轮识别"}快手ID：{identity.snapshot.platformUserId} · {identity.label}</p>
    {conflict ? <p className="text-amber-300">登记冲突：手填平台ID与识别结果不同；不会覆盖人工ID或解除绑定。</p>
      : identity.status === "conflict" && <p className="text-amber-300">登记冲突：后台报告身份登记冲突，请核对登记；不会覆盖人工ID或解除绑定。</p>}
    <p>识别结果仅展示，不会填入或保存到人工登记。</p>
    <Button size="sm" onClick={identity.openDetails}>快手详情</Button>
  </div>;
}

export function KuaishouIdentityDialog(): JSX.Element | null {
  const { selectedId } = useKuaishouIdentities();
  return selectedId ? <IdentityDetails key={selectedId} profileId={selectedId} /> : null;
}

function IdentityDetails({ profileId }: { profileId: string }): JSX.Element {
  const identity = useKuaishouIdentity(profileId);
  const { openDetails, refresh, loading } = useKuaishouIdentities();
  const [copyMessage, setCopyMessage] = useState("");
  const mounted = useRef(false);
  const copyTicket = useRef(0);
  const platformId = identity.snapshot?.platformUserId;
  useEffect(() => {
    mounted.current = true;
    setCopyMessage("");
    ++copyTicket.current;
    return () => { mounted.current = false; ++copyTicket.current; };
  }, [platformId]);
  async function copyId(): Promise<void> {
    if (!platformId) return;
    const ticket = ++copyTicket.current;
    try {
      await navigator.clipboard.writeText(platformId);
      if (mounted.current && ticket === copyTicket.current) setCopyMessage("快手ID已复制");
    } catch {
      if (mounted.current && ticket === copyTicket.current) setCopyMessage("复制失败，请手动选择快手ID复制");
    }
  }
  return <Modal open onClose={() => openDetails(null)} title="快手详情" subtitle={identity.profile?.name ?? profileId} width={760}>
    <section data-testid="kuaishou-detail" aria-label="快手身份检测详情" className="p-5 space-y-5 min-w-0 text-sm text-slate-300 break-words">
      <div className="flex gap-3 items-center min-w-0">
        <IdentityAvatar snapshot={identity.snapshot} size={56} />
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{identity.snapshot?.nickname || "未读出昵称"}</h2>
          <p data-status={identity.status} className={identity.current ? "text-emerald-300" : "text-amber-200"}>
            {identity.label}{identity.history && " · 以下为上次识别信息"}
          </p>
        </div>
      </div>
      <dl className="grid grid-cols-1 sm:grid-cols-[110px_minmax(0,1fr)] gap-2 rounded-xl border border-white/10 p-4">
        <dt className="text-slate-500">快手ID</dt><dd className="font-mono select-text break-all">{platformId || "未读出"}</dd>
        <dt className="text-slate-500">检测时间</dt><dd>{identity.snapshot?.checkedAt || "暂无检测时间"}</dd>
        <dt className="text-slate-500">最后识别时间</dt><dd>{identity.snapshot?.lastSeenAt || "暂无识别时间"}</dd>
        <dt className="text-slate-500">检测来源</dt><dd>Rust 后台 · 小店页面身份检测（本地结果）</dd>
      </dl>
      {identity.error && <p role="alert" className="text-amber-300">{identity.error}</p>}
      {identity.snapshot?.message && <p className="text-amber-200 whitespace-pre-wrap">{identity.snapshot.message}</p>}
      {identity.status === "conflict" && <p className="text-amber-300">登记冲突：识别身份与登记不一致或存在冲突。不会覆盖人工ID或解除绑定，请自行核对登记。</p>}
      <div className="flex flex-wrap gap-2">
        <Button disabled={!platformId} onClick={() => void copyId()} leftIcon={<Copy size={12} />}>复制快手ID</Button>
        <DetectButton profileId={profileId} />
        <Button disabled={loading} onClick={() => void refresh()}>刷新检测状态</Button>
      </div>
      {copyMessage && <p role="status">{copyMessage}</p>}
      <div className="rounded-xl bg-white/[0.03] p-4 space-y-2 text-[12px] leading-relaxed text-slate-400">
        <p>仅展示页面已读出的身份，不能据此确认当前已登录；离线或检测未成功时展示上次识别信息。重新检测的运行条件由后台最终判定。</p>
        <p>本批未实现主体资料、OCR 或初始化；没有初始化完成状态。</p>
        <p>识别结果与手工登记分开保存，不会自动覆盖人工ID，也不会自动解绑或迁移账号。</p>
      </div>
    </section>
  </Modal>;
}
