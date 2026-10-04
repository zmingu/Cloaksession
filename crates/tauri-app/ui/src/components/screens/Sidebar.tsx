import type { JSX } from "react";
import { Boxes, Briefcase, Command, Plug, Settings } from "lucide-react";
import { cn } from "../../lib/cn";
import type { ProfileGroup } from "../../types";
import { useT } from "../../i18n/LanguageProvider";

export type Section = "profiles" | "mcp" | "business" | "settings";

/** Sidebar group filter — mirrors `Constellation.GroupFilter` structurally. */
export type GroupFilter = "all" | "ungrouped" | (string & {});

interface NavItem {
  id: Section;
  icon: typeof Boxes;
  kbd: string;
}

const NAV: NavItem[] = [
  { id: "profiles", icon: Boxes, kbd: "1" },
  { id: "mcp", icon: Plug, kbd: "2" },
  { id: "business", icon: Briefcase, kbd: "3" },
  { id: "settings", icon: Settings, kbd: "," },
];

interface Props {
  active: Section;
  onChange: (s: Section) => void;
  onCmdK: () => void;
  groups: ProfileGroup[];
  groupFilter: GroupFilter;
  onGroupFilterChange: (g: GroupFilter) => void;
  onDeleteGroup?: (name: string) => void;
}

export function Sidebar({
  active,
  onChange,
  onCmdK,
  groups,
  groupFilter,
  onGroupFilterChange,
  onDeleteGroup,
}: Props): JSX.Element {
  const t = useT();
  const navLabel = (id: Section): string =>
    id === "profiles"
      ? t("nav.profiles")
      : id === "settings"
        ? t("nav.settings")
        : id === "business"
          ? t("nav.business")
          : "MCP";
  const namedGroups = groups.filter((g) => g.name != null);
  const ungrouped = groups.find((g) => g.name == null);
  const ungroupedCount = ungrouped?.count ?? 0;
  const totalCount = groups.reduce((n, g) => n + g.count, 0);

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
          return (
            <button
              key={it.id}
              type="button"
              title={t("nav.item.shortcutTitle", { label: navLabel(it.id), kbd: it.kbd })}
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

      <div className="flex-1" />
      <div className="px-3">
        <button
          type="button"
          title={t("nav.commandPalette.title")}
          onClick={onCmdK}
          className={cn(
            "flex items-center gap-2.5 w-full px-2.5 h-8 rounded-[9px] text-[13px] font-medium transition-colors",
            "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
          )}
        >
          <Command size={15} strokeWidth={1.5} />
          {t("nav.commandPalette")}
        </button>
      </div>
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