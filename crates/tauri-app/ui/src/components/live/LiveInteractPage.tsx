import { type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { CAutoMessagePanel } from "../business/CAutoMessagePanel";
import { CAutoReplyPanel } from "../business/CAutoReplyPanel";
import { CommentListenerPage } from "../business/CommentListenerPage";
import { CScenePlayPanel } from "../business/CScenePlayPanel";
import { DAutoPopup } from "../business/DAutoPopup";
import { DProductScripts } from "../business/DProductScripts";
import { DShopHelper } from "../business/DShopHelper";
import { SubAccountsPage } from "../business/SubAccountsPage";
import { LiveAccountRail } from "./LiveAccountRail";
import type { LiveAccountsState } from "./useLiveAccounts";

/**
 * 「直播 › 直播互动」页 = **账号工作台**。
 *
 * 左侧账号侧栏（`LiveAccountRail`，键 = `profileId`）+ 右侧该账号的互动模块，
 * 分三组（沿用 `live.interact.*` i18n 与 `live-interact-*` testid）：
 *   1. 互动账号脚本互动 → SubAccountsPage + CScenePlayPanel
 *   2. 自动上车         → DShopHelper + DProductScripts
 *   3. 自动发言         → CAutoMessagePanel + CAutoReplyPanel + DAutoPopup + CommentListenerPage
 *
 * 账号状态（列表 / 选中 / 在播）由 `LiveSection` 的 `useLiveAccounts` 单处订阅后
 * 以 `accounts` 下传（本页**不再**自行订阅 `live-launch-state-changed`）。选中账号的
 * `profileId` 传给所有模块；切换账号时右侧 `key={selectedId}` 强制重挂载，避免跨账号串状态。
 */
interface Props {
  /** 账号状态（由 LiveSection 的 useLiveAccounts 统一提供）。 */
  accounts: LiveAccountsState;
}

export function LiveInteractPage({ accounts }: Props): JSX.Element {
  const t = useT();
  const { items, selectedId, setSelectedId } = accounts;

  const profileId = selectedId;

  return (
    <div data-testid="live-interact-page" className="h-full flex min-w-0 min-h-0">
      <LiveAccountRail
        items={items}
        selectedId={selectedId}
        onSelect={setSelectedId}
        emptyText={t("live.accounts.empty")}
        title={t("live.accounts.title")}
      />

      <div className="flex-1 min-w-0 min-h-0 overflow-y-auto" key={selectedId}>
        <section data-testid="live-interact-script" className="flex flex-col min-w-0 px-6 py-4 gap-4">
          <h2 className="text-[15px] font-semibold">{t("live.interact.script")}</h2>
          <SubAccountsPage profileId={profileId} />
          <CScenePlayPanel />
        </section>

        <section data-testid="live-interact-shelf" className="flex flex-col min-w-0 px-6 py-4 gap-4">
          <h2 className="text-[15px] font-semibold">{t("live.interact.shelf")}</h2>
          <DShopHelper profileId={profileId} />
          <DProductScripts profileId={profileId} />
        </section>

        <section data-testid="live-interact-speak" className="flex flex-col min-w-0 px-6 py-4 gap-4">
          <h2 className="text-[15px] font-semibold">{t("live.interact.speak")}</h2>
          <CAutoMessagePanel accountId={profileId} />
          <CAutoReplyPanel accountId={profileId} />
          <DAutoPopup profileId={profileId} />
          <CommentListenerPage profileId={profileId} />
        </section>
      </div>
    </div>
  );
}
