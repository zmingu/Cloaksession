import type { JSX } from "react";
import { Boxes, Command, Plug, Settings } from "lucide-react";
import { cn } from "../../lib/cn";
import { useT } from "../../i18n/LanguageProvider";

export type Section = "profiles" | "mcp" | "settings";

interface Item {
  id: Section;
  icon: typeof Boxes;
  kbd: string;
}

const ITEMS: Item[] = [
  { id: "profiles", icon: Boxes, kbd: "1" },
  { id: "mcp", icon: Plug, kbd: "2" },
  { id: "settings", icon: Settings, kbd: "," },
];

interface Props {
  active: Section;
  onChange: (s: Section) => void;
  onCmdK: () => void;
}

export function LeftRail({ active, onChange, onCmdK }: Props): JSX.Element {
  const t = useT();
  const navLabel = (id: Section): string =>
    id === "profiles" ? t("nav.profiles") : id === "settings" ? t("nav.settings") : "MCP";
  return (
    <div
      className="flex flex-col items-center pt-3.5 gap-1.5 flex-shrink-0"
      style={{
        width: 56,
        background: "rgba(255,255,255,0.01)",
        borderRight: "1px solid rgba(255,255,255,0.04)",
      }}
    >
      {ITEMS.map((it) => {
        const Icon = it.icon;
        const isActive = active === it.id;
        return (
          <button
            key={it.id}
            type="button"
            title={t("nav.item.shortcutTitle", { label: navLabel(it.id), kbd: it.kbd })}
            onClick={() => onChange(it.id)}
            className={cn(
              "w-9 h-9 rounded-[10px] flex items-center justify-center transition-colors",
              isActive ? "text-accent-foreground" : "text-muted-foreground hover:text-foreground hover:bg-accent",
            )}
            style={{
              background: isActive ? "var(--accent)" : undefined,
              boxShadow: isActive ? "inset 0 0 0 1px var(--ring)" : undefined,
            }}
          >
            <Icon size={16} strokeWidth={1.5} />
          </button>
        );
      })}
      <div className="flex-1" />
      <button
        type="button"
        title={t("nav.commandPalette.title")}
        onClick={onCmdK}
        className="mb-3.5 w-9 h-9 rounded-[10px] flex items-center justify-center text-slate-600 hover:text-slate-300 hover:bg-white/5 transition-colors"
      >
        <Command size={16} strokeWidth={1.5} />
      </button>
    </div>
  );
}
