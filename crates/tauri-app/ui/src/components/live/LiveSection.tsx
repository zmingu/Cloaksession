import { type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { cn } from "../../lib/cn";
import { usePersistedState } from "../../lib/persisted";
import type { ProfileSummary } from "../../types";
import { HuiboLivePage } from "../business/HuiboLivePage";
import { JinniuPromotePage } from "../business/JinniuPromotePage";
import { LiveLaunchPage } from "../business/LiveLaunchPage";
import { LiveAccountRail } from "./LiveAccountRail";
import { LiveInteractPage } from "./LiveInteractPage";
import { LiveScriptPage } from "./LiveScriptPage";
import { useLiveAccounts } from "./useLiveAccounts";

export type LiveTab = "prepare" | "live" | "interact" | "ads";

const LIVE_TABS: LiveTab[] = ["prepare", "live", "interact", "ads"];

/**
 * 直播板块壳（已**归档**到「其它」板块下，待重新设计）。
 *
 * 四个页面（开播准备 / 正式开播 / 直播互动 / 投流）由本组件顶部的页签切换，
 * 选中态持久化在 localStorage（`multizen.ui.liveTab`）。
 * 统一按**账号维度**组织：
 *   开播准备 → 左账号侧栏（LiveAccountRail）+ 右脚本处理
 *              （LiveScriptPage，多视频多脚本：每个卖货视频一份弹幕脚本）；
 *   正式开播 → 左账号侧栏 + 右「该账号的开播控制」：
 *              慧播开播（该 profile 的录播开播）+ 伴侣开播（独立子区块）；
 *   直播互动 → LiveInteractPage（账号工作台：左账号侧栏 + 右互动模块三组）；
 *   投流     → 金牛推广（达人授权已移至「磁力金牛账号」的二级菜单）。
 *
 * 账号状态（列表 + 在播标记）由 `useLiveAccounts` 单次订阅后下传，prepare /
 * live / interact 三页共用同一份，保证 `live-launch-state-changed` 只有一处订阅。
 */

interface Props {
  profiles: ProfileSummary[];
}

export function LiveSection({ profiles }: Props): JSX.Element {
  const t = useT();
  const [tab, setTab] = usePersistedState<LiveTab>("liveTab", "prepare");
  // 账号状态：列表 + 在播标记；prepare / live / interact 三页共用（唯一订阅处）。
  const accounts = useLiveAccounts(profiles);

  const label = (id: LiveTab): string => {
    switch (id) {
      case "prepare":
        return t("live.tab.prepare");
      case "live":
        return t("live.tab.live");
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
          // 开播准备：账号维度 —— 左账号侧栏 + 右脚本处理（多视频多脚本）。
          <div data-testid="live-prepare-page" className="h-full flex min-w-0 min-h-0">
            <LiveAccountRail
              items={accounts.items}
              selectedId={accounts.selectedId}
              onSelect={accounts.setSelectedId}
              emptyText={t("live.accounts.empty")}
              title={t("live.accounts.title")}
            />

            <div className="flex-1 min-w-0 min-h-0 overflow-y-auto" key={accounts.selectedId}>
              <LiveScriptPage />
            </div>
          </div>
        )}
        {tab === "live" && (
          // 正式开播：账号维度 —— 左账号侧栏 + 右「该账号的开播控制」。
          <div data-testid="live-live-page" className="h-full flex min-w-0 min-h-0">
            <LiveAccountRail
              items={accounts.items}
              selectedId={accounts.selectedId}
              onSelect={accounts.setSelectedId}
              emptyText={t("live.accounts.empty")}
              title={t("live.accounts.title")}
            />

            <div className="flex-1 min-w-0 min-h-0 overflow-y-auto" key={accounts.selectedId}>
              <section data-testid="live-live-huibo" className="flex flex-col min-w-0">
                <h2 className="px-6 pt-5 text-[13px] font-semibold text-slate-400">
                  {t("live.prepare.huibo")}
                </h2>
                <HuiboLivePage profileId={accounts.selectedId} />
              </section>

              <section
                data-testid="live-live-mate"
                className="flex flex-col min-w-0 border-t border-white/5"
              >
                <h2 className="px-6 pt-5 text-[13px] font-semibold text-slate-400">
                  {t("live.prepare.mate")}
                </h2>
                {/* 伴侣开播独立于浏览器环境：mate 账号与 profile 是两套命名空间（组件内自述）。 */}
                <LiveLaunchPage />
              </section>
            </div>
          </div>
        )}
        {tab === "interact" && <LiveInteractPage accounts={accounts} />}
        {tab === "ads" && <JinniuPromotePage profiles={profiles} />}
      </div>
    </div>
  );
}
