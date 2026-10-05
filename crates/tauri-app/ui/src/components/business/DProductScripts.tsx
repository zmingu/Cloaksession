import { useCallback, useEffect, useState, type JSX } from "react";
import { onProductScriptState, productScripts } from "../../lib/productScripts";
import type {
  AddShopProductScriptLineInput,
  ProductScriptPlayState,
  ScriptLineAction,
  ShopProductScript,
  ShopProductScriptDetail,
} from "../../types";
import { Button } from "../atoms/Button";
import { useT } from "../../i18n/LanguageProvider";

const ACTIONS: ScriptLineAction[] = ["on-shelf", "off-shelf", "explain", "cancel-explain"];
const CONTROL =
  "w-full min-w-0 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-2 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

function errDetail(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

interface Props {
  profileId: string;
}

/**
 * 商品话术库页面（D 组）。
 *
 * 覆盖 11 个 `shop_product_script_*` 命令：脚本 CRUD + 话术行 CRUD + 整组重排
 * + 播放/停止。删除与播放是真实写操作：走内联二次确认（先点删除/播放再点确认）。
 * 播放状态经 `product-script-state-changed` 订阅。
 */
export function DProductScripts({ profileId }: Props): JSX.Element {
  const t = useT();
  const [scripts, setScripts] = useState<ShopProductScript[]>([]);
  const [detail, setDetail] = useState<ShopProductScriptDetail | null>(null);
  const [busy, setBusy] = useState<"loading" | "saving" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [playState, setPlayState] = useState<ProductScriptPlayState | null>(null);
  const [confirmPlayId, setConfirmPlayId] = useState<string | null>(null);

  // Create form.
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  // Update form (tri-state description: keep / clear / set).
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [clearDescription, setClearDescription] = useState(false);
  // Add-line form.
  const [lineGoodsId, setLineGoodsId] = useState("");
  const [lineGoodsName, setLineGoodsName] = useState("");
  const [lineAction, setLineAction] = useState<ScriptLineAction>("explain");
  const [lineVideoTime, setLineVideoTime] = useState("0");
  const [lineLead, setLineLead] = useState("0");
  const [lineContent, setLineContent] = useState("");

  const reload = useCallback(async (): Promise<void> => {
    setBusy("loading");
    setError(null);
    try {
      const list = await productScripts.list();
      setScripts(list);
      if (detail) {
        const next = await productScripts.get(detail.script.id);
        setDetail(next);
        if (next) {
          setEditName(next.script.name);
          setEditDescription(next.script.description ?? "");
          setClearDescription(false);
        }
      }
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }, [detail, t]);

  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onProductScriptState((state) => {
      if (active) setPlayState(state);
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  async function play(scriptId: string): Promise<void> {
    setBusy("saving");
    setError(null);
    setMessage(null);
    try {
      const playback = await productScripts.play(profileId, scriptId);
      setConfirmPlayId(null);
      setMessage(t("biz.pscript.playingToast", { n: playback.scheduledCount }));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function stop(scriptId: string): Promise<void> {
    setBusy("saving");
    setError(null);
    setMessage(null);
    try {
      const stopped = await productScripts.stop(scriptId);
      setMessage(stopped ? t("biz.pscript.stoppedToast") : t("biz.pscript.notPlayingToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function open(id: string): Promise<void> {
    setBusy("loading");
    setError(null);
    try {
      const next = await productScripts.get(id);
      setDetail(next);
      if (next) {
        setEditName(next.script.name);
        setEditDescription(next.script.description ?? "");
        setClearDescription(false);
      }
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function create(): Promise<void> {
    if (!name.trim() || busy) return;
    setBusy("saving");
    setError(null);
    setMessage(null);
    try {
      const created = await productScripts.create({
        name: name.trim(),
        description: description.trim() || null,
      });
      setName("");
      setDescription("");
      setScripts((prev) => [created, ...prev]);
      setMessage(t("biz.pscript.savedToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function saveDetail(): Promise<void> {
    if (!detail || busy) return;
    setBusy("saving");
    setError(null);
    setMessage(null);
    try {
      const updated = await productScripts.update(detail.script.id, {
        name: editName.trim() || undefined,
        // Tri-state: undefined = keep, null = clear, string = set.
        description: clearDescription ? null : editDescription.trim() ? editDescription.trim() : undefined,
      });
      const next = await productScripts.get(updated.id);
      if (next) {
        setDetail(next);
        setEditName(next.script.name);
        setEditDescription(next.script.description ?? "");
        setClearDescription(false);
      }
      setScripts((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      setMessage(t("biz.pscript.savedToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function remove(id: string): Promise<void> {
    setBusy("saving");
    setError(null);
    try {
      await productScripts.delete(id);
      setConfirmId(null);
      setScripts((prev) => prev.filter((s) => s.id !== id));
      if (detail?.script.id === id) setDetail(null);
      setMessage(t("biz.pscript.deletedToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function addLine(): Promise<void> {
    if (!detail || !lineGoodsId.trim() || busy) return;
    setBusy("saving");
    setError(null);
    try {
      const input: AddShopProductScriptLineInput = {
        scriptId: detail.script.id,
        action: lineAction,
        goodsId: lineGoodsId.trim(),
        goodsName: lineGoodsName.trim() || null,
        videoTimeSec: Number(lineVideoTime) || 0,
        leadSec: Number(lineLead) || 0,
        content: lineContent,
      };
      await productScripts.addLine(input);
      const next = await productScripts.get(detail.script.id);
      if (next) setDetail(next);
      setLineGoodsId("");
      setLineGoodsName("");
      setLineContent("");
      setMessage(t("biz.pscript.savedToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function removeLine(id: string): Promise<void> {
    if (!detail) return;
    setBusy("saving");
    setError(null);
    try {
      await productScripts.deleteLine(id);
      setConfirmId(null);
      const next = await productScripts.get(detail.script.id);
      if (next) setDetail(next);
      setMessage(t("biz.pscript.deletedToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  async function moveLine(id: string, dir: -1 | 1): Promise<void> {
    if (!detail || busy) return;
    const ids = detail.lines.map((l) => l.id);
    const i = ids.indexOf(id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= ids.length) return;
    const reordered = ids.slice();
    [reordered[i], reordered[j]] = [reordered[j], reordered[i]];
    setBusy("saving");
    setError(null);
    try {
      const lines = await productScripts.reorderLines(detail.script.id, reordered);
      setDetail({ script: detail.script, lines });
      setMessage(t("biz.pscript.reorderedToast"));
    } catch (cause) {
      setError(t("biz.pscript.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(null);
    }
  }

  return (
    <section aria-label={t("biz.pscript.title")} aria-busy={busy !== null} className="min-w-0 space-y-3 text-[12px]">
      <div className="space-y-1.5">
        <h3 className="font-medium text-slate-200">{t("biz.pscript.title")}</h3>
        <p className="text-slate-400 leading-relaxed">{t("biz.pscript.subtitle")}</p>
      </div>

      {error && <p role="alert" className="break-words text-red-300">{error}</p>}
      {message && <p role="status" className="text-emerald-300">{message}</p>}
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => void reload()} disabled={!!busy}>{t("biz.pscript.refresh")}</Button>
      </div>

      <div className="rounded-lg border border-white/10 p-3 space-y-1.5" data-testid="pscript-play-state">
        <p className="font-medium text-slate-200">{t("biz.pscript.playStatusTitle")}</p>
        <p className="text-slate-400">
          {playState
            ? `${playState.status} · ${t("biz.pscript.playProgress", {
                current: playState.currentIndex,
                total: playState.total,
              })}`
            : "—"}
        </p>
        {playState?.currentLineId && (
          <p className="text-slate-500 break-words">{playState.currentLineId}</p>
        )}
      </div>

      {!detail && (
        <>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void create();
            }}
            className="rounded-lg border border-white/10 p-3 space-y-2.5"
          >
            <label className="block space-y-1.5 text-slate-400">
              <span>{t("biz.pscript.nameLabel")}</span>
              <input
                className={CONTROL}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("biz.pscript.namePlaceholder")}
              />
            </label>
            <label className="block space-y-1.5 text-slate-400">
              <span>{t("biz.pscript.descriptionLabel")}</span>
              <input
                className={CONTROL}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("biz.pscript.descriptionPlaceholder")}
              />
            </label>
            <Button type="submit" variant="primary" disabled={!name.trim() || !!busy}>
              {t("biz.pscript.create")}
            </Button>
          </form>

          <div className="space-y-2" data-testid="pscript-list">
            <p className="font-medium text-slate-200">{t("biz.pscript.listLabel")}</p>
            {scripts.length === 0 && <p className="text-slate-500">{t("biz.pscript.empty")}</p>}
            {scripts.map((s) => (
              <div key={s.id} className="rounded-lg border border-white/10 p-3 space-y-1.5">
                <p className="font-medium text-slate-200">{s.name}</p>
                {s.description && <p className="text-slate-400 break-words">{s.description}</p>}
                <div className="flex flex-wrap gap-2">
                  <Button size="sm" onClick={() => void open(s.id)} disabled={!!busy}>
                    {t("biz.pscript.open")}
                  </Button>
                  {confirmPlayId === s.id ? (
                    <div role="group" aria-label={t("biz.pscript.confirmPlayTitle")} className="flex flex-wrap gap-2 items-center">
                      <span className="text-amber-200">{t("biz.pscript.confirmPlayBody")}</span>
                      <Button size="sm" variant="primary" onClick={() => void play(s.id)} disabled={!!busy}>
                        {t("common.confirm")}
                      </Button>
                      <Button size="sm" onClick={() => setConfirmPlayId(null)} disabled={!!busy}>
                        {t("common.cancel")}
                      </Button>
                    </div>
                  ) : (
                    <Button size="sm" variant="primary" onClick={() => setConfirmPlayId(s.id)} disabled={!!busy}>
                      {t("biz.pscript.play")}
                    </Button>
                  )}
                  {confirmId === s.id ? (
                    <div role="group" aria-label={t("biz.pscript.confirmDeleteTitle")} className="flex flex-wrap gap-2 items-center">
                      <span className="text-amber-200">{t("biz.pscript.confirmDeleteBody")}</span>
                      <Button size="sm" variant="danger" onClick={() => void remove(s.id)} disabled={!!busy}>
                        {t("common.confirm")}
                      </Button>
                      <Button size="sm" onClick={() => setConfirmId(null)} disabled={!!busy}>
                        {t("common.cancel")}
                      </Button>
                    </div>
                  ) : (
                    <Button size="sm" variant="danger" onClick={() => setConfirmId(s.id)} disabled={!!busy}>
                      {t("biz.pscript.delete")}
                    </Button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      {detail && (
        <div className="space-y-3" data-testid="pscript-detail">
          <div className="flex flex-wrap gap-2">
            <Button size="sm" onClick={() => setDetail(null)}>{t("biz.pscript.backToList")}</Button>
            <Button
              size="sm"
              variant="danger"
              onClick={() => void stop(detail.script.id)}
              disabled={!!busy || playState?.scriptId !== detail.script.id}
            >
              {t("biz.pscript.stop")}
            </Button>
          </div>
          <p className="font-medium text-slate-200">{t("biz.pscript.detailTitle")}：{detail.script.name}</p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void saveDetail();
            }}
            className="rounded-lg border border-white/10 p-3 space-y-2.5"
          >
            <label className="block space-y-1.5 text-slate-400">
              <span>{t("biz.pscript.nameLabel")}</span>
              <input className={CONTROL} value={editName} onChange={(e) => setEditName(e.target.value)} />
            </label>
            <label className="block space-y-1.5 text-slate-400">
              <span>{t("biz.pscript.descriptionLabel")}</span>
              <input
                className={CONTROL}
                value={editDescription}
                disabled={clearDescription}
                onChange={(e) => setEditDescription(e.target.value)}
              />
            </label>
            <label className="flex items-center gap-2 text-slate-400 cursor-pointer">
              <input
                type="checkbox"
                checked={clearDescription}
                onChange={(e) => setClearDescription(e.target.checked)}
                className="w-3.5 h-3.5 rounded accent-[var(--accent)]"
              />
              {t("biz.pscript.tristateNote")}
            </label>
            <Button type="submit" variant="primary" disabled={!!busy}>{t("biz.pscript.save")}</Button>
          </form>

          <div className="space-y-2">
            <p className="font-medium text-slate-200">{t("biz.pscript.linesTitle")}</p>
            <p className="text-[11px] text-slate-500">{t("biz.pscript.reorderHint")}</p>
            {detail.lines.map((l) => (
              <div key={l.id} className="rounded-lg border border-white/10 p-3 space-y-1.5">
                <p className="text-slate-200 break-words">
                  {l.goodsId}{l.goodsName ? ` · ${l.goodsName}` : ""} · {l.action} · {l.videoTimeSec}s
                </p>
                {l.content && <p className="text-slate-400 break-words">{l.content}</p>}
                <div className="flex flex-wrap gap-2 items-center">
                  <Button size="sm" onClick={() => void moveLine(l.id, -1)} disabled={!!busy}>↑</Button>
                  <Button size="sm" onClick={() => void moveLine(l.id, 1)} disabled={!!busy}>↓</Button>
                  {confirmId === l.id ? (
                    <div role="group" aria-label={t("biz.pscript.confirmDeleteTitle")} className="flex flex-wrap gap-2 items-center">
                      <Button size="sm" variant="danger" onClick={() => void removeLine(l.id)} disabled={!!busy}>
                        {t("common.confirm")}
                      </Button>
                      <Button size="sm" onClick={() => setConfirmId(null)} disabled={!!busy}>
                        {t("common.cancel")}
                      </Button>
                    </div>
                  ) : (
                    <Button size="sm" variant="danger" onClick={() => setConfirmId(l.id)} disabled={!!busy}>
                      {t("biz.pscript.delete")}
                    </Button>
                  )}
                </div>
              </div>
            ))}
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              void addLine();
            }}
            className="rounded-lg border border-white/10 p-3 space-y-2.5"
          >
            <div className="grid grid-cols-2 gap-2.5">
              <label className="block space-y-1.5 text-slate-400">
                <span>{t("biz.pscript.goodsIdLabel")}</span>
                <input className={CONTROL} value={lineGoodsId} onChange={(e) => setLineGoodsId(e.target.value)} />
              </label>
              <label className="block space-y-1.5 text-slate-400">
                <span>{t("biz.pscript.goodsNameLabel")}</span>
                <input className={CONTROL} value={lineGoodsName} onChange={(e) => setLineGoodsName(e.target.value)} />
              </label>
            </div>
            <div className="grid grid-cols-3 gap-2.5">
              <label className="block space-y-1.5 text-slate-400">
                <span>{t("biz.pscript.actionLabel")}</span>
                <select className={CONTROL} value={lineAction} onChange={(e) => setLineAction(e.target.value as ScriptLineAction)}>
                  {ACTIONS.map((a) => (
                    <option key={a} value={a}>{a}</option>
                  ))}
                </select>
              </label>
              <label className="block space-y-1.5 text-slate-400">
                <span>{t("biz.pscript.videoTimeLabel")}</span>
                <input className={CONTROL} value={lineVideoTime} onChange={(e) => setLineVideoTime(e.target.value)} />
              </label>
              <label className="block space-y-1.5 text-slate-400">
                <span>{t("biz.pscript.leadLabel")}</span>
                <input className={CONTROL} value={lineLead} onChange={(e) => setLineLead(e.target.value)} />
              </label>
            </div>
            <label className="block space-y-1.5 text-slate-400">
              <span>{t("biz.pscript.contentLabel")}</span>
              <input className={CONTROL} value={lineContent} onChange={(e) => setLineContent(e.target.value)} />
            </label>
            <Button type="submit" disabled={!lineGoodsId.trim() || !!busy}>
              {t("biz.pscript.addLine")}
            </Button>
          </form>
        </div>
      )}
    </section>
  );
}
