import { useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { jinniuPromote } from "../../lib/jinniuPromote";
import { confirm } from "../atoms";
import { Button } from "../atoms/Button";
import type {
  JinniuLiveUser,
  ProfileSummary,
  StoreCreatePhase1Config,
} from "../../types";

interface Props {
  profiles: ProfileSummary[];
}

function errText(e: unknown): string {
  return typeof e === "string" ? e : (e as Error).message ?? String(e);
}

/**
 * 磁力金牛推广页 (jieger `jinniuPromote`).
 *
 * Flow: open store-create → pick a live user → apply phase 1 → apply
 * phase 2 → submit. Every step after `openStoreCreate` mutates ad-account
 * state and is confirm-gated.
 */
export function JinniuPromotePage({ profiles }: Props): JSX.Element {
  const t = useT();
  const [profileId, setProfileId] = useState(profiles[0]?.id ?? "");
  const [accountId, setAccountId] = useState<string | null>(null);
  const [users, setUsers] = useState<JinniuLiveUser[] | null>(null);
  const [copies, setCopies] = useState<string[] | null>(null);
  const [phase1, setPhase1] = useState<StoreCreatePhase1Config>({
    enableNetRoi: false,
    dailyBudget: "",
    roiCoefficient: "",
    promoteType: "",
    roiTargetMode: "",
    creativeMode: "",
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const currentProfile = profileId || profiles[0]?.id || "";

  function patch<K extends keyof StoreCreatePhase1Config>(key: K, value: string): void {
    setPhase1((prev) => ({ ...prev, [key]: value }));
  }

  /** Empty inputs fall back to the backend's jieger defaults — send null, not "". */
  function resolvedConfig(): StoreCreatePhase1Config {
    const out: StoreCreatePhase1Config = { enableNetRoi: phase1.enableNetRoi ?? false };
    for (const key of ["dailyBudget", "roiCoefficient", "promoteType", "roiTargetMode", "creativeMode"] as const) {
      const v = (phase1[key] ?? "").trim();
      out[key] = v === "" ? null : v;
    }
    return out;
  }

  async function run(key: string, fn: () => Promise<void>): Promise<void> {
    setBusy(key);
    setNotice(null);
    try {
      await fn();
    } catch (e) {
      setNotice(t("account.failedToast", { detail: errText(e) }));
    } finally {
      setBusy(null);
    }
  }

  const onOpen = (): Promise<void> =>
    run("open", async () => {
      const tab = await jinniuPromote.openStoreCreate(currentProfile);
      setAccountId(tab.accountId);
      setNotice(t("biz.jinniu.openedToast", { accountId: tab.accountId }));
    });

  const onUsers = (): Promise<void> =>
    run("users", async () => {
      const res = await jinniuPromote.liveUsers(currentProfile);
      setUsers(res.users);
      setAccountId(res.accountId);
    });

  const onSelect = (user: JinniuLiveUser): Promise<void> =>
    run(`select:${user.uid}`, async () => {
      const ok = await confirm({
        title: t("biz.jinniu.selectUser"),
        body: t("biz.jinniu.confirmSelect", { name: user.displayName }),
        confirmLabel: t("biz.jinniu.selectUser"),
      });
      if (!ok) return;
      const selected = await jinniuPromote.selectLiveUser(currentProfile, user.uid);
      setUsers((prev) => prev?.map((u) => ({ ...u, isSelected: u.uid === selected.uid })) ?? null);
      setNotice(t("biz.jinniu.selectedToast", { name: selected.displayName }));
    });

  const onPhase1 = (): Promise<void> =>
    run("phase1", async () => {
      const ok = await confirm({
        title: t("biz.jinniu.applyPhase1"),
        body: t("biz.jinniu.confirmPhase1"),
        confirmLabel: t("biz.jinniu.applyPhase1"),
        destructive: true,
      });
      if (!ok) return;
      await jinniuPromote.applyPhase1(currentProfile, resolvedConfig());
    });

  const onPhase2 = (): Promise<void> =>
    run("phase2", async () => {
      const result = await jinniuPromote.applyPhase2(currentProfile);
      setCopies(result);
      setNotice(t("biz.jinniu.phase2Done", { n: String(result.length) }));
    });

  const onSubmit = (): Promise<void> =>
    run("submit", async () => {
      const ok = await confirm({
        title: t("biz.jinniu.submit"),
        body: t("biz.jinniu.confirmSubmit"),
        confirmLabel: t("biz.jinniu.submit"),
        destructive: true,
      });
      if (!ok) return;
      await jinniuPromote.submit(currentProfile);
      setNotice(t("biz.jinniu.submittedToast"));
    });

  const ready = currentProfile !== "" && busy === null;

  return (
    <section aria-label={t("biz.jinniu.title")} className="flex flex-col gap-4 p-6 max-w-3xl">
      <div>
        <h2 className="text-lg font-semibold">{t("biz.jinniu.title")}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t("biz.jinniu.desc")}</p>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.profile")}
          <select
            aria-label={t("biz.profile")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={currentProfile}
            onChange={(e) => setProfileId(e.target.value)}
          >
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        {accountId !== null && (
          <p data-testid="jinniu-account" className="text-sm self-end mono opacity-70">
            {accountId}
          </p>
        )}
      </div>

      <div className="flex gap-2 flex-wrap">
        <Button size="sm" disabled={!ready} onClick={() => void onOpen()}>
          {busy === "open" ? t("common.loading") : t("biz.jinniu.openStoreCreate")}
        </Button>
        <Button size="sm" disabled={!ready} onClick={() => void onUsers()}>
          {busy === "users" ? t("common.loading") : t("biz.jinniu.liveUsers")}
        </Button>
      </div>

      {users !== null &&
        (users.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.jinniu.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm">
            {users.map((u) => (
              <li
                key={u.uid}
                data-testid={`jinniu-user-${u.uid}`}
                className="flex items-center gap-2 border-t border-[var(--border)] py-1.5"
              >
                <span className="flex-1 truncate">
                  {u.displayName} <span className="mono text-xs opacity-70">{u.uid}</span>
                </span>
                {u.isSelected ? (
                  <span className="text-xs text-muted-foreground">✓</span>
                ) : (
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={!ready}
                    onClick={() => void onSelect(u)}
                  >
                    {t("biz.jinniu.selectUser")}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        ))}

      <fieldset className="flex flex-col gap-3 border border-[var(--border)] rounded-lg p-4">
        <legend className="text-sm font-medium px-1">{t("biz.jinniu.phase1")}</legend>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={phase1.enableNetRoi ?? false}
            onChange={(e) => setPhase1((prev) => ({ ...prev, enableNetRoi: e.target.checked }))}
          />
          {t("biz.jinniu.enableNetRoi")}
        </label>
        <div className="grid grid-cols-2 gap-3">
          {(
            [
              ["dailyBudget", t("biz.jinniu.dailyBudget")],
              ["roiCoefficient", t("biz.jinniu.roiCoefficient")],
              ["promoteType", t("biz.jinniu.promoteType")],
              ["roiTargetMode", t("biz.jinniu.roiTargetMode")],
              ["creativeMode", t("biz.jinniu.creativeMode")],
            ] as const
          ).map(([key, label]) => (
            <label key={key} className="flex flex-col gap-1 text-sm">
              {label}
              <input
                aria-label={label}
                className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
                value={phase1[key] ?? ""}
                onChange={(e) => patch(key, e.target.value)}
              />
            </label>
          ))}
        </div>
        <div className="flex gap-2 flex-wrap">
          <Button size="sm" variant="accent" disabled={!ready} onClick={() => void onPhase1()}>
            {busy === "phase1" ? t("common.loading") : t("biz.jinniu.applyPhase1")}
          </Button>
          <Button size="sm" variant="secondary" disabled={!ready} onClick={() => void onPhase2()}>
            {busy === "phase2" ? t("common.loading") : t("biz.jinniu.applyPhase2")}
          </Button>
          <Button size="sm" variant="primary" disabled={!ready} onClick={() => void onSubmit()}>
            {busy === "submit" ? t("common.loading") : t("biz.jinniu.submit")}
          </Button>
        </div>
      </fieldset>

      {notice && (
        <div role="status" className="text-sm text-muted-foreground">
          {notice}
        </div>
      )}

      {copies !== null && copies.length > 0 && (
        <ul data-testid="jinniu-copies" className="flex flex-col gap-1 text-sm">
          {copies.map((c, i) => (
            <li key={i} className="border-t border-[var(--border)] py-1.5">
              {c}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
