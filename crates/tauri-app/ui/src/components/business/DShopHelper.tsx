import { useEffect, useState, type JSX } from "react";
import { onShopHelperGoodsChanged, shopHelper } from "../../lib/shopHelper";
import type { HelperGoodInfo, HelperGoodTab, ShopHelperGoodsChanged } from "../../types";
import { Button } from "../atoms/Button";
import { useT } from "../../i18n/LanguageProvider";

const CONTROL =
  "w-full min-w-0 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-2 text-[12px] text-slate-200 outline-none focus:border-purple-400/60 disabled:opacity-50";

function errDetail(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

interface Props {
  profileId: string;
}

/**
 * 跟播助手上车页面（D 组）。
 *
 * 覆盖 4 个 `shop_helper_*` 命令 + 订阅 `shop-helper:goods-changed`。
 * 上车/下车是真实上架写动作：每次执行前走内联二次确认。
 */
export function DShopHelper({ profileId }: Props): JSX.Element {
  const t = useT();
  const [targetId, setTargetId] = useState("");
  const [tab, setTab] = useState<HelperGoodTab>("toAdd");
  const [goods, setGoods] = useState<HelperGoodInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ goodsId: string; action: "on" | "off" } | null>(null);
  const [lastEvent, setLastEvent] = useState<ShopHelperGoodsChanged | null>(null);

  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onShopHelperGoodsChanged((change) => {
      if (active && change.profileId === profileId) setLastEvent(change);
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, [profileId]);

  async function read(): Promise<void> {
    if (!targetId.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      setGoods(await shopHelper.readGoods(profileId, targetId.trim(), tab));
    } catch (cause) {
      setError(t("biz.helper.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function switchTab(): Promise<void> {
    if (!targetId.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      setGoods(await shopHelper.switchTab(profileId, targetId.trim(), tab));
    } catch (cause) {
      setError(t("biz.helper.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  async function act(goodsId: string, action: "on" | "off"): Promise<void> {
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const result =
        action === "on"
          ? await shopHelper.addToCart(profileId, targetId.trim(), goodsId)
          : await shopHelper.removeFromCart(profileId, targetId.trim(), goodsId);
      setConfirm(null);
      setGoods(result.goods);
      setMessage(t("biz.helper.actionDone", { detail: result.detail }));
    } catch (cause) {
      setError(t("biz.helper.loadFailed", { detail: errDetail(cause) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label={t("biz.helper.title")} aria-busy={busy} className="min-w-0 space-y-3 text-[12px]">
      <div className="space-y-1.5">
        <h3 className="font-medium text-slate-200">{t("biz.helper.title")}</h3>
        <p className="text-slate-400 leading-relaxed">{t("biz.helper.subtitle")}</p>
      </div>

      {error && <p role="alert" className="break-words text-red-300">{error}</p>}
      {message && <p role="status" className="text-emerald-300">{message}</p>}

      <div className="rounded-lg border border-white/10 p-3 space-y-2.5">
        <label className="block space-y-1.5 text-slate-400">
          <span>{t("biz.helper.targetLabel")}</span>
          <input
            className={CONTROL}
            value={targetId}
            onChange={(e) => setTargetId(e.target.value)}
            placeholder={t("biz.helper.targetPlaceholder")}
          />
        </label>
        <label className="block space-y-1.5 text-slate-400">
          <span>Tab</span>
          <select className={CONTROL} value={tab} onChange={(e) => setTab(e.target.value as HelperGoodTab)}>
            <option value="toAdd">{t("biz.helper.tabToAdd")}</option>
            <option value="inCart">{t("biz.helper.tabInCart")}</option>
          </select>
        </label>
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => void read()} disabled={!targetId.trim() || busy}>
            {t("biz.helper.read")}
          </Button>
          <Button onClick={() => void switchTab()} disabled={!targetId.trim() || busy}>
            {t("biz.helper.switch")}
          </Button>
        </div>
      </div>

      <div className="space-y-2" data-testid="helper-goods">
        <p className="font-medium text-slate-200">{t("biz.helper.goodsList")}</p>
        {goods.length === 0 && <p className="text-slate-500">{t("biz.helper.empty")}</p>}
        {goods.map((g) => (
          <div key={g.goodsId} className="rounded-lg border border-white/10 p-3 space-y-1.5">
            <p className="font-medium text-slate-200">
              {g.goodsName} · {g.goodsId}
            </p>
            <p className="text-slate-500 break-words">
              {t("biz.helper.statusLabel")}：{g.status} · {t("biz.helper.sourceTabLabel")}：{g.sourceTab} ·{" "}
              {t("biz.helper.actionsLabel")}：{g.availableActions.join(" / ") || "—"}
            </p>
            {g.rawText && <p className="text-slate-600 break-words line-clamp-2">{g.rawText}</p>}
            <div className="flex flex-wrap gap-2 items-center">
              {confirm?.goodsId === g.goodsId ? (
                <div
                  role="group"
                  aria-label={confirm.action === "on" ? t("biz.helper.confirmOnTitle") : t("biz.helper.confirmOffTitle")}
                  className="flex flex-wrap gap-2 items-center"
                >
                  <span className="text-amber-200">
                    {t(confirm.action === "on" ? "biz.helper.confirmOnBody" : "biz.helper.confirmOffBody", {
                      id: g.goodsId,
                    })}
                  </span>
                  <Button
                    size="sm"
                    variant={confirm.action === "on" ? "primary" : "danger"}
                    onClick={() => void act(g.goodsId, confirm.action)}
                    disabled={busy}
                  >
                    {t("common.confirm")}
                  </Button>
                  <Button size="sm" onClick={() => setConfirm(null)} disabled={busy}>
                    {t("common.cancel")}
                  </Button>
                </div>
              ) : (
                <>
                  <Button size="sm" variant="primary" onClick={() => setConfirm({ goodsId: g.goodsId, action: "on" })} disabled={busy}>
                    {t("biz.helper.onCart")}
                  </Button>
                  <Button size="sm" variant="danger" onClick={() => setConfirm({ goodsId: g.goodsId, action: "off" })} disabled={busy}>
                    {t("biz.helper.offCart")}
                  </Button>
                </>
              )}
            </div>
          </div>
        ))}
      </div>

      <div className="rounded-lg border border-white/10 p-3 space-y-1.5" data-testid="helper-event">
        <p className="font-medium text-slate-200">{t("biz.helper.lastEvent")}</p>
        <p className="text-slate-500">
          {lastEvent ? `${lastEvent.action} · ${lastEvent.goodsId} · ${lastEvent.ok ? "ok" : "fail"}` : "—"}
        </p>
      </div>
    </section>
  );
}
