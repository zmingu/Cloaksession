import { useEffect, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import { ChromixFingerprintForm } from "./ChromixFingerprintForm";

interface Props {
  options: Record<string, unknown>;
  onChange: (options: Record<string, unknown>) => void;
}

export function ChromixProfileOptions({ options, onChange }: Props): JSX.Element {
  const t = useT();
  const [json, setJson] = useState(() => JSON.stringify(options, null, 2));
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setJson(JSON.stringify(options, null, 2));
    setError(null);
  }, [options]);

  function applyJson(): void {
    try {
      const parsed: unknown = JSON.parse(json);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error(t("chromix.profileOptions.jsonObject"));
      }
      onChange(parsed as Record<string, unknown>);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div className="min-w-0 space-y-4">
      <div className="text-[12px] text-slate-400 leading-relaxed">
        {t("chromix.profileOptions.intro")}
      </div>
      <ChromixFingerprintForm options={options} onChange={onChange} />
      <details className="rounded-lg border border-white/10 p-3">
        <summary className="cursor-pointer text-[12px] text-slate-300">{t("chromix.profileOptions.title")}</summary>
        <p className="my-2 text-[11px] text-slate-500">
          {t("chromix.profileOptions.desc")}
        </p>
        <textarea
          aria-label={t("chromix.profileOptions.aria")}
          className="h-40 w-full min-w-0 resize-y rounded-lg bg-white/5 p-2 font-mono text-[12px] text-slate-200 sm:h-56"
          rows={12}
          spellCheck={false}
          value={json}
          onChange={(event) => setJson(event.target.value)}
        />
        {error && <p role="alert" className="mt-2 text-[12px] text-red-400">{error}</p>}
        <button type="button" className="btn-secondary mt-2 rounded-lg px-3 py-2 text-[12px]" onClick={applyJson}>
          {t("chromix.profileOptions.apply")}
        </button>
      </details>
    </div>
  );
}
