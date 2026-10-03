import { useMemo, useState, type JSX } from "react";
import { Grid3x3, List, Plus } from "lucide-react";
import type { ProfileSummary, ActivityEvent } from "../../types";
import { Kbd } from "../atoms";
import { ProfileTile, deriveTileState, type TileData, type TileState } from "./ProfileTile";
import { ProfileTable } from "./ProfileTable";
import { usePersistedState } from "../../lib/persisted";
import { useT } from "../../i18n/LanguageProvider";
import { useScrollFade } from "../../lib/useScrollFade";
import { cn } from "../../lib/cn";
import { KuaishouIdentityToolbar } from "./KuaishouIdentity";
import { SubjectArchives } from "./SubjectArchives";
import { ProfilesEmptyState } from "./EmptyState";

type ViewMode = "grid" | "list";

interface FilterChip {
  id: "all" | TileState;
  kind?: TileState;
}

const FILTERS: FilterChip[] = [
  { id: "all" },
  { id: "running", kind: "running" },
  { id: "ai", kind: "ai" },
  { id: "error", kind: "error" },
  { id: "idle", kind: "idle" },
];

const FILTER_LABEL_KEYS = {
  all: "profile.filter.all",
  running: "profile.filter.running",
  ai: "profile.filter.aiDriven",
  error: "profile.filter.errors",
  idle: "profile.filter.idle",
} as const;

const DOT_COLOR: Record<TileState, string> = {
  running: "var(--success)",
  ai: "var(--warning)",
  error: "var(--destructive)",
  idle: "var(--muted-foreground)",
};

export type GroupFilter = "all" | "ungrouped" | string;

export interface GroupEntry {
  name: string | null;
  count: number;
}

function GroupTab({
  label,
  count,
  active,
  onSelect,
}: {
  label: string;
  count: number;
  active: boolean;
  onSelect: () => void;
}): JSX.Element {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "flex items-center gap-1.5 px-2.5 py-[5px] rounded-lg text-[12px] font-medium transition-colors border border-border",
        active ? "bg-accent text-accent-foreground" : "text-muted-foreground hover:text-slate-300",
      )}
    >
      {label}
      <span className="mono text-[10px] opacity-70">{count}</span>
    </button>
  );
}

interface Props {
  profiles: ProfileSummary[];
  recentEvents: ActivityEvent[];
  /** Controlled group filter. */
  groupFilter: GroupFilter;
  onGroupFilterChange: (group: GroupFilter) => void;
  /** Named groups with profile counts (from profiles_list_groups). */
  groups: GroupEntry[];
  /** Profiles in the terminating phase (winding down, not yet exited). */
  closingIds?: Set<string>;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onLaunch: (id: string) => void;
  onStop: (id: string) => void;
  onExport: (id: string) => void;
  onDelete: (id: string) => void;
}

export function Constellation({
  profiles,
  recentEvents,
  closingIds,
  groupFilter,
  onGroupFilterChange,
  groups,
  onSelect,
  onCreate,
  onLaunch,
  onStop,
  onExport,
  onDelete,
}: Props): JSX.Element {
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<FilterChip["id"]>("all");
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const [viewMode, setViewMode] = usePersistedState<ViewMode>("profilesView", "grid");
  const scrollRef = useScrollFade<HTMLDivElement>();
  const t = useT();

  const tileData: TileData[] = useMemo(
    () =>
      profiles.map((p) => ({
        ...p,
        ...deriveTileState(p, recentEvents),
      })),
    [profiles, recentEvents],
  );

  const counts = useMemo(() => {
    const c: Record<FilterChip["id"], number> = { all: tileData.length, running: 0, ai: 0, error: 0, idle: 0 };
    for (const t of tileData) c[t.state] += 1;
    return c;
  }, [tileData]);

  const allTags = useMemo(() => {
    const set = new Set<string>();
    for (const p of profiles) for (const t of p.tags) set.add(t);
    return Array.from(set).slice(0, 12);
  }, [profiles]);

  const ungroupedCount = useMemo(
    () => tileData.filter((t) => t.group == null).length,
    [tileData],
  );

  const filtered = useMemo(() => {
    return tileData.filter((t) => {
      const group = t.group;
      if (groupFilter === "ungrouped") {
        if (group != null) return false;
      } else if (groupFilter !== "all") {
        if (group !== groupFilter) return false;
      }
      if (filter !== "all" && t.state !== filter) return false;
      if (activeTag && !t.tags.includes(activeTag)) return false;
      if (search.trim() && ![t.name, t.id, ...t.tags].some(value => value.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()))) return false;
      return true;
    });
  }, [tileData, filter, activeTag, search, groupFilter]);

  const aiCount = counts.ai;

  return (
    <div className="flex-1 flex flex-col min-w-0 min-h-0">
      {/* Title row */}
      <div className="flex items-center gap-3.5 px-6 pt-4 pb-3">
        <div className="text-lg font-bold tracking-tight text-slate-100">{t("profile.list.allProfiles")}</div>
        <div className="mono text-[11px] text-slate-600">
          ·  {profiles.length} {t("nav.topbar.total")} · {counts.running + counts.ai} {t("nav.topbar.running")}
          {aiCount > 0 && ` · ${aiCount} ${t("profile.list.aiCount")}`}
        </div>
        <div className="flex-1" />
        <div
          className="flex gap-1 p-[3px] rounded-lg"
          style={{
            background: "rgba(255,255,255,0.03)",
            boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.06)",
          }}
        >
          <button
            type="button"
            onClick={() => setViewMode("grid")}
            className={cn(
              "w-7 h-6 rounded-md flex items-center justify-center transition-colors",
              viewMode === "grid"
                ? "text-slate-100"
                : "text-slate-500 hover:text-slate-300",
            )}
            style={{
              background: viewMode === "grid" ? "rgba(255,255,255,0.06)" : undefined,
            }}
            title={t("profile.list.gridView")}
          >
            <Grid3x3 size={13} strokeWidth={1.5} />
          </button>
          <button
            type="button"
            onClick={() => setViewMode("list")}
            className={cn(
              "w-7 h-6 rounded-md flex items-center justify-center transition-colors",
              viewMode === "list"
                ? "text-slate-100"
                : "text-slate-500 hover:text-slate-300",
            )}
            style={{
              background: viewMode === "list" ? "rgba(255,255,255,0.06)" : undefined,
            }}
            title={t("profile.list.listView")}
          >
            <List size={13} strokeWidth={1.5} />
          </button>
        </div>
        <button
          type="button"
          onClick={onCreate}
          className="btn-brand rounded-[9px] text-[12px] px-3 py-[7px]"
        >
          <Plus size={12} strokeWidth={2} />
          {t("profile.new.title")}
          <Kbd variant="on-brand">⌘ N</Kbd>
        </button>
      </div>

      {/* Group tabs row */}
      <div className="flex items-center gap-1.5 px-6 pb-2 flex-wrap">
        <GroupTab
          label={t("group.filter.all")}
          count={tileData.length}
          active={groupFilter === "all"}
          onSelect={() => onGroupFilterChange("all")}
        />
        {groups
          .filter((g) => g.name != null)
          .map((g) => (
            <GroupTab
              key={g.name}
              label={g.name as string}
              count={g.count}
              active={groupFilter === g.name}
              onSelect={() => onGroupFilterChange(g.name as string)}
            />
          ))}
        <GroupTab
          label={t("group.filter.ungrouped")}
          count={ungroupedCount}
          active={groupFilter === "ungrouped"}
          onSelect={() => onGroupFilterChange("ungrouped")}
        />
      </div>

      {/* Filter row */}
      <div className="flex items-center gap-1.5 px-6 pb-3.5 flex-wrap">
        {FILTERS.map((c) => {
          const isActive = filter === c.id;
          return (
            <button
              key={c.id}
              type="button"
              onClick={() => setFilter(c.id)}
              className={cn(
                "flex items-center gap-1.5 px-2.5 py-[5px] rounded-lg text-[12px] font-medium transition-colors",
                isActive ? "text-slate-100" : "text-slate-500 hover:text-slate-300",
              )}
              style={{
                background: isActive ? "rgba(255,255,255,0.06)" : undefined,
                boxShadow: isActive ? "inset 0 0 0 1px rgba(255,255,255,0.08)" : undefined,
              }}
            >
              {c.kind && (
                <span
                  className="w-[5px] h-[5px] rounded-full"
                  style={{ background: DOT_COLOR[c.kind] }}
                />
              )}
              {t(FILTER_LABEL_KEYS[c.id])}
              <span className={cn("mono text-[10px]", isActive ? "text-slate-400" : "text-slate-600")}>
                {counts[c.id]}
              </span>
            </button>
          );
        })}

        {allTags.length > 0 && (
          <>
            <div className="w-px h-4 bg-white/[0.06] mx-1.5" />
            {allTags.map((t) => {
              const isActive = activeTag === t;
              return (
                <button
                  key={t}
                  type="button"
                  onClick={() => setActiveTag(isActive ? null : t)}
                  className={cn(
                    "mz-pill mono cursor-pointer transition-colors",
                    isActive ? "text-accent-foreground" : "text-slate-500 hover:text-slate-300",
                  )}
                  style={{
                    background: isActive ? "var(--accent)" : "rgba(255,255,255,0.03)",
                    boxShadow: isActive
                      ? "inset 0 0 0 1px var(--ring)"
                      : "inset 0 0 0 1px rgba(255,255,255,0.05)",
                  }}
                >
                  {t}
                </button>
              );
            })}
          </>
        )}
      </div>

      <div className="px-6 pb-3">
        <input aria-label={t("profile.list.searchAria")} placeholder={t("profile.list.searchPlaceholder")} maxLength={128}
          value={search} onChange={event => setSearch(event.target.value)} autoComplete="off"
          className="w-full rounded-lg border border-white/10 bg-white/5 p-2 text-sm text-slate-200" />
      </div>
      <KuaishouIdentityToolbar />

      {/* Body — grid or list. `pt-3` keeps the running/AI glow from
          getting clipped against the top edge of the scroll container
          (box-shadow extends ~32px outside the tile). */}
      <div ref={scrollRef} className="flex-1 overflow-auto scroll-fade px-6 pb-6 pt-3">
        {profiles.length === 0 ? <ProfilesEmptyState onCreate={onCreate} /> : filtered.length === 0 ? (
          <div className="text-sm text-slate-500 py-12 text-center">
            {t("profile.list.noFilterMatch")}
          </div>
        ) : viewMode === "list" ? (
          <ProfileTable
            profiles={filtered}
            closingIds={closingIds}
            onSelect={onSelect}
            onLaunch={onLaunch}
            onStop={onStop}
            onExport={onExport}
            onDelete={onDelete}
          />
        ) : (
          <div
            className="grid gap-3.5"
            style={{
              gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
            }}
          >
            {filtered.map((p) => (
              <ProfileTile
                key={p.id}
                profile={p}
                terminating={closingIds?.has(p.id) ?? false}
                onOpen={() => onSelect(p.id)}
                onLaunch={() => onLaunch(p.id)}
                onStop={() => onStop(p.id)}
                onExport={() => onExport(p.id)}
                onDelete={() => onDelete(p.id)}
              />
            ))}
          </div>
        )}
        <SubjectArchives key={search.trim()} search={search.trim()}
          profilesRevision={JSON.stringify(profiles.map(profile => [profile.id, profile.name]))} />
      </div>
    </div>
  );
}
