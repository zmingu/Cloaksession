import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { ProfileSummary } from "../types";
import { IdentityAvatarCache, kuaishouIdentity, type KuaishouIdentitySnapshot } from "./kuaishouIdentity";

interface Entry {
  snapshot: KuaishouIdentitySnapshot | null;
  error: string | null;
  pending: boolean;
  staleObservation?: string;
}
interface IdentityContextValue {
  entries: Map<string, Entry>;
  profiles: ProfileSummary[];
  closingIds?: Set<string>;
  listError: string | null;
  loading: boolean;
  refresh: () => Promise<void>;
  detect: (profileId: string) => Promise<void>;
  avatar: (key: string) => Promise<string | null>;
  selectedId: string | null;
  openDetails: (profileId: string | null) => void;
}
const IdentityContext = createContext<IdentityContextValue | null>(null);
const emptyEntry = (): Entry => ({ snapshot: null, error: null, pending: false });
const errorText = (cause: unknown): string => cause instanceof Error ? cause.message : String(cause);
// Backend observation markers are compared only for equality, never to local wall time.
const observationKey = (snapshot: KuaishouIdentitySnapshot): string =>
  JSON.stringify([snapshot.checkedAt, snapshot.lastSeenAt, snapshot.platformUserId]);

/** One owner for the entire app, not one poller per card. Only Rust performs automatic detection. */
export function KuaishouIdentityProvider({ profiles, closingIds, children }: {
  profiles: ProfileSummary[];
  closingIds?: Set<string>;
  children: ReactNode;
}) {
  const [entries, setEntries] = useState<Map<string, Entry>>(() => new Map());
  const [listError, setListError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [selectedId, openDetails] = useState<string | null>(null);
  const [avatars] = useState(() => new IdentityAvatarCache());
  const mounted = useRef(false);
  const profilesRef = useRef(profiles);
  const closingRef = useRef(closingIds);
  profilesRef.current = profiles;
  closingRef.current = closingIds;
  const listFlight = useRef<Promise<void> | null>(null);
  const manualFlights = useRef(new Map<string, Promise<void>>());
  // Request order, never the local clock or untrusted checkedAt timestamps.
  const versions = useRef(new Map<string, number>());
  const runningStates = useRef(new Map<string, boolean>());
  const runVersions = useRef(new Map<string, number>());

  useLayoutEffect(() => {
    const next = new Map(profiles.map((profile) => [profile.id, profile.isRunning && !closingIds?.has(profile.id)]));
    const changed = [...next.keys()].filter((id) => runningStates.current.has(id) && runningStates.current.get(id) !== next.get(id));
    runningStates.current = next;
    if (!changed.length) return;
    for (const id of changed) {
      versions.current.set(id, (versions.current.get(id) ?? 0) + 1);
      runVersions.current.set(id, (runVersions.current.get(id) ?? 0) + 1);
    }
    // Invalidate before paint: a close/reopen must not briefly promote a previous run.
    setEntries((previous) => {
      const updated = new Map(previous);
      for (const id of changed) {
        const entry = previous.get(id) ?? emptyEntry();
        updated.set(id, { ...entry, staleObservation: entry.snapshot ? observationKey(entry.snapshot) : entry.staleObservation });
      }
      return updated;
    });
  }, [profiles, closingIds]);

  const refresh = useCallback((): Promise<void> => {
    if (listFlight.current) return listFlight.current;
    const atStart = new Map(versions.current);
    const busyAtStart = new Set(manualFlights.current.keys());
    const eligible = (id: string): boolean => !busyAtStart.has(id) && !manualFlights.current.has(id)
      && (atStart.get(id) ?? 0) === (versions.current.get(id) ?? 0);
    setLoading(true);
    const task = (async () => {
      try {
        const snapshots = await kuaishouIdentity.list();
        if (!mounted.current) return;
        const results = new Map(snapshots.map((snapshot) => [snapshot.profileId, snapshot]));
        setEntries((previous) => {
          const next = new Map(previous);
          const ids = new Set([...previous.keys(), ...results.keys(), ...profilesRef.current.map((profile) => profile.id)]);
          for (const id of ids) {
            if (!eligible(id)) continue;
            const snapshot = results.get(id) ?? null;
            const stale = previous.get(id)?.staleObservation;
            const freshObservation = snapshot?.status === "detected" && observationKey(snapshot) !== stale;
            next.set(id, { snapshot, error: null, pending: false, staleObservation: freshObservation ? undefined : stale });
          }
          return next;
        });
        setListError(null);
      } catch (cause) {
        if (!mounted.current) return;
        const error = `检测状态未知，读取失败：${errorText(cause)}`;
        setListError(error);
        setEntries((previous) => {
          const next = new Map(previous);
          const ids = new Set([...previous.keys(), ...profilesRef.current.map((profile) => profile.id)]);
          for (const id of ids) {
            if (eligible(id)) next.set(id, { ...(previous.get(id) ?? emptyEntry()), error });
          }
          return next;
        });
      } finally {
        listFlight.current = null;
        if (mounted.current) setLoading(false);
      }
    })();
    listFlight.current = task;
    return task;
  }, []);

  const detect = useCallback((profileId: string): Promise<void> => {
    const existing = manualFlights.current.get(profileId);
    if (existing) return existing;
    if (!profilesRef.current.some((profile) => profile.id === profileId && profile.isRunning)
      || closingRef.current?.has(profileId)) return Promise.resolve();
    const runVersion = runVersions.current.get(profileId) ?? 0;
    const sameRun = (): boolean => runVersion === (runVersions.current.get(profileId) ?? 0);
    versions.current.set(profileId, (versions.current.get(profileId) ?? 0) + 1);
    setEntries((previous) => new Map(previous).set(profileId, {
      ...(previous.get(profileId) ?? emptyEntry()), pending: true, error: null,
    }));
    const task = (async () => {
      try {
        const snapshot = await kuaishouIdentity.detect(profileId);
        if (mounted.current) setEntries((previous) => sameRun()
          ? new Map(previous).set(profileId, { snapshot, error: null, pending: false }) : previous);
      } catch (cause) {
        if (mounted.current) setEntries((previous) => sameRun() ? new Map(previous).set(profileId, {
          ...(previous.get(profileId) ?? emptyEntry()), pending: false,
          error: `检测状态未知，重新检测失败，可重试：${errorText(cause)}`,
        }) : previous);
      } finally {
        versions.current.set(profileId, (versions.current.get(profileId) ?? 0) + 1);
        manualFlights.current.delete(profileId);
        if (mounted.current) setEntries((previous) => {
          const entry = previous.get(profileId);
          return entry?.pending ? new Map(previous).set(profileId, { ...entry, pending: false }) : previous;
        });
      }
    })();
    manualFlights.current.set(profileId, task);
    return task;
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh();
    }, 15_000);
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
      // StrictMode replays the effect synchronously; retain its shared in-flight request.
      queueMicrotask(() => { if (!mounted.current) avatars.dispose(); });
    };
  }, [refresh, avatars]);

  const value = useMemo(() => ({ entries, profiles, closingIds, listError, loading, refresh, detect,
    avatar: avatars.get, selectedId, openDetails }),
  [entries, profiles, closingIds, listError, loading, refresh, detect, avatars, selectedId]);
  return <IdentityContext.Provider value={value}>{children}</IdentityContext.Provider>;
}

export function useKuaishouIdentities(): IdentityContextValue {
  const value = useContext(IdentityContext);
  if (!value) throw new Error("KuaishouIdentityProvider is required");
  return value;
}

export function useKuaishouIdentity(profileId: string) {
  const context = useKuaishouIdentities();
  const entry = context.entries.get(profileId) ?? { ...emptyEntry(), error: context.listError };
  const profile = context.profiles.find((item) => item.id === profileId);
  const running = !!profile?.isRunning && !context.closingIds?.has(profileId);
  const snapshot = entry.snapshot;
  const status = entry.error ? "unknown" : entry.pending ? "unknown"
    : snapshot?.status === "conflict" ? "conflict"
    : snapshot && !running ? "closed"
    : snapshot?.status === "detected" && (!snapshot.platformUserId || entry.staleObservation) ? "unknown"
    : snapshot?.status ?? "unknown";
  const labels: Record<KuaishouIdentitySnapshot["status"], string> = {
    unknown: entry.error ? "检测状态未知 / 读取失败" : entry.staleObservation ? "等待本次运行重新检测" : "未检测",
    detected: "本轮已识别", "not-detected": "本轮未检测到", conflict: "登记冲突",
    closed: "已关闭", skipped: "已跳过", error: "检测失败",
  };
  const current = status === "detected" && !!snapshot?.platformUserId;
  const label = entry.pending ? "检测中…" : labels[status];
  return { ...entry, profile, running, status, current, label,
    history: !!snapshot?.platformUserId && !current,
    detect: () => context.detect(profileId), openDetails: () => context.openDetails(profileId) };
}
