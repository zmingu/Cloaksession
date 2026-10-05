import { useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { cn } from "../../lib/cn";
import type { ProfileSummary } from "../../types";
import { BindAuthorizePage } from "../business/BindAuthorizePage";
import { HuiboLivePage } from "../business/HuiboLivePage";
import { JinniuPromotePage } from "../business/JinniuPromotePage";
import { LiveLaunchPage } from "../business/LiveLaunchPage";
import { LiveInteractPage } from "./LiveInteractPage";

/**
 * 直播 section 壳。
 *
 * 三个页签（开播准备 / 直播互动 / 投流）：
 *   开播准备 → 慧播开播 + 伴侣开播；
 *   直播互动 → LiveInteractPage（三个堆叠子区）；
 *   投流     → 金牛推广 + 达人授权。
 * 选中态是本地 state（不需要持久化）。
 */
export type LiveTabId = "prepare" | "interact" | "ads";

const LIVE_TABS: LiveTabId[] = ["prepare", "interact", "ads"];

interface Props {
  profiles: ProfileSummary[];
}

export function LiveSection({ profiles }: Props): JSX.Element {
  const t = useT();
  const [tab, setTab] = useState<LiveTabId>("prepare");

  const label = (id: LiveTabId): string => {
    switch (id) {
      case "prepare":
        return t("live.tab.prepare");
      case "interact":
        return t("live.tab.interact");
      case "ads":
        return t("live.tab.ads");
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex gap-1 px-6 pt-4 shrink-0" role="tablist" aria-label={t("nav.live")}>
        {LIVE_TABS.map((id) => (
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
        {tab === "prepare" && (
          <div className="flex flex-col min-w-0">
            <section data-testid="live-prepare-huibo" className="flex flex-col min-w-0">
              <h2 className="px-6 pt-5 text-[13px] font-semibold text-slate-400">
                {t("live.prepare.huibo")}
              </h2>
              <HuiboLivePage profiles={profiles} />
            </section>
            <section
              data-testid="live-prepare-mate"
              className="flex flex-col min-w-0 border-t border-white/5"
            >
              <h2 className="px-6 pt-5 text-[13px] font-semibold text-slate-400">
                {t("live.prepare.mate")}
              </h2>
              <LiveLaunchPage />
            </section>
          </div>
        )}
        {tab === "interact" && <LiveInteractPage profiles={profiles} />}
        {tab === "ads" && (
          <div className="flex flex-col min-w-0">
            <JinniuPromotePage profiles={profiles} />
            <BindAuthorizePage profiles={profiles} />
          </div>
        )}
      </div>
    </div>
  );
}
