import { useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { bindCreator } from "../../lib/ipc";
import { confirm } from "../atoms";
import { Button } from "../atoms/Button";
import type { AuthorizeItem, ProfileSummary } from "../../types";

interface Props {
  profiles: ProfileSummary[];
}

function errText(e: unknown): string {
  return typeof e === "string" ? e : (e as Error).message ?? String(e);
}

/**
 * 达人授权页 (jieger `bindCreator`).
 *
 * - `list` reads persisted records only (no browser, no confirm).
 * - `sync` scrapes the Jinniu backend in the browser → confirm-gated.
 * - `authorize` runs the full bind flow → confirm-gated write action.
 */
export function BindAuthorizePage({ profiles }: Props): JSX.Element {
  const t = useT();
  const [profileId, setProfileId] = useState(profiles[0]?.id ?? "");
  const [jinniuId, setJinniuId] = useState("");
  const [accountId, setAccountId] = useState("");
  const [kuaishouId, setKuaishouId] = useState("");
  const [skipConfirm, setSkipConfirm] = useState(false);
  const [items, setItems] = useState<AuthorizeItem[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const currentProfile = profileId || profiles[0]?.id || "";
  const accountArg = accountId.trim() === "" ? undefined : accountId.trim();

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

  const onList = (): Promise<void> =>
    run("list", async () => {
      const res = await bindCreator.list(jinniuId.trim());
      if (!res.ok) throw new Error(res.error ?? "unknown");
      setItems(res.data);
    });

  const onSync = (): Promise<void> =>
    run("sync", async () => {
      const ok = await confirm({
        title: t("biz.bind.sync"),
        body: t("biz.bind.confirmSync"),
        confirmLabel: t("biz.bind.sync"),
      });
      if (!ok) return;
      const res = await bindCreator.sync(currentProfile, jinniuId.trim(), accountArg);
      if (!res.ok) throw new Error(res.error ?? "unknown");
      setItems(res.data);
      setNotice(t("biz.bind.syncedToast", { n: String(res.data.length) }));
    });

  const onAuthorize = (): Promise<void> =>
    run("authorize", async () => {
      const ok = await confirm({
        title: t("biz.bind.authorize"),
        body: t("biz.bind.confirmAuthorize", { id: kuaishouId.trim() }),
        confirmLabel: t("biz.bind.authorize"),
        destructive: !skipConfirm,
      });
      if (!ok) return;
      const res = await bindCreator.authorize(
        currentProfile,
        jinniuId.trim(),
        kuaishouId.trim(),
        skipConfirm || undefined,
        accountArg,
      );
      if (!res.ok) throw new Error(res.error ?? "unknown");
      setNotice(skipConfirm ? t("biz.bind.dryRunToast") : t("biz.bind.authorizedToast"));
    });

  return (
    <section aria-label={t("biz.bind.title")} className="flex flex-col gap-4 p-6 max-w-3xl">
      <div>
        <h2 className="text-lg font-semibold">{t("biz.bind.title")}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t("biz.bind.desc")}</p>
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
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.jinniuId")}
          <input
            aria-label={t("biz.jinniuId")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={jinniuId}
            onChange={(e) => setJinniuId(e.target.value)}
            placeholder="jinniu-…"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.accountId")}
          <input
            aria-label={t("biz.accountId")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={accountId}
            onChange={(e) => setAccountId(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.bind.kuaishouId")}
          <input
            aria-label={t("biz.bind.kuaishouId")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={kuaishouId}
            onChange={(e) => setKuaishouId(e.target.value)}
          />
        </label>
      </div>

      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={skipConfirm}
          onChange={(e) => setSkipConfirm(e.target.checked)}
        />
        {t("biz.bind.skipConfirm")}
      </label>

      <div className="flex gap-2">
        <Button size="sm" disabled={busy !== null || jinniuId.trim() === ""} onClick={() => void onList()}>
          {t("biz.bind.list")}
        </Button>
        <Button
          size="sm"
          variant="accent"
          disabled={busy !== null || jinniuId.trim() === "" || currentProfile === ""}
          onClick={() => void onSync()}
        >
          {busy === "sync" ? t("common.loading") : t("biz.bind.sync")}
        </Button>
        <Button
          size="sm"
          variant={skipConfirm ? "secondary" : "primary"}
          disabled={busy !== null || jinniuId.trim() === "" || kuaishouId.trim() === "" || currentProfile === ""}
          onClick={() => void onAuthorize()}
        >
          {busy === "authorize" ? t("common.loading") : t("biz.bind.authorize")}
        </Button>
      </div>

      {notice && (
        <div role="status" className="text-sm text-muted-foreground">
          {notice}
        </div>
      )}

      {items !== null &&
        (items.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.bind.empty")}</p>
        ) : (
          <table className="text-sm w-full">
            <thead>
              <tr className="text-left text-muted-foreground">
                <th className="py-1 pr-3">{t("biz.bind.cols.user")}</th>
                <th className="py-1 pr-3">{t("biz.bind.cols.status")}</th>
                <th className="py-1">{t("biz.bind.cols.time")}</th>
              </tr>
            </thead>
            <tbody>
              {items.map((it) => (
                <tr key={it.userId} data-testid={`bind-row-${it.userId}`} className="border-t border-[var(--border)]">
                  <td className="py-1.5 pr-3">
                    {it.userName} <span className="mono text-xs opacity-70">{it.userId}</span>
                  </td>
                  <td className="py-1.5 pr-3">{it.status}</td>
                  <td className="py-1.5">{it.authorizeTime}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ))}
    </section>
  );
}
