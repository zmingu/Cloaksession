import type { JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import type { ProfileSummary } from "../../types";
import { CAutoMessagePanel } from "../business/CAutoMessagePanel";
import { CAutoReplyPanel } from "../business/CAutoReplyPanel";
import { CommentListenerPage } from "../business/CommentListenerPage";
import { CScenePlayPanel } from "../business/CScenePlayPanel";
import { DAutoPopup } from "../business/DAutoPopup";
import { DProductScripts } from "../business/DProductScripts";
import { DShopHelper } from "../business/DShopHelper";
import { LiveRoomMonitorPage } from "../business/LiveRoomMonitorPage";
import { SubAccountsPage } from "../business/SubAccountsPage";

/**
 * 「直播 › 直播互动」页。
 *
 * 页面内是三个**堆叠子区**（不是嵌套 tab），顺序固定：
 *   1. 互动账号脚本互动 → LiveRoomMonitorPage + SubAccountsPage + CScenePlayPanel
 *   2. 自动上车         → DShopHelper + DProductScripts
 *   3. 自动发言         → CAutoMessagePanel + CAutoReplyPanel + DAutoPopup + CommentListenerPage
 *
 * 子区内保留子区标题；`D*` 组件需要的 profileId 取首个浏览器配置
 * （与搬迁前业务页行为一致）。
 */
interface Props {
  profiles: ProfileSummary[];
}

export function LiveInteractPage({ profiles }: Props): JSX.Element {
  const t = useT();
  const profileId = profiles[0]?.id ?? "";

  return (
    <div data-testid="live-interact-page" className="flex-1 flex flex-col min-w-0 min-h-0">
      <section data-testid="live-interact-script" className="flex flex-col min-w-0 px-6 py-4 gap-4">
        <h2 className="text-[15px] font-semibold">{t("live.interact.script")}</h2>
        <LiveRoomMonitorPage />
        <SubAccountsPage />
        <CScenePlayPanel />
      </section>

      <section data-testid="live-interact-shelf" className="flex flex-col min-w-0 px-6 py-4 gap-4">
        <h2 className="text-[15px] font-semibold">{t("live.interact.shelf")}</h2>
        <DShopHelper profileId={profileId} />
        <DProductScripts profileId={profileId} />
      </section>

      <section data-testid="live-interact-speak" className="flex flex-col min-w-0 px-6 py-4 gap-4">
        <h2 className="text-[15px] font-semibold">{t("live.interact.speak")}</h2>
        <CAutoMessagePanel />
        <CAutoReplyPanel />
        <DAutoPopup profileId={profileId} />
        <CommentListenerPage />
      </section>
    </div>
  );
}
