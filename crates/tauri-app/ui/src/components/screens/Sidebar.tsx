import { useEffect, useState, type JSX } from "react";
import { UserCheck, Users, Boxes, ChevronRight, Coins, MessageCircle, MessagesSquare, Plug, Settings, ShoppingBag, Store, Video } from "lucide-react";
import { cn } from "../../lib/cn";
import type { ProfileGroup } from "../../types";
import { useT } from "../../i18n/LanguageProvider";

export type Section = "profiles" | "kuaishou" | "jinniu" | "mcp" | "business" | "settings";

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

export type JinniuTab = "accounts" | "authorize";

interface JinniuNavItem {
  id: JinniuTab;
  icon: typeof Boxes;
}

export const JINNIU_TABS: JinniuNavItem[] = [
  { id: "accounts", icon: Users },
  { id: "authorize", icon: UserCheck },
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
  jinniuTab: JinniuTab;
  onJinniuTabChange: (tab: JinniuTab) => void;
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
  jinniuTab,
  onJinniuTabChange,
}: Props): JSX.Element {
  const t = useT();
  const navLabel = (id: Section): string =>
    id === "profiles"
      ? t("nav.profiles")
      : id === "kuaishou"
        ? t("nav.kuaishou")
        : id === "jinniu"
          ? t("nav.jinniu")
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
  const jinniuLabel = (id: JinniuTab): string =>
    id === "accounts"
      ? t("jinniu.tab.accounts")
      : t("jinniu.tab.authorize");
  const namedGroups = groups.filter((g) => g.name != null);
  const ungrouped = groups.find((g) => g.name == null);
  const ungroupedCount = ungrouped?.count ?? 0;
  const totalCount = groups.reduce((n, g) => n + g.count, 0);
  // 子菜单的展开/折叠状态：进入对应板块时自动展开，也可手动收起（快手账号 / 磁力金牛账号）。
  const [kuaishouOpen, setKuaishouOpen] = useState(true);
  const [jinniuOpen, setJinniuOpen] = useState(true);
  useEffect(() => {
    if (active === "kuaishou") setKuaishouOpen(true);
    if (active === "jinniu") setJinniuOpen(true);
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
          if (it.id === "kuaishou") {
            return (
              <NavGroup
                key={it.id}
                icon={Icon}
                label={navLabel(it.id)}
                active={isActive}
                open={kuaishouOpen}
                onParentClick={() => {
                  if (active === "kuaishou") {
                    setKuaishouOpen((v) => !v);
                  } else {
                    onChange(it.id);
                    setKuaishouOpen(true);
                  }
                }}
                items={KUAISHOU_TABS.map((tab) => ({
                  id: tab.id,
                  icon: tab.icon,
                  label: kuaishouLabel(tab.id),
                }))}
                activeChildId={kuaishouTab}
                onChildClick={(id) => {
                  onChange("kuaishou");
                  onKuaishouTabChange(id as KuaishouTab);
                }}
              />
            );
          }
          if (it.id === "jinniu") {
            return (
              <NavGroup
                key={it.id}
                icon={Icon}
                label={navLabel(it.id)}
                active={isActive}
                open={jinniuOpen}
                onParentClick={() => {
                  if (active === "jinniu") {
                    setJinniuOpen((v) => !v);
                  } else {
                    onChange(it.id);
                    setJinniuOpen(true);
                  }
                }}
                items={JINNIU_TABS.map((tab) => ({
                  id: tab.id,
                  icon: tab.icon,
                  label: jinniuLabel(tab.id),
                }))}
                activeChildId={jinniuTab}
                onChildClick={(id) => {
                  onChange("jinniu");
                  onJinniuTabChange(id as JinniuTab);
                }}
              />
            );
          }
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

interface NavChildItem {
  id: string;
  icon: typeof Boxes;
  label: string;
}

/**
 * 可展开的板块父项：父项可点击展开/折叠 + 缩进二级子菜单。
 * 快手账号 / 磁力金牛账号 两个带子页的板块共用。
 */
function NavGroup({
  icon: Icon,
  label,
  active,
  open,
  onParentClick,
  items,
  activeChildId,
  onChildClick,
}: {
  icon: typeof Boxes;
  label: string;
  active: boolean;
  open: boolean;
  onParentClick: () => void;
  items: NavChildItem[];
  activeChildId: string;
  onChildClick: (id: string) => void;
}): JSX.Element {
  return (
    <div className="flex flex-col gap-0.5">
      <button
        type="button"
        title={label}
        aria-expanded={open}
        onClick={onParentClick}
        className={cn(
          "flex items-center gap-2.5 px-2.5 h-8 rounded-[9px] text-[13px] font-medium transition-colors",
          active
            ? "bg-accent text-accent-foreground"
            : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
        )}
      >
        <Icon size={15} strokeWidth={1.5} />
        {label}
        <ChevronRight
          size={14}
          strokeWidth={2}
          className={cn("ml-auto transition-transform", open && "rotate-90")}
        />
      </button>
      {open && (
        <div className="flex flex-col gap-0.5 ml-4 pl-2 border-l border-white/10">
          {items.map((item) => {
            const ChildIcon = item.icon;
            const childActive = active && item.id === activeChildId;
            return (
              <button
                key={item.id}
                type="button"
                title={item.label}
                onClick={() => onChildClick(item.id)}
                className={cn(
                  "flex items-center gap-2 px-2.5 h-7 rounded-[8px] text-[12px] font-medium transition-colors text-left",
                  childActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                )}
              >
                <ChildIcon size={13} strokeWidth={1.5} />
                {item.label}
              </button>
            );
          })}
        </div>
      )}
    </div>
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