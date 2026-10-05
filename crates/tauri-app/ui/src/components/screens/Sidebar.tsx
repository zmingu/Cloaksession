import { useEffect, useState, type JSX } from "react";
import { Boxes, ChevronRight, Coins, MessageCircle, MessagesSquare, Plug, Radio, Settings, ShoppingBag, Store, Video } from "lucide-react";
import { cn } from "../../lib/cn";
import type { ProfileGroup } from "../../types";
import { useT } from "../../i18n/LanguageProvider";

export type Section = "profiles" | "kuaishou" | "jinniu" | "live" | "mcp" | "business" | "settings";

export type KuaishouTab = "shop" | "interact" | "mate";

interface KuaishouNavItem {
  id: KuaishouTab;
  icon: typeof Boxes;
}

export const KUAISHOU_TABS: KuaishouNavItem[] = [
  { id: "shop", icon: ShoppingBag },
  { id: "mate", icon: Video },
  { id: "interact", icon: MessageCircle },
];

/** Sidebar group filter — mirrors `Constellation.GroupFilter` structurally. */
export type GroupFilter = "all" | "ungrouped" | (string & {});

interface NavItem {
  id: Section;
  icon: typeof Boxes;
}

const NAV: NavItem[] = [
  { id: "kuaishou", icon: Store },
  { id: "jinniu", icon: Coins },
  { id: "live", icon: Radio },
  { id: "profiles", icon: Boxes },
  { id: "mcp", icon: Plug },
  { id: "business", icon: MessagesSquare },
  { id: "settings", icon: Settings },
];

interface Props {
  active: Section;
  onChange: (s: Section) => void;
  groups: ProfileGroup[];
  groupFilter: GroupFilter;
  onGroupFilterChange: (g: GroupFilter) => void;
  onDeleteGroup?: (name: string) => void;
  kuaishouTab: KuaishouTab;
  onKuaishouTabChange: (tab: KuaishouTab) => void;
}

export function Sidebar({
  active,
  onChange,
  groups,
  groupFilter,
  onGroupFilterChange,
  onDeleteGroup,
  kuaishouTab,
  onKuaishouTabChange,
}: Props): JSX.Element {
  const t = useT();
  const navLabel = (id: Section): string =>
    id === "profiles"
      ? t("nav.profiles")
      : id === "kuaishou"
        ? t("nav.kuaishou")
        : id === "jinniu"
          ? t("nav.jinniu")
          : id === "live"
            ? t("nav.live")
            : id === "settings"
              ? t("nav.settings")
              : id === "business"
                ? t("nav.business")
                : "MCP";
  const kuaishouLabel = (id: KuaishouTab): string =>
    id === "shop"
      ? t("kuaishou.tab.shop")
      : id === "interact"
        ? t("kuaishou.tab.interact")
        : t("kuaishou.tab.mate");
  const namedGroups = groups.filter((g) => g.name != null);
  const ungrouped = groups.find((g) => g.name == null);
  const ungroupedCount = ungrouped?.count ?? 0;
  const totalCount = groups.reduce((n, g) => n + g.count, 0);
  // 快手子菜单的展开/折叠状态：进入快手板块时自动展开，也可手动收起。
  const [kuaishouOpen, setKuaishouOpen] = useState(true);
  useEffect(() => {
    if (active === "kuaishou") setKuaishouOpen(true);
  }, [active]);

  return (
    <nav
      className="flex flex-col flex-shrink-0 py-3.5"
      style={{
        width: 220,
        background: "var(--sidebar)",
        borderRight: "1px solid var(--sidebar-border)",
        color: "var(--sidebar-foreground)",
      }}
    >
      <div className="flex flex-col gap-1 px-3">
        {NAV.map((it) => {
          const Icon = it.icon;
          const isActive = active === it.id;
          if (it.id !== "kuaishou") {
            return (
              <button
                key={it.id}
                type="button"
                title={navLabel(it.id)}
                onClick={() => onChange(it.id)}
                className={cn(
                  "flex items-center gap-2.5 px-2.5 h-8 rounded-[9px] text-[13px] font-medium transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
              >
                <Icon size={15} strokeWidth={1.5} />
                {navLabel(it.id)}
              </button>
            );
          }
          // 快手：父项可点击展开/折叠 + 缩进子菜单
          return (
            <div key={it.id} className="flex flex-col gap-0.5">
              <button
                type="button"
                title={navLabel(it.id)}
                aria-expanded={kuaishouOpen}
                onClick={() => {
                  if (active === "kuaishou") {
                    setKuaishouOpen((v) => !v);
                  } else {
                    onChange(it.id);
                    setKuaishouOpen(true);
                  }
                }}
                className={cn(
                  "flex items-center gap-2.5 px-2.5 h-8 rounded-[9px] text-[13px] font-medium transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
              >
                <Icon size={15} strokeWidth={1.5} />
                {navLabel(it.id)}
                <ChevronRight
                  size={14}
                  strokeWidth={2}
                  className={cn("ml-auto transition-transform", kuaishouOpen && "rotate-90")}
                />
              </button>
              {kuaishouOpen && (
                <div className="flex flex-col gap-0.5 ml-4 pl-2 border-l border-white/10">
                  {KUAISHOU_TABS.map((tab) => {
                    const TabIcon = tab.icon;
                    const tabActive = isActive && kuaishouTab === tab.id;
                    return (
                      <button
                        key={tab.id}
                        type="button"
                        title={kuaishouLabel(tab.id)}
                        onClick={() => {
                          onChange("kuaishou");
                          onKuaishouTabChange(tab.id);
                        }}
                        className={cn(
                          "flex items-center gap-2 px-2.5 h-7 rounded-[8px] text-[12px] font-medium transition-colors text-left",
                          tabActive
                            ? "bg-accent text-accent-foreground"
                            : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                        )}
                      >
                        <TabIcon size={13} strokeWidth={1.5} />
                        {kuaishouLabel(tab.id)}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {active === "profiles" && (
        <div className="mt-4 px-3">
          <div className="px-2.5 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("nav.groups")}
          </div>
          <div className="flex flex-col gap-0.5">
            <GroupRow
              label={t("group.filter.all")}
              count={totalCount}
              active={groupFilter === "all"}
              onSelect={() => onGroupFilterChange("all")}
            />
            {namedGroups.map((g) => (
              <GroupRow
                key={g.name as string}
                label={g.name as string}
                count={g.count}
                active={groupFilter === g.name}
                onSelect={() => onGroupFilterChange(g.name as string)}
                onDelete={onDeleteGroup}
              />
            ))}
            <GroupRow
              label={t("group.filter.ungrouped")}
              count={ungroupedCount}
              active={groupFilter === "ungrouped"}
              onSelect={() => onGroupFilterChange("ungrouped")}
            />
          </div>
        </div>
      )}

    </nav>
  );
}

function GroupRow({
  label,
  count,
  active,
  onSelect,
  onDelete,
}: {
  label: string;
  count: number;
  active: boolean;
  onSelect: () => void;
  onDelete?: (name: string) => void;
}): JSX.Element {
  const t = useT();
  return (
    <div
      className={cn(
        "group flex items-center rounded-lg transition-colors",
        active ? "bg-accent" : "hover:bg-accent",
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        className={cn(
          "flex-1 flex items-center gap-1.5 px-2.5 h-7 text-[12px] font-medium transition-colors text-left",
          active
            ? "text-accent-foreground"
            : "text-muted-foreground hover:text-accent-foreground",
        )}
      >
        <span className="flex-1 truncate">{label}</span>
        <span className="mono text-[10px] opacity-70">{count}</span>
      </button>
      {onDelete && (
        <button
          type="button"
          title={t("group.delete.title", { label })}
          onClick={() => onDelete(label)}
          className="opacity-0 group-hover:opacity-100 px-1.5 mr-1 text-muted-foreground hover:text-destructive transition-opacity"
          aria-label={t("group.delete.aria", { label })}
        >
          ×
        </button>
      )}
    </div>
  );
}