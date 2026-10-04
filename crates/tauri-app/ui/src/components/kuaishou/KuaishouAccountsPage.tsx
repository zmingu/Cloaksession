import type { JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import type { KuaishouTab } from "../screens/Sidebar";
import { KuaishouInteractAccounts } from "./KuaishouInteractAccounts";
import { KuaishouShopAccounts } from "./KuaishouShopAccounts";

/**
 * 快手三个二级菜单的内容分发（小店 / 互动账号 / 直播伴侣）。
 *
 * 小店与互动账号已有内容；直播伴侣仍是空壳，后续按 tab 填充，
 * 不在此处堆业务。
 */
export function KuaishouAccountsPage({
  tab,
  onAddAccount,
}: {
  tab: KuaishouTab;
  /** Opens the shared "New profile" sheet owned by App, so a shop account and a
   *  browser profile are created by the same flow. */
  onAddAccount?: () => void;
}): JSX.Element {
  if (tab === "shop") return <KuaishouShopAccounts onAddAccount={onAddAccount} />;
  if (tab === "interact") return <KuaishouInteractAccounts />;
  return <KuaishouShell tab={tab} />;
}

function KuaishouShell({ tab }: { tab: KuaishouTab }): JSX.Element {
  const t = useT();
  const isMate = tab === "mate";
  const title = isMate ? t("kuaishou.tab.mate") : t("kuaishou.tab.interact");
  const hint = isMate ? t("kuaishou.mate.hint") : t("kuaishou.interact.hint");
  return (
    <div className="flex-1 flex flex-col min-w-0 min-h-0 overflow-y-auto px-6 py-4">
      <h2 className="text-[15px] font-semibold">{title}</h2>
      <div className="mt-2 text-[13px] text-muted-foreground">{hint}</div>
    </div>
  );
}
