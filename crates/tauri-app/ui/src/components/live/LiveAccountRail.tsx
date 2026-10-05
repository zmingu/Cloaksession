import type { JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { cn } from "../../lib/cn";
import { Pill } from "../atoms/Pill";

/**
 * 「直播 › 直播互动」账号侧栏（账号工作台左侧）。
 *
 * 账号维度的键 = `profileId`（浏览器环境 = 快手小店账号）。两个 tab 共用，
 * 只负责「选谁」，不关心右侧挂哪些模块。
 *
 * 视觉与 Sidebar 一致：选中态 `bg-accent text-accent-foreground`。
 */
export interface LiveAccountRailItem {
  id: string;
  name: string;
  live: boolean;
}

interface Props {
  items: LiveAccountRailItem[];
  selectedId: string;
  onSelect: (id: string) => void;
  /** 每个账号的模块运行指示（可选，小圆点）。 */
  activity?: Record<string, { label: string; tone: "running" | "pending" | "error" }[]>;
  emptyText: string;
  title: string;
}

/** 活动小圆点配色（与 Pill 同源）。 */
const TONE_COLOR: Record<"running" | "pending" | "error", string> = {
  running: "#34d399",
  pending: "#fbbf24",
  error: "#f87171",
};

export function LiveAccountRail(props: Props): JSX.Element {
  const { items, selectedId, onSelect, activity, emptyText, title } = props;
  const t = useT();

  return (
    <aside
      aria-label={title}
      data-testid="live-account-rail"
      className="w-[232px] shrink-0 flex flex-col min-h-0 border-r border-white/5"
    >
      <h2 className="shrink-0 px-3 pt-4 pb-2 text-[12px] font-semibold text-slate-400">{title}</h2>

      {items.length === 0 ? (
        <p className="px-3 text-[12px] text-slate-500">{emptyText}</p>
      ) : (
        <ul className="flex-1 min-h-0 overflow-y-auto px-2 pb-3 space-y-0.5">
          {items.map((item) => {
            const active = item.id === selectedId;
            const dots = activity?.[item.id] ?? [];
            return (
              <li key={item.id}>
                <button
                  type="button"
                  data-testid={`live-account-${item.id}`}
                  aria-current={active}
                  onClick={() => onSelect(item.id)}
                  className={cn(
                    "w-full min-w-0 flex items-center gap-2 px-2.5 h-9 rounded-[9px] text-[13px] text-left transition-colors",
                    active
                      ? "bg-accent text-accent-foreground"
                      : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                  )}
                >
                  <span className="flex-1 min-w-0 truncate">{item.name}</span>

                  {dots.length > 0 && (
                    <span className="flex items-center gap-1 shrink-0">
                      {dots.map((dot, idx) => (
                        <span
                          key={`${dot.label}-${idx}`}
                          title={dot.label}
                          aria-hidden
                          className="w-1.5 h-1.5 rounded-full"
                          style={{ background: TONE_COLOR[dot.tone] }}
                        />
                      ))}
                    </span>
                  )}

                  {item.live && <Pill kind="running">{t("live.accounts.live")}</Pill>}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </aside>
  );
}
