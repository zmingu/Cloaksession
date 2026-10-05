import type { JSX } from "react";

import type { KuaishouTab } from "../screens/Sidebar";
import { MateLoginPage } from "../business/MateLoginPage";
import { KuaishouInteractAccounts } from "./KuaishouInteractAccounts";
import { KuaishouShopAccounts } from "./KuaishouShopAccounts";

/**
 * 快手三个二级菜单的内容分发（小店 / 互动账号 / 直播伴侣）。
 *
 * 小店与互动账号各自拥有「添加账号」入口：小店复用 App 的通用新建环境表单，
 * 互动账号使用自己的建号向导（环境 + 快手主站扫码 + 互动登记）。
 * 直播伴侣挂载伴侣扫码登录页（`MateLoginPage`）——它只负责扫码登录，不开播；
 * 伴侣开播（本地视频推流）在「直播 › 开播准备」。
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
  return <MateLoginPage />;
}
