import { useEffect, useMemo, useState } from "react";

import { onLiveLaunchStateChanged } from "../../lib/liveLaunch";
import type { ProfileSummary } from "../../types";
import type { LiveAccountRailItem } from "./LiveAccountRail";

/**
 * 直播账号状态（「直播」下 prepare / interact 两页共用）。
 *
 * 账号维度的键 = `profileId`（浏览器环境 = 快手小店账号）；「在播」来自
 * `live-launch-state-changed` 中 `subjectKind === "profile" && status === "streaming"`。
 *
 * 由 `LiveSection` 单次调用并向下传，保证同一事件**只有一处订阅**。
 */
export interface LiveAccountsState {
  /** 侧栏条目（id / name / live），顺序与 profiles 一致。 */
  items: LiveAccountRailItem[];
  /** 当前选中的 profileId（空字符串表示未选）。 */
  selectedId: string;
  /** 选择账号（侧栏点击）。 */
  setSelectedId: (id: string) => void;
  /** 正在推流的 profileId 集合。 */
  liveIds: ReadonlySet<string>;
}

export function useLiveAccounts(profiles: ProfileSummary[]): LiveAccountsState {
  const [selectedId, setSelectedId] = useState("");
  const [liveIds, setLiveIds] = useState<ReadonlySet<string>>(new Set());

  // 「直播中」：profile 桶 + streaming 才算在播。
  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onLiveLaunchStateChanged((s) => {
      if (!active || s.subjectKind !== "profile") return;
      setLiveIds((prev) => {
        const next = new Set(prev);
        if (s.status === "streaming") next.add(s.profileId);
        else next.delete(s.profileId);
        return next;
      });
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);

  // 默认选中：第一个在播账号 → 否则第一个 profile。用户一旦选中即保持。
  useEffect(() => {
    if (selectedId !== "" && profiles.some((p) => p.id === selectedId)) return;
    const firstLive = profiles.find((p) => liveIds.has(p.id));
    setSelectedId((firstLive ?? profiles[0])?.id ?? "");
  }, [profiles, liveIds, selectedId]);

  const items = useMemo<LiveAccountRailItem[]>(
    () => profiles.map((p) => ({ id: p.id, name: p.name, live: liveIds.has(p.id) })),
    [profiles, liveIds],
  );

  return { items, selectedId, setSelectedId, liveIds };
}
