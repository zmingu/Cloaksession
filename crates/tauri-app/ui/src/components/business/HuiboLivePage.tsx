import { useState, type JSX } from "react";

import { useT } from "../../i18n/LanguageProvider";
import { huiboLive, type HuiboVideo, type ShopLiveState } from "../../lib/huiboLive";
import { confirm } from "../atoms";
import { Button } from "../atoms/Button";

interface Props {
  /** 当前选中的浏览器环境（账号维度的键）；空字符串表示未选。 */
  profileId: string;
}

function errText(e: unknown): string {
  return typeof e === "string" ? e : (e as Error).message ?? String(e);
}

/**
 * 跟播/回播页 (jieger `huiboLive`)。
 *
 * **账号维度**：不再自持 profile 选择器，改为接收上游选中的 `profileId`，
 * 只渲染该账号的开播控制（命令调用语义不变：`get_huibo_video_list` /
 * `start_huibo_live` / `get_shop_live_state` / `cancel_huibo_task`）。
 *
 * `startLive` only drives the flow — the re-read `ShopLiveState`
 * (`liveState`, fetched right after start) is the source of truth for
 * success. `cancel` is confirm-gated.
 */
export function HuiboLivePage({ profileId }: Props): JSX.Element {
  const t = useT();
  const [videos, setVideos] = useState<HuiboVideo[] | null>(null);
  const [replayId, setReplayId] = useState("");
  const [live, setLive] = useState<ShopLiveState | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const currentProfile = profileId;

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

  const onRefreshVideos = (): Promise<void> =>
    run("videos", async () => {
      const list = await huiboLive.videoList(currentProfile);
      setVideos(list);
      if (list.length > 0 && replayId === "") setReplayId(list[0].id);
    });

  const onRefreshState = (): Promise<void> =>
    run("state", async () => {
      setLive(await huiboLive.liveState(currentProfile));
    });

  const onStart = (): Promise<void> =>
    run("start", async () => {
      const ok = await confirm({
        title: t("biz.huibo.start"),
        body: t("biz.huibo.confirmStart", { id: replayId }),
        confirmLabel: t("biz.huibo.start"),
        destructive: true,
      });
      if (!ok) return;
      await huiboLive.startLive(currentProfile, replayId);
      // Success is proven by the re-read state, not by the start call.
      const state = await huiboLive.liveState(currentProfile);
      setLive(state);
      setNotice(t("biz.huibo.startedToast", { status: state.status }));
    });

  const onCancel = (): Promise<void> =>
    run("cancel", async () => {
      const ok = await confirm({
        title: t("biz.huibo.cancel"),
        body: t("biz.huibo.confirmCancel"),
        confirmLabel: t("biz.huibo.cancel"),
        destructive: true,
      });
      if (!ok) return;
      const cancelled = await huiboLive.cancel(currentProfile);
      if (cancelled) setNotice(t("biz.huibo.cancelledToast"));
    });

  return (
    <section aria-label={t("biz.huibo.title")} className="flex flex-col gap-4 p-6 max-w-3xl">
      <div>
        <h2 className="text-lg font-semibold">{t("biz.huibo.title")}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t("biz.huibo.desc")}</p>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="flex flex-col gap-1 text-sm">
          {t("biz.profile")}
          {/* 账号来自左侧侧栏选中项（不再自持选择器）。 */}
          <p data-testid="huibo-current-account" className="h-8 flex items-center text-muted-foreground">
            {currentProfile !== "" ? currentProfile : t("live.accounts.selectFirst")}
          </p>
        </div>
        <label className="flex flex-col gap-1 text-sm">
          {t("biz.huibo.replayId")}
          <select
            aria-label={t("biz.huibo.replayId")}
            className="h-8 rounded-md px-2 bg-transparent border border-[var(--border)]"
            value={replayId}
            onChange={(e) => setReplayId(e.target.value)}
          >
            {(videos ?? []).map((v) => (
              <option key={v.id} value={v.id}>
                {v.name} · {v.duration}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="flex gap-2 flex-wrap">
        <Button size="sm" disabled={busy !== null || currentProfile === ""} onClick={() => void onRefreshVideos()}>
          {busy === "videos" ? t("common.loading") : t("biz.huibo.refreshVideos")}
        </Button>
        <Button
          size="sm"
          variant="primary"
          disabled={busy !== null || currentProfile === "" || replayId === ""}
          onClick={() => void onStart()}
        >
          {busy === "start" ? t("common.loading") : t("biz.huibo.start")}
        </Button>
        <Button size="sm" variant="secondary" disabled={busy !== null || currentProfile === ""} onClick={() => void onRefreshState()}>
          {busy === "state" ? t("common.loading") : t("biz.huibo.refreshState")}
        </Button>
        <Button size="sm" variant="danger" disabled={busy !== null || currentProfile === ""} onClick={() => void onCancel()}>
          {busy === "cancel" ? t("common.loading") : t("biz.huibo.cancel")}
        </Button>
      </div>

      {notice && (
        <div role="status" className="text-sm text-muted-foreground">
          {notice}
        </div>
      )}

      {live && (
        <p data-testid="huibo-live-state" className="text-sm">
          {t("biz.huibo.state", { status: live.status })}
          {live.liveRoomUrl ? (
            <span className="mono text-xs opacity-70"> · {live.liveRoomUrl}</span>
          ) : null}
        </p>
      )}

      {videos !== null &&
        (videos.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("biz.huibo.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm">
            {videos.map((v) => (
              <li
                key={v.id}
                data-testid={`huibo-video-${v.id}`}
                className="flex items-center gap-2 border-t border-[var(--border)] py-1.5"
              >
                <span className="flex-1 truncate">
                  {v.name} <span className="mono text-xs opacity-70">{v.duration} · {v.size}</span>
                </span>
                <span className="mono text-xs opacity-70">{v.status}</span>
              </li>
            ))}
          </ul>
        ))}
    </section>
  );
}
