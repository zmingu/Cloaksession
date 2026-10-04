import type { JSX } from "react";
import { Boxes, Plug, Settings } from "lucide-react";
import { cn } from "../../lib/cn";
import { useT } from "../../i18n/LanguageProvider";
import type { Section } from "./Sidebar";

interface Item {
  id: Section;
  icon: typeof Boxes;
}

const ITEMS: Item[] = [
  { id: "profiles", icon: Boxes },
  { id: "mcp", icon: Plug },
  { id: "settings", icon: Settings },
];

interface Props {
  active: Section;
  onChange: (s: Section) => void;
}

export function LeftRail({ active, onChange }: Props): JSX.Element {
  const t = useT();
  const navLabel = (id: Section): string =>
    id === "profiles"
      ? t("nav.profiles")
      : id === "kuaishou"
        ? t("nav.kuaishou")
        : id === "settings"
          ? t("nav.settings")
          : id === "business"
            ? t("nav.business")
            : "MCP";
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
            title={navLabel(it.id)}
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
    </div>
  );
}
