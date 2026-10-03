import type { JSX } from "react";
import { Boxes } from "lucide-react";
import { Kbd } from "../atoms";
import { useT } from "../../i18n/LanguageProvider";

interface Props {
  onCreate: () => void;
}

export function ProfilesEmptyState({ onCreate }: Props): JSX.Element {
  const t = useT();
  return (
    <div className="flex-1 flex items-center justify-center" style={{ padding: 48 }}>
      <div
        className="text-center"
        style={{
          maxWidth: 420,
          padding: 32,
          borderRadius: 18,
          background: "rgba(255,255,255,0.02)",
          boxShadow:
            "inset 0 0 0 1px rgba(255,255,255,0.05), 0 24px 48px -16px rgba(0,0,0,0.5)",
        }}
      >
        <div
          className="mx-auto flex items-center justify-center"
          style={{
            width: 48,
            height: 48,
            borderRadius: 12,
            background: "var(--accent)",
            color: "var(--warning)",
            boxShadow: "inset 0 0 0 1px var(--ring)",
          }}
        >
          <Boxes size={22} strokeWidth={1.5} />
        </div>
        <div className="text-[14px] font-semibold text-slate-100 mt-3.5">{t("profile.empty.title")}</div>
        <div className="text-[12px] text-slate-500 mt-1.5 leading-relaxed">
          {t("profile.empty.body")}
        </div>
        <div className="flex items-center justify-center gap-2 mt-4">
          <button
            type="button"
            onClick={onCreate}
            className="btn-brand text-[12px] px-3.5 py-2 rounded-[10px]"
          >
            {t("profile.new.title")}
          </button>
          <Kbd>⌘ N</Kbd>
        </div>
      </div>
    </div>
  );
}
