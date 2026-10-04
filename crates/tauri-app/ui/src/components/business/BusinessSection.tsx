import { useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { cn } from "../../lib/cn";
import type { ProfileSummary } from "../../types";
import { KuaishouAuthPage } from "./KuaishouAuthPage";
import { LiveLaunchPage } from "./LiveLaunchPage";
import { LiveRoomMonitorPage } from "./LiveRoomMonitorPage";
import { MateLoginPage } from "./MateLoginPage";
import { BindAuthorizePage } from "./BindAuthorizePage";
import { CommentListenerPage } from "./CommentListenerPage";
import { SubAccountsPage } from "./SubAccountsPage";
import { CAutoMessagePanel } from "./CAutoMessagePanel";
import { CAutoReplyPanel } from "./CAutoReplyPanel";
import { CScenePlayPanel } from "./CScenePlayPanel";
import { DAutoPopup } from "./DAutoPopup";
import { DProductScripts } from "./DProductScripts";
import { DShopHelper } from "./DShopHelper";
import { HuiboLivePage } from "./HuiboLivePage";
import { JinniuPromotePage } from "./JinniuPromotePage";

/**
 * 业务 section 壳 (design: 独立 section + 组内子导航).
 *
 * 各组按同一模式追加 tab (向 `BUSINESS_TABS` 加一项 + 一个页面组件).
 * E 组: `bind` / `huibo` / `jinniu`; C 组: `msg` / `reply` / `scene`;
 * D 组: `pscript` / `helper` / `popup`; A 组: `auth` / `mate` / `live` / `monitor`;
 * B 组: `comments` / `sub`.
 */
export type BusinessTabId =
  | "auth"
  | "mate"
  | "live"
  | "monitor"
  | "bind"
  | "huibo"
  | "jinniu"
  | "msg"
  | "reply"
  | "scene"
  | "pscript"
  | "helper"
  | "popup"
  | "comments"
  | "sub";

const BUSINESS_TABS: BusinessTabId[] = [
  "auth",
  "mate",
  "live",
  "monitor",
  "bind",
  "huibo",
  "jinniu",
  "msg",
  "reply",
  "scene",
  "pscript",
  "helper",
  "popup",
  "comments",
  "sub",
];

interface Props {
  profiles: ProfileSummary[];
}

export function BusinessSection({ profiles }: Props): JSX.Element {
  const t = useT();
  const [tab, setTab] = useState<BusinessTabId>("auth");

  const label = (id: BusinessTabId): string => {
    switch (id) {
      case "auth":
        return t("biz.auth.tab");
      case "mate":
        return t("biz.mate.tab");
      case "live":
        return t("biz.live.tab");
      case "monitor":
        return t("biz.monitor.tab");
      case "bind":
        return t("biz.bind.tab");
      case "huibo":
        return t("biz.huibo.tab");
      case "jinniu":
        return t("biz.jinniu.tab");
      case "msg":
        return t("biz.msg.tab");
      case "reply":
        return t("biz.reply.tab");
      case "scene":
        return t("biz.scene.tab");
      case "pscript":
        return t("biz.pscript.tab");
      case "helper":
        return t("biz.helper.tab");
      case "popup":
        return t("biz.popup.tab");
      case "comments":
        return t("biz.comments.tab");
      case "sub":
        return t("biz.sub.tab");
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex gap-1 px-6 pt-4 shrink-0" role="tablist" aria-label={t("nav.business")}>
        {BUSINESS_TABS.map((id) => (
          <button
            key={id}
            role="tab"
            aria-selected={tab === id}
            type="button"
            onClick={() => setTab(id)}
            className={cn(
              "px-3 h-8 rounded-[9px] text-[13px] font-medium transition-colors",
              tab === id
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
            )}
          >
            {label(id)}
          </button>
        ))}
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto">
        {tab === "auth" && <KuaishouAuthPage />}
        {tab === "mate" && <MateLoginPage />}
        {tab === "live" && <LiveLaunchPage />}
        {tab === "monitor" && <LiveRoomMonitorPage />}
        {tab === "bind" && <BindAuthorizePage profiles={profiles} />}
        {tab === "huibo" && <HuiboLivePage profiles={profiles} />}
        {tab === "jinniu" && <JinniuPromotePage profiles={profiles} />}
        {tab === "msg" && <CAutoMessagePanel />}
        {tab === "reply" && <CAutoReplyPanel />}
        {tab === "scene" && <CScenePlayPanel />}
        {tab === "pscript" && <DProductScripts />}
        {tab === "helper" && <DShopHelper profileId={profiles[0]?.id ?? ""} />}
        {tab === "popup" && <DAutoPopup profileId={profiles[0]?.id ?? ""} />}
        {tab === "comments" && <CommentListenerPage />}
        {tab === "sub" && <SubAccountsPage />}
      </div>
    </div>
  );
}
