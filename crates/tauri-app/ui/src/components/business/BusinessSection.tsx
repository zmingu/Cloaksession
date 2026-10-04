import { useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { cn } from "../../lib/cn";
import type { ProfileSummary } from "../../types";
import { BindAuthorizePage } from "./BindAuthorizePage";
import { HuiboLivePage } from "./HuiboLivePage";
import { JinniuPromotePage } from "./JinniuPromotePage";

/**
 * 业务 section 壳 (design: 独立 section + 组内子导航).
 *
 * B 组注册 `comments` / `sub` 两个入口; A/C/D/E 组按同一模式追加 tab
 * (向 `BUSINESS_TABS` 加一项 + 一个页面组件), 无需改动壳结构.
 * E 组追加 `bind` / `huibo` / `jinniu` (达人授权 / 跟播回播 / 金牛推广).
 */
export type BusinessTabId = "bind" | "huibo" | "jinniu";

const BUSINESS_TABS: BusinessTabId[] = ["bind", "huibo", "jinniu"];

interface Props {
  profiles: ProfileSummary[];
}

export function BusinessSection({ profiles }: Props): JSX.Element {
  const t = useT();
  const [tab, setTab] = useState<BusinessTabId>("bind");

  const label = (id: BusinessTabId): string => {
    switch (id) {
      case "bind":
        return t("biz.bind.tab");
      case "huibo":
        return t("biz.huibo.tab");
      case "jinniu":
        return t("biz.jinniu.tab");
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="flex gap-1 px-6 pt-4" role="tablist" aria-label={t("nav.business")}>
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
      {tab === "bind" && <BindAuthorizePage profiles={profiles} />}
      {tab === "huibo" && <HuiboLivePage profiles={profiles} />}
      {tab === "jinniu" && <JinniuPromotePage profiles={profiles} />}
    </div>
  );
}
