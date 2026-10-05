import { useEffect, useRef, useState, type JSX } from "react";
import { Copy, RefreshCw } from "lucide-react";
import type { KuaishouIdentitySnapshot } from "../../lib/kuaishouIdentity";
import { useKuaishouIdentities, useKuaishouIdentity } from "../../lib/KuaishouIdentityProvider";
import { Modal } from "../atoms";
import { Button } from "../atoms/Button";
import { profiles } from "../../lib/ipc";
import { identityCopyText, kuaishouSubject, type KuaishouSubjectDetail } from "../../lib/kuaishouSubject";
import { InitSteps, InitSummary } from "./KuaishouSubject";

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

/** Platform avatar for a detected identity; null when no id was read. Reused by
 *  the shop account list so every surface shows the same photo and cache. */
export function IdentityAvatar({ snapshot, size = 26 }: { snapshot: KuaishouIdentitySnapshot | null; size?: number }): JSX.Element | null {
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
      {identity.snapshot?.platformUserId && <InitSummary key={identity.snapshot.platformUserId}
        platformUserId={identity.snapshot.platformUserId} observation={identity.snapshot.checkedAt ?? null} />}
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

/**
 * Slim identity dialog: avatar + nickname, Kuaishou ID, the subject archive's
 * real name and ID-card number, the account's initialization status, and one
 * copy button. Detection timestamps, provenance, photo/validation panels and
 * the manual retry controls live elsewhere (account archives / the shop list).
 */
function IdentityDetails({ profileId }: { profileId: string }): JSX.Element {
  const identity = useKuaishouIdentity(profileId);
  const { openDetails } = useKuaishouIdentities();
  const [detail, setDetail] = useState<KuaishouSubjectDetail | null>(null);
  const [copyMessage, setCopyMessage] = useState("");
  const alive = useRef(false);
  const version = useRef(0);
  const copyTicket = useRef(0);
  const platformId = identity.snapshot?.platformUserId ?? null;
  const archive = detail?.archive ?? null;

  // One read fills both the subject fields and the init steps. Late responses
  // are dropped by the version ticket, and unmount stops any setState.
  useEffect(() => {
    alive.current = true;
    setCopyMessage("");
    ++copyTicket.current;
    setDetail(null);
    if (!platformId) {
      return () => {
        alive.current = false;
        ++version.current;
        ++copyTicket.current;
      };
    }
    const ticket = ++version.current;
    void kuaishouSubject.detail(platformId).then(
      (value) => {
        if (alive.current && ticket === version.current) setDetail(value);
      },
      () => {
        if (alive.current && ticket === version.current) setDetail(null);
      },
    );
    return () => {
      alive.current = false;
      ++version.current;
      ++copyTicket.current;
    };
  }, [platformId]);

  async function copyInfo(): Promise<void> {
    if (!platformId) return;
    const ticket = ++copyTicket.current;
    try {
      await navigator.clipboard.writeText(
        identityCopyText(platformId, archive?.realName ?? "", archive?.idCard ?? ""),
      );
      if (alive.current && ticket === copyTicket.current) setCopyMessage("已复制快手ID、姓名、身份证号");
    } catch {
      if (alive.current && ticket === copyTicket.current) setCopyMessage("复制失败，请手动选择复制");
    }
  }

  return (
    <Modal open onClose={() => openDetails(null)} title="快手详情" subtitle={identity.profile?.name ?? profileId} width={520}>
      <section data-testid="kuaishou-detail" aria-label="快手身份详情" className="p-5 space-y-4 min-w-0 text-sm text-slate-300 break-words">
        <div className="flex gap-3 items-center min-w-0">
          <IdentityAvatar snapshot={identity.snapshot} size={56} />
          <h2 className="min-w-0 truncate text-base font-semibold">{identity.snapshot?.nickname || "未读出昵称"}</h2>
        </div>
        <dl className="grid grid-cols-1 sm:grid-cols-[92px_minmax(0,1fr)] gap-2 rounded-xl border border-white/10 p-4">
          <dt className="text-slate-500">快手ID</dt>
          <dd className="font-mono select-text break-all">{platformId || "—"}</dd>
          <dt className="text-slate-500">姓名</dt>
          <dd className="select-text break-all">{archive?.realName || "—"}</dd>
          <dt className="text-slate-500">身份证号</dt>
          <dd className="font-mono select-text break-all">{archive?.idCard || "—"}</dd>
          <dt className="text-slate-500">初始化状态</dt>
          <dd>
            <InitSteps steps={detail?.steps ?? []} compact />
          </dd>
        </dl>
        <div className="flex flex-wrap items-center gap-2">
          <Button disabled={!platformId} onClick={() => void copyInfo()} leftIcon={<Copy size={12} />}>
            复制信息
          </Button>
          {copyMessage && <p role="status">{copyMessage}</p>}
        </div>
      </section>
    </Modal>
  );
}
