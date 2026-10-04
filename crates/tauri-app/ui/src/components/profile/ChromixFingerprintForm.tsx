import { useId, useState, type JSX } from "react";
import { useT } from "../../i18n/LanguageProvider";
import {
  CHROMIX_FINGERPRINT_FIELDS,
  CHROMIX_FINGERPRINT_GROUPS,
  CHROMIX_FINGERPRINT_SOURCE,
  chromixArgsProblem,
  chromixFingerprintNotices,
  parseChromixRawArgs,
  readChromixField,
  readChromixFlag,
  setChromixField,
  validateChromixValue,
  type ChromixFingerprintField,
  type ChromixFlagValue,
} from "../../lib/chromixFingerprint";

export interface ChromixFingerprintFormProps {
  options: Record<string, unknown>;
  onChange: (options: Record<string, unknown>) => void;
}

type Props = ChromixFingerprintFormProps;

const controlClass = "w-full min-w-0 rounded-lg bg-white/[0.03] px-2.5 py-2 mono text-[12px] text-slate-200 outline-none focus:bg-white/[0.05] disabled:opacity-50";
const controlStyle = { boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.08)" };
const optionStyle = { background: "#12131a", color: "#e2e8f0" };
const smallButtonClass = "rounded px-2 py-1 text-[11px] text-muted-foreground hover:bg-white/[0.05] disabled:opacity-40";

export function ChromixFingerprintForm({ options, onChange }: Props): JSX.Element {
  const t = useT();
  const id = useId();
  const [query, setQuery] = useState("");
  const notices = chromixFingerprintNotices(options);
  const invalidArgs = !!chromixArgsProblem(options);
  const search = query.trim().toLowerCase();
  const fields = CHROMIX_FINGERPRINT_FIELDS.filter((field) => [field.label, field.flag, field.sdkKey, field.description, ...(field.aliases ?? [])].join(" ").toLowerCase().includes(search));
  const source = `${CHROMIX_FINGERPRINT_SOURCE.repository}/blob/${CHROMIX_FINGERPRINT_SOURCE.commit}`;

  return (
    <section className="min-w-0 space-y-3" aria-label={t("chromix.form.title")}>
      <div className="space-y-1.5 text-[11px] leading-relaxed text-slate-400">
        <h3 className="text-[13px] font-medium text-slate-200">{t("chromix.form.title")}</h3>
        <p>
          {t("chromix.form.intro")}
        </p>
        <p>
          {t("chromix.form.contractNote")}
        </p>
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-muted-foreground">
          <a href={`${source}/${CHROMIX_FINGERPRINT_SOURCE.flags}`} target="_blank" rel="noopener noreferrer">{t("chromix.form.docLinks.publicFlags")}</a>
          <a href={`${source}/${CHROMIX_FINGERPRINT_SOURCE.sdk}`} target="_blank" rel="noopener noreferrer">{t("chromix.form.docLinks.nodeSdk")}</a>
          <a href={`${source}/${CHROMIX_FINGERPRINT_SOURCE.backend}`} target="_blank" rel="noopener noreferrer">{t("chromix.form.docLinks.backendPolicy")}</a>
          <span className="mono text-slate-500">{CHROMIX_FINGERPRINT_SOURCE.commit.slice(0, 12)}</span>
        </div>
      </div>
      <div>
        <label htmlFor={`${id}-search`} className="mb-1 block text-[11px] text-slate-400">{t("chromix.form.search")}</label>
        <input id={`${id}-search`} type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("chromix.form.searchPlaceholder")} className={controlClass} style={controlStyle} />
      </div>
      {notices.length > 0 && (
        <details className="rounded-lg border border-amber-400/20 bg-amber-500/[0.04] p-3" open>
          <summary className="cursor-pointer text-[12px] text-amber-200">{t("chromix.form.notesTitle", { count: notices.length })}</summary>
          <ul className="mt-2 list-disc space-y-1.5 pl-4 text-[11px] leading-relaxed text-amber-200/80" aria-live="polite">
            {notices.map((notice) => <li key={notice}>{notice}</li>)}
          </ul>
        </details>
      )}
      {CHROMIX_FINGERPRINT_GROUPS.map((group) => {
        const groupFields = fields.filter((field) => field.group === group.id);
        if (!groupFields.length) return null;
        const configured = groupFields.filter((field) => readChromixField(options, field).present).length;
        return (
          <details key={`${group.id}-${search ? "search" : "browse"}`} className="mz-panel min-w-0 p-3" open={!!search || group.id === "identity" || configured > 0}>
            <summary className="cursor-pointer text-[12px] font-medium text-slate-200">
              {t(`chromix.groups.${group.id}`)}<span className="ml-2 text-[10px] font-normal text-slate-500">{t("chromix.form.groupCount", { set: configured, total: groupFields.length })}</span>
            </summary>
            <p className="mt-2 text-[11px] leading-relaxed text-slate-500">{t(`chromix.groups.${group.id}Desc`)}</p>
            <div className="mt-3 grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-2">
              {groupFields.map((field) => <FingerprintField key={field.id} field={field} options={options} onChange={onChange} disabled={invalidArgs && !field.sdkKey} />)}
            </div>
          </details>
        );
      })}
      {fields.length === 0 && <p className="text-[12px] text-slate-500">{t("chromix.form.noMatch")}</p>}
      <RawArgsEditor options={options} onChange={onChange} />
      <p className="text-[10px] leading-relaxed text-slate-500">
        {t("chromix.form.footerNote")}
      </p>
    </section>
  );
}

function FingerprintField({ field, options, onChange, disabled }: Props & { field: ChromixFingerprintField; disabled: boolean }): JSX.Element {
  const t = useT();
  const id = useId();
  const [selectedName, setSelectedName] = useState<string>();
  const effective = readChromixField(options, field);
  const state = selectedName ? readChromixFlag(options, selectedName) : effective;
  const error = validateChromixValue(field, state.value);
  const setValue = (value: ChromixFlagValue) => onChange(setChromixField(options, field, value, selectedName));
  const selectField = ["enum", "boolean", "presence", "feature"].includes(field.kind);
  const choices = field.kind === "boolean" || field.kind === "feature" ? ["true", "false"] : field.choices ?? [];
  const encodedValue = state.value === undefined ? "unset" : state.value === null ? "bare" : JSON.stringify(state.value);
  const customValue = state.value !== undefined && state.value !== null && !choices.includes(state.value);
  const rawTarget = state.name.startsWith("--uxr-");

  return (
    <div className="min-w-0 space-y-1.5">
      <div className="flex items-start justify-between gap-2">
        <label htmlFor={`${id}-value`} className="text-[12px] text-slate-300">{field.label}{field.unit && <span className="ml-1 text-slate-500">({field.unit})</span>}</label>
        {state.present && <button type="button" disabled={disabled} className={smallButtonClass} onClick={() => setValue(undefined)} title={t("chromix.field.unsetTitle")}>{t("chromix.field.unset")}</button>}
      </div>
      <div className="break-all mono text-[10px] text-slate-500">{field.sdkKey ? `options.${field.sdkKey}` : field.kind === "feature" ? "--enable-blink-features=FakeShadowRoot" : state.name}</div>
      {!!field.aliases?.length && (
        <select
          aria-label={t("chromix.field.editParamAria", { label: field.label })}
          value={selectedName ?? effective.name}
          onChange={(event) => setSelectedName(event.target.value)}
          disabled={disabled}
          className={`${controlClass} !py-1 !text-[10px]`}
          style={controlStyle}
        >
          {[field.flag!, ...field.aliases].map((name) => <option key={name} value={name} style={optionStyle}>{name}{name === field.flag ? t("chromix.field.srcPublic") : t("chromix.field.srcRaw")}</option>)}
        </select>
      )}
      {selectField ? (
        <select
          id={`${id}-value`}
          value={encodedValue}
          onChange={(event) => setValue(event.target.value === "unset" ? undefined : event.target.value === "bare" ? null : JSON.parse(event.target.value) as string)}
          disabled={disabled}
          aria-describedby={`${id}-help${error ? ` ${id}-error` : ""}`}
          aria-invalid={!!error}
          className={controlClass}
          style={controlStyle}
        >
          <option value="unset" style={optionStyle}>{t("chromix.field.notSet")}</option>
          {(field.kind === "presence" || state.value === null) && <option value="bare" style={optionStyle}>{field.sdkKey ? t("chromix.field.nullStored") : t("chromix.field.bareFlag")}</option>}
          {choices.map((choice) => <option key={choice} value={JSON.stringify(choice)} style={optionStyle}>{choice === "" ? t("chromix.field.explicitEmptyDisabled") : choice}</option>)}
          {customValue && <option value={encodedValue} style={optionStyle}>{t("chromix.field.storedValue", { stored: state.value || t("chromix.field.explicitEmpty") })}</option>}
        </select>
      ) : (
        <>
          <input
            id={`${id}-value`}
            type="text"
            inputMode={field.integer ? "numeric" : field.decimal ? "decimal" : undefined}
            value={state.value ?? ""}
            onChange={(event) => setValue(event.target.value === "" ? undefined : event.target.value)}
            disabled={disabled}
            spellCheck={false}
            placeholder={state.present ? state.value === null ? t("chromix.field.phBare") : t("chromix.field.phEmpty") : t("chromix.field.phUnset")}
            aria-invalid={!!error}
            aria-describedby={`${id}-help${error ? ` ${id}-error` : ""}`}
            className={controlClass}
            style={controlStyle}
          />
          {field.kind === "seed" && (
            <div className="flex flex-wrap gap-1">
              <button type="button" disabled={disabled} className={smallButtonClass} onClick={() => setValue(null)}>{t("chromix.field.bareOrSeed")}</button>
              <button type="button" disabled={disabled} className={smallButtonClass} onClick={() => setValue("off")}>{t("chromix.field.setOff")}</button>
            </div>
          )}
          {state.present && (state.value === "" || state.value === null) && <p className="text-[10px] text-slate-400">{t("chromix.field.storedExplicitly", { value: state.value === null ? t("chromix.field.bareFlagValue") : t("chromix.field.emptyString") })}</p>}
        </>
      )}
      <p id={`${id}-help`} className="text-[10px] leading-relaxed text-slate-500">{field.description}</p>
      {rawTarget && <p className="text-[10px] text-muted-foreground/80">{t("chromix.field.rawAliasNote")}</p>}
      {state.count > 1 && <p className="text-[10px] text-amber-200/80">{t("chromix.field.multiOccurrences", { count: state.count })}</p>}
      {error && <p id={`${id}-error`} className="text-[10px] leading-relaxed text-amber-200" role="status">{error}</p>}
    </div>
  );
}

function RawArgsEditor({ options, onChange }: Props): JSX.Element {
  const t = useT();
  const id = useId();
  const [draft, setDraft] = useState<{ base: string; text: string }>();
  const [error, setError] = useState<string>();
  const args = Array.isArray(options.args) && options.args.every((arg) => typeof arg === "string") ? options.args as string[] : [];
  const serialized = JSON.stringify(options.args);
  const base = serialized ?? "undefined";
  const text = draft?.text ?? args.join("\n");
  const stale = draft !== undefined && draft.base !== base;
  const problem = chromixArgsProblem(options) ?? (args.some((arg) => /[\r\n]/.test(arg)) ? t("chromix.validation.lineBreak") : undefined);

  function apply(): void {
    if (stale || problem) return;
    try {
      const next = parseChromixRawArgs(text);
      onChange({ ...options, args: next });
      setDraft(undefined);
      setError(undefined);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  return (
    <details className="mz-panel min-w-0 p-3">
      <summary className="cursor-pointer text-[12px] text-muted-foreground">{t("chromix.rawArgs.title")}</summary>
      <p id={`${id}-help`} className="my-2 text-[11px] leading-relaxed text-slate-500">
        {t("chromix.rawArgs.desc")}
      </p>
      <label htmlFor={`${id}-raw`} className="mb-1 block text-[11px] text-slate-400">{t("chromix.rawArgs.browserArgs")}</label>
      <textarea id={`${id}-raw`} rows={8} value={text} disabled={!!problem} onChange={(event) => { setDraft({ base: draft?.base ?? base, text: event.target.value }); setError(undefined); }} spellCheck={false} className={`${controlClass} resize-y`} style={controlStyle} aria-describedby={`${id}-help`} aria-invalid={!!error} />
      <div className="mt-2 flex flex-wrap gap-2">
        <button type="button" className={smallButtonClass} disabled={!!problem || stale || !draft} onClick={apply}>{t("chromix.rawArgs.apply")}</button>
        {draft && <button type="button" className={smallButtonClass} onClick={() => { setDraft(undefined); setError(undefined); }}>{t("chromix.rawArgs.discard")}</button>}
      </div>
      {(error || problem || stale) && <p className="mt-2 text-[11px] text-amber-200" role="alert">{error ?? problem ?? t("chromix.rawArgs.stale")}</p>}
    </details>
  );
}
