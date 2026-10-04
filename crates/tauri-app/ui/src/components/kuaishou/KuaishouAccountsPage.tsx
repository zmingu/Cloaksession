import type { JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import type { KuaishouTab } from "../screens/Sidebar";
import { KuaishouInteractAccounts } from "./KuaishouInteractAccounts";
import { KuaishouShopAccounts } from "./KuaishouShopAccounts";

/**
 * 快手三个二级菜单的内容分发（小店 / 互动账号 / 直播伴侣）。
 *
 * 小店与互动账号各自拥有「添加账号」入口：小店复用 App 的通用新建环境表单，
 * 互动账号使用自己的建号向导（环境 + 快手主站扫码 + 互动登记）。
 * 直播伴侣仍是空壳，后续按 tab 填充，不在此处堆业务。
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
  // 互动账号自带建号向导（含主站扫码与互动登记），不需要 App 的通用新建环境入口。
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
