import { useEffect, useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { businessAccounts } from "../../lib/businessAccounts";
import { bindCreator } from "../../lib/ipc";
import { jinniu } from "../../lib/jinniu";
import { useKuaishouIdentities } from "../../lib/KuaishouIdentityProvider";
import type { JinniuAccountWithStatus } from "../../lib/jinniu";
import { confirm } from "../atoms";
import { Button } from "../atoms/Button";
import type { AuthorizeItem, ProfileSummary } from "../../types";

interface Props {
  profiles: ProfileSummary[];
}

function errText(e: unknown): string {
  return typeof e === "string" ? e : (e as Error).message ?? String(e);
}

/** 小店用户下拉选项：profile + 身份检测到的快手 ID。 */
interface ShopOption {
  id: string;
  name: string;
  kuaishouId: string;
}

/**
 * 达人授权页 (jieger `bindCreator`).
 *
 * 确认「金牛用户 ↔ 小店用户」的对应关系只需要两个选择框：
 * - 金牛账户：提供运行中的浏览器环境（profileId）、金牛 ID = 已识别的主账号
 *   快手 ID（jieger 业务键）、当前子户 ID（随授权 URL 附加）。
 * - 小店用户：提供达人快手号 = 身份检测到的快手 ID。
 *
 * - `list` reads persisted records only (no browser, no confirm).
 * - `sync` scrapes the Jinniu backend in the browser → confirm-gated.
 * - `authorize` runs the full bind flow → confirm-gated write action.
 */
export function BindAuthorizePage({ profiles }: Props): JSX.Element {
  const t = useT();
  const { entries } = useKuaishouIdentities();
  const [jinniuRows, setJinniuRows] = useState<JinniuAccountWithStatus[] | null>(null);
  // 已被其它账号视角（互动 / 金牛）占用的环境，不出现在小店用户下拉里。
  const [takenProfileIds, setTakenProfileIds] = useState<Set<string>>(() => new Set());
  const [jinniuAccountId, setJinniuAccountId] = useState("");
  const [shopProfileId, setShopProfileId] = useState("");
  const [skipConfirm, setSkipConfirm] = useState(false);
  const [items, setItems] = useState<AuthorizeItem[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      const [accounts, records] = await Promise.all([
        jinniu.list().catch(() => null),
        businessAccounts.list().catch(() => null),
      ]);
      if (!active) return;
      setJinniuRows(accounts);
      setTakenProfileIds(
        new Set(
          (records ?? [])
            .filter(
              (record) =>
                (record.kind === "kuaishou-sub" || record.kind === "jinniu") &&
                record.profileId,
            )
            .map((record) => record.profileId as string),
        ),
      );
    })();
    return () => {
      active = false;
    };
  }, []);

  const selectedJinniu = jinniuRows?.find((row) => row.id === jinniuAccountId) ?? null;
  const jinniuId = selectedJinniu?.masterId?.trim() ?? "";
  const currentProfile = selectedJinniu?.profileId ?? "";
  const accountArg = selectedJinniu?.currentSubAccountId?.trim() || undefined;

  const shopOptions: ShopOption[] = profiles
    .filter((profile) => !takenProfileIds.has(profile.id))
    .map((profile) => ({
      id: profile.id,
      name: profile.name,
      kuaishouId: entries.get(profile.id)?.snapshot?.platformUserId ?? "",
    }));
  const kuaishouId = shopOptions.find((option) => option.id === shopProfileId)?.kuaishouId ?? "";

  async function run(key: string, fn: () => Promise<void>): Promise<void> {
    setBusy(key);
    setNotice(null);
    try {
      await fn();
    } catch (e) {
      setNotice(t("account.failedToast", { detail: errText(e) }));
    } finally {
      setBusy(null);
    }
  }

  const onList = (): Promise<void> =>
    run("list", async () => {
      const res = await bindCreator.list(jinniuId);
      if (!res.ok) throw new Error(res.error ?? "unknown");
      setItems(res.data);
    });

  const onSync = (): Promise<void> =>
    run("sync", async () => {
      const ok = await confirm({
        title: t("biz.bind.sync"),
        body: t("biz.bind.confirmSync"),
        confirmLabel: t("biz.bind.sync"),
      });
      if (!ok) return;
      const res = await bindCreator.sync(currentProfile, jinniuId, accountArg);
      if (!res.ok) throw new Error(res.error ?? "unknown");
      setItems(res.data);
      setNotice(t("biz.bind.syncedToast", { n: String(res.data.length) }));
    });

  const onAuthorize = (): Promise<void> =>
    run("authorize", async () => {
      const ok = await confirm({
        title: t("biz.bind.authorize"),
        body: t("biz.bind.confirmAuthorize", { id: kuaishouId }),
        confirmLabel: t("biz.bind.authorize"),
        destructive: !skipConfirm,
      });
      if (!ok) return;
      const res = await bindCreator.authorize(
        currentProfile,
        jinniuId,
        kuaishouId,
        skipConfirm || undefined,
        accountArg,
      );
      if (!res.ok) throw new Error(res.error ?? "unknown");
      setNotice(skipConfirm ? t("biz.bind.dryRunToast") : t("biz.bind.authorizedToast"));
    });

  const ready = jinniuId !== "" && currentProfile !== "";
  // 主账号识别发生在账户启动之后；未启动过的账户拿不到金牛 ID。
  const needMaster = selectedJinniu !== null && jinniuId === "";
  const needShopId = shopProfileId !== "" && kuaishouId === "";

  return (
    <section aria-label={t("biz.bind.title")} className="flex flex-col gap-4 p-6 max-w-3xl">
      <div>
        <h2 className="text-lg font-semibold">{t("biz.bind.title")}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t("biz.bind.desc")}</p>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.bind.jinniuAccount")}
          <select
            aria-label={t("biz.bind.jinniuAccount")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={jinniuAccountId}
            onChange={(e) => setJinniuAccountId(e.target.value)}
          >
            <option value="">{t("biz.bind.pickJinniu")}</option>
            {(jinniuRows ?? []).map((row) => (
              <option key={row.id} value={row.id}>
                {row.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.bind.shopUser")}
          <select
            aria-label={t("biz.bind.shopUser")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={shopProfileId}
            onChange={(e) => setShopProfileId(e.target.value)}
          >
            <option value="">{t("biz.bind.pickShop")}</option>
            {shopOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.kuaishouId ? `${option.name}（${option.kuaishouId}）` : option.name}
              </option>
            ))}
          </select>
        </label>
      </div>

      {needMaster && <p className="text-sm text-amber-300">{t("biz.bind.needMaster")}</p>}
      {needShopId && <p className="text-sm text-amber-300">{t("biz.bind.needShopId")}</p>}

      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={skipConfirm}
          onChange={(e) => setSkipConfirm(e.target.checked)}
        />
        {t("biz.bind.skipConfirm")}
      </label>

      <div className="flex gap-2">
        <Button size="sm" disabled={busy !== null || !ready} onClick={() => void onList()}>
          {t("biz.bind.list")}
        </Button>
        <Button
          size="sm"
          variant="accent"
          disabled={busy !== null || !ready}
          onClick={() => void onSync()}
        >
          {busy === "sync" ? t("common.loading") : t("biz.bind.sync")}
        </Button>
        <Button
          size="sm"
          variant={skipConfirm ? "secondary" : "primary"}
          disabled={busy !== null || !ready || kuaishouId === ""}
          onClick={() => void onAuthorize()}
        >
          {busy === "authorize" ? t("common.loading") : t("biz.bind.authorize")}
        </Button>
      </div>

      {notice && (
        <div role="status" className="text-sm text-muted-foreground">
          {notice}
        </div>
      )}

      {items !== null &&
        (items.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.bind.empty")}</p>
        ) : (
          <table className="text-sm w-full">
            <thead>
              <tr className="text-left text-muted-foreground">
                <th className="py-1 pr-3">{t("biz.bind.cols.user")}</th>
                <th className="py-1 pr-3">{t("biz.bind.cols.status")}</th>
                <th className="py-1">{t("biz.bind.cols.time")}</th>
              </tr>
            </thead>
            <tbody>
              {items.map((it) => (
                <tr key={it.userId} data-testid={`bind-row-${it.userId}`} className="border-t border-[var(--border)]">
                  <td className="py-1.5 pr-3">
                    {it.userName} <span className="mono text-xs opacity-70">{it.userId}</span>
                  </td>
                  <td className="py-1.5 pr-3">{it.status}</td>
                  <td className="py-1.5">{it.authorizeTime}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ))}
    </section>
  );
}
