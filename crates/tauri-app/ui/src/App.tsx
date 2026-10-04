import { activity, chromium, profiles as profilesApi, profilesDeleteGroup, profilesListGroups, system, onActivityEvent, onChromiumStatus, onExtensionInstalled, onProxyCountryUpdated, onRunningChanged } from "./lib/ipc";
import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { Sidebar, type Section, type KuaishouTab } from "./components/screens/Sidebar";
import { Constellation, type GroupFilter } from "./components/profile/Constellation";
import { NewProfileSheet } from "./components/profile/NewProfileSheet";
import { ProfileEditSheet } from "./components/profile/ProfileEditSheet";
import type { Profile, ProfileGroup } from "./types";
import { ActivityDrawer } from "./components/activity/ActivityDrawer";
import { McpPanel } from "./components/mcp/McpPanel";
import { Settings } from "./components/screens/Settings";
import { BusinessSection } from "./components/business/BusinessSection";
import { KuaishouAccountsPage } from "./components/kuaishou/KuaishouAccountsPage";
import { KuaishouAccountWizard } from "./components/kuaishou/KuaishouAccountWizard";
import { Confirm, Prompt } from "./components/screens/Confirm";
import { CommandPalette, type CommandAction } from "./components/palette/CommandPalette";
import { FirstRun } from "./components/onboarding/FirstRun";
import { ChromiumBootstrapModal } from "./components/onboarding/ChromiumBootstrapModal";
import { UpdateBanner } from "./components/UpdateBanner";
import { Modal, ConfirmHost, confirm } from "./components/atoms";
import { readPersisted, usePersistedState, writePersisted } from "./lib/persisted";
import { useT } from "./i18n/LanguageProvider";
import { KuaishouIdentityProvider } from "./lib/KuaishouIdentityProvider";
import { KuaishouIdentityDialog } from "./components/profile/KuaishouIdentity";
import type { ActivityEvent, ChromiumStatus, ProfileSummary, SystemInfo } from "./types";

type ModalState =
  | { kind: "none" }
  | { kind: "import-passphrase" }
  | { kind: "export-passphrase"; profileId: string }
  | { kind: "delete-confirm"; profileId: string };

export function App(): JSX.Element {
  const t = useT();
  // Persisted UI state — survives app restarts under localStorage `multizen.ui.*`.
  const [section, setSection] = usePersistedState<Section>("section", "profiles");
  const [kuaishouTab, setKuaishouTab] = usePersistedState<KuaishouTab>("kuaishouTab", "shop");
  const [drawerOpen, setDrawerOpen] = usePersistedState<boolean>("drawerOpen", false);

  // Migrate the legacy "activity" section (renamed to "mcp") from localStorage
  // so an existing install doesn't land on a blank screen.
  useEffect(() => {
    if ((section as string) === "activity") setSection("mcp");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Ephemeral UI state.
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  // Profiles whose Chromium is winding down (window closed / Stop pressed) but
  // hasn't fully exited — surfaced as a "Terminating…" state on the card.
  const [closingIds, setClosingIds] = useState<Set<string>>(new Set());
  // One live safety-net timer per terminating profile. Keyed by id so a
  // re-terminate (or a relaunch) cancels the *previous* episode's timer
  // instead of letting a stale one fire and clear a genuinely-closing card.
  const closingTimers = useRef<Map<string, number>>(new Map());
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  // Whether the Chromium runtime is ready — used to suppress the update banner
  // while the blocking first-run bootstrap modal is up, so they don't compete.
  const [chromiumReady, setChromiumReady] = useState(false);
  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    const apply = (s: ChromiumStatus): void =>
      setChromiumReady(s.kind === "ready" || s.kind === "dev-system");
    void chromium.status().then(apply);
    void onChromiumStatus(apply).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);
  // Global toast when the companion "Add to JiegeGo" button installs an
  // extension into a running profile (the edit sheet may not be open).
  useEffect(() => {
    let unlisten = (): void => {};
    let active = true;
    void onExtensionInstalled((e) => {
      showToast(
        e.ok
          ? `Added "${e.extension.name}" — profile is reopening…`
          : `Extension install failed: ${e.error}`,
      );
    }).then((fn) => {
      if (active) unlisten = fn;
    });
    return () => {
      active = false;
      unlisten();
    };
  }, []);
  // Last-interacted profile id — only used by the command palette's
  // "Export" action, which exports whichever profile the user most
  // recently opened in the edit modal.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  async function openEditFor(id: string): Promise<void> {
    setSelectedId(id);
    const p = await profilesApi.get(id);
    if (p) setEditingProfile(p);
  }
  const [showOnboarding, setShowOnboarding] = useState(
    () => !readPersisted<boolean>("onboarded", false),
  );
  const [showSheet, setShowSheet] = useState(false);
  // Shop-account wizard (快手 › 小店 › 添加账号). Separate from the generic
  // New-profile sheet: it also drives the hidden-browser QR sign-in.
  const [wizardOpen, setWizardOpen] = useState(false);
  const [sheetDirty, setSheetDirty] = useState(false);
  const [editingProfile, setEditingProfile] = useState<Profile | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [modal, setModal] = useState<ModalState>({ kind: "none" });
  const [toast, setToast] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const list = await profilesApi.list();
    setProfiles(list);
  }, []);

  // Named groups + "Ungrouped" counts, shown in the sidebar. Fetched on
  // mount and after any create/edit/delete that can move group membership.
  const [groups, setGroups] = useState<ProfileGroup[]>([]);
  const [groupFilter, setGroupFilter] = useState<GroupFilter>("all");
  const refreshGroups = useCallback(async () => {
    try {
      setGroups(await profilesListGroups());
    } catch {
      // Backend may not register the command in every build — degrade to
      // an empty groups list rather than crashing the sidebar mount.
      setGroups([]);
    }
  }, []);

  // Initial load + activity stream subscription
  useEffect(() => {
    void refresh();
    void refreshGroups();
    void system.info().then(setInfo);
    void activity.recent().then(setEvents);

    let offEvents = (): void => {};
    let offRunning = (): void => {};
    let offProxyCountry = (): void => {};
    let active = true;

    void onActivityEvent((e) => {
      setEvents((prev) => {
        const idx = prev.findIndex((x) => x.id === e.id);
        if (idx >= 0) {
          const copy = prev.slice();
          copy[idx] = e;
          return copy;
        }
        return [...prev, e].slice(-500);
      });
      // Refetch profiles when MCP-driven create/launch/close — covers the
      // case where an AI agent created a profile we don't know about.
      if (
        e.tool === "launch_profile" ||
        e.tool === "close_profile" ||
        e.tool === "create_profile"
      ) {
        void refresh();
      }
    }).then((fn) => {
      if (active) offEvents = fn;
    });

    // Refetch on ANY running-state change, including the "user closed
    // Chromium window directly" case which doesn't go through MCP at all.
    // Track the transient "closing" phase separately so the card can show
    // "Terminating…" while the process winds down (still in the running map).
    void onRunningChanged((change) => {
      const id = change.profileId;
      const timers = closingTimers.current;
      // Any state transition ends the previous episode's safety timer, so a
      // stale timer can never clear a card that a later episode re-marked.
      const existing = timers.get(id);
      if (existing !== undefined) {
        window.clearTimeout(existing);
        timers.delete(id);
      }
      setClosingIds((prev) => {
        const next = new Set(prev);
        if (change.kind === "closing") next.add(id);
        else next.delete(id); // launched / closed
        return next;
      });
      if (change.kind === "closing") {
        // Safety net: if a terminal (closed/launched) event is ever missed,
        // don't strand the card on "Terminating…" — self-clear after 10s.
        const timer = window.setTimeout(() => {
          // Guard against a superseding episode: if a newer "closing" replaced
          // this timer between it elapsing and its callback running, do nothing.
          if (timers.get(id) !== timer) return;
          timers.delete(id);
          setClosingIds((prev) => {
            if (!prev.has(id)) return prev;
            const next = new Set(prev);
            next.delete(id);
            return next;
          });
        }, 10_000);
        timers.set(id, timer);
      }
      void refresh();
    }).then((fn) => {
      if (active) offRunning = fn;
    });

    // Background proxy-country backfill emits this as each probe lands —
    // refresh so the flag chip updates without the user touching anything.
    void onProxyCountryUpdated(() => {
      void refresh();
    }).then((fn) => {
      if (active) offProxyCountry = fn;
    });

    return () => {
      active = false;
      offEvents();
      offRunning();
      offProxyCountry();
      closingTimers.current.forEach((t) => window.clearTimeout(t));
      closingTimers.current.clear();
    };
  }, [refresh, refreshGroups]);

  // Keyboard shortcuts: ⌘K palette, ⌘N new profile,
  // ⌘⇧A drawer, esc closes overlays.
  useEffect(() => {
    function onKey(e: KeyboardEvent): void {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen(true);
        return;
      }
      if (meta && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setShowSheet(true);
        return;
      }
      if (meta && e.shiftKey && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setDrawerOpen((v) => !v);
        return;
      }
      // ESC: <Modal> handles its own ESC (with dirty-form confirm); no
      // other UI listens for ESC at the app level.
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showSheet]);

  function showToast(msg: string): void {
    setToast(msg);
    window.setTimeout(() => setToast((t) => (t === msg ? null : t)), 4000);
  }

  function dismissOnboarding(): void {
    writePersisted("onboarded", true);
    setShowOnboarding(false);
  }

  async function onboardCreate(name: string, tags: string[]): Promise<void> {
    await profilesApi.create({ name, tags });
    dismissOnboarding();
    await refresh();
    void refreshGroups();
  }

  async function deleteGroup(name: string): Promise<void> {
    try {
      await profilesDeleteGroup(name);
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error).message ?? String(e);
      showToast(`Delete group failed: ${msg}`);
      return;
    }
    if (groupFilter === name) setGroupFilter("all");
    await Promise.all([refresh(), refreshGroups()]);
  }

  async function launchProfile(id: string): Promise<void> {
    try {
      await profilesApi.launch(id);
    } catch (e) {
      const msg = typeof e === "string" ? e : (e as Error).message ?? String(e);
      showToast(`Launch failed: ${msg}`);
    }
    await refresh();
  }

  async function closeProfile(id: string): Promise<void> {
    // close() may reject if graceful shutdown throws, but the driver's
    // finally still emits "closed" (which drives its own refresh), so a
    // rejection here is cosmetic — swallow it to avoid an unhandled rejection.
    try {
      await profilesApi.close(id);
    } catch {
      /* driver emits "closed" regardless; state self-heals */
    }
    await refresh();
  }

  async function importProfile(passphrase: string): Promise<void> {
    setModal({ kind: "none" });
    const result = await profilesApi.importArchive(passphrase);
    if (!result.ok) {
      if (result.reason !== "cancelled") showToast(`Import failed: ${result.reason}`);
      return;
    }
    await refresh();
    void refreshGroups();
  }

  async function exportProfile(profileId: string, passphrase: string): Promise<void> {
    setModal({ kind: "none" });
    if (passphrase.length < 8) {
      showToast("Passphrase must be at least 8 characters");
      return;
    }
    const result = await profilesApi.exportArchive(profileId, passphrase);
    if (result.ok) {
      const file = result.path.split("/").slice(-1)[0];
      showToast(`Exported to ${file}`);
    } else if (result.reason !== "cancelled") {
      showToast(`Export failed: ${result.reason}`);
    }
  }

  async function deleteProfile(id: string): Promise<void> {
    setModal({ kind: "none" });
    await profilesApi.close(id).catch(() => {});
    await profilesApi.delete(id);
    if (selectedId === id) setSelectedId(null);
    await refresh();
    void refreshGroups();
  }

  function handleCommand(a: CommandAction): void {
    switch (a.kind) {
      case "launch":
        void launchProfile(a.profileId);
        break;
      case "open":
        void openEditFor(a.profileId);
        break;
      case "create":
        setShowSheet(true);
        break;
      case "import":
        setModal({ kind: "import-passphrase" });
        break;
      case "export":
        if (selectedId) setModal({ kind: "export-passphrase", profileId: selectedId });
        break;
      case "settings":
        setSection("settings");
        break;
      case "kuaishou":
        setSection("kuaishou");
        setKuaishouTab(a.tab);
        break;
      case "section":
        setSection(a.id);
        break;
    }
  }

  if (showOnboarding && true) {
    return <FirstRun onCreate={onboardCreate} />;
  }

  return (
    <KuaishouIdentityProvider profiles={profiles} closingIds={closingIds}>
    <div className="h-screen flex flex-col">
      <KuaishouIdentityDialog />

      <UpdateBanner suppressed={!chromiumReady} />

      <div className="flex-1 flex min-h-0">
        <Sidebar
          active={section}
          onChange={setSection}
          groups={groups}
          groupFilter={groupFilter}
          onGroupFilterChange={setGroupFilter}
          onDeleteGroup={deleteGroup}
          kuaishouTab={kuaishouTab}
          onKuaishouTabChange={setKuaishouTab}
        />

        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          {false && (
            <div
              className="m-6 px-4 py-3 rounded-lg text-sm text-red-300"
              style={{ background: "rgba(239,68,68,0.06)", boxShadow: "inset 0 0 0 1px rgba(239,68,68,0.25)" }}
            >
              Preload bridge missing — <code>window.multizen</code> is undefined. Open DevTools for details.
            </div>
          )}

          {section === "profiles" && (
            <>
              <Modal
                open={showSheet}
                title="New profile"
                subtitle="Cookies, login state, and fingerprint live in this profile only."
                width={720}
                onClose={() => {
                  setShowSheet(false);
                  setSheetDirty(false);
                }}
                confirmClose={async () => {
                  if (!sheetDirty) return true;
                  return confirm({
                    title: t("common.discardChanges.title"),
                    body: t("profile.create.discardBody"),
                    confirmLabel: t("common.discard"),
                    destructive: true,
                  });
                }}
              >
                <NewProfileSheet
                  onCancel={() => {
                    setShowSheet(false);
                    setSheetDirty(false);
                  }}
                  onDirtyChange={setSheetDirty}
                  onCreated={async (id, autoLaunch) => {
                    setShowSheet(false);
                    setSheetDirty(false);
                    await refresh();
                    void refreshGroups();
                    if (autoLaunch) {
                      await launchProfile(id);
                    }
                  }}
                />
              </Modal>
              <Constellation
                  profiles={profiles}
                  recentEvents={events}
                  closingIds={closingIds}
                  groupFilter={groupFilter}
                  onGroupFilterChange={setGroupFilter}
                  groups={groups}
                  onSelect={openEditFor}
                  onCreate={() => setShowSheet(true)}
                  onLaunch={launchProfile}
                  onStop={closeProfile}
                  onExport={(id) => setModal({ kind: "export-passphrase", profileId: id })}
                  onDelete={(id) => setModal({ kind: "delete-confirm", profileId: id })}
                />
            </>
          )}

          {section === "mcp" && (
            <McpPanel
              events={events}
              profiles={profiles}
              mcpUrl={info?.mcpHttpUrl ?? null}
              mcpToken={info?.mcpAuthToken ?? null}
            />
          )}

          {section === "settings" && <Settings onImport={() => setModal({ kind: "import-passphrase" })} />}

          {section === "kuaishou" && (
            <KuaishouAccountsPage tab={kuaishouTab} onAddAccount={() => setWizardOpen(true)} />
          )}

          {section === "business" && <BusinessSection profiles={profiles} />}
        </div>

      </div>

      {/* Edit profile — autosaves as you go (Discord-settings style), so no
          Save button and no discard gate; the sheet flushes a pending change
          on close. */}
      <Modal
        open={editingProfile !== null}
        title={editingProfile ? `Edit ${editingProfile.name}` : "Edit profile"}
        subtitle="Profile changes autosave. Business account registration saves separately."
        width={720}
        onClose={() => {
          setEditingProfile(null);
          void refresh();
          void refreshGroups();
        }}
      >
        {editingProfile && (
          <ProfileEditSheet profile={editingProfile} onSaved={() => { void refresh(); void refreshGroups(); }} />
        )}
      </Modal>

      {section !== "mcp" && (
        <ActivityDrawer
          open={drawerOpen}
          events={events}
          profiles={profiles}
          onToggle={() => setDrawerOpen((v) => !v)}
        />
      )}

      <ChromiumBootstrapModal />

      {/* Mount once at app root so confirm() works from anywhere. */}
      <ConfirmHost />

      <CommandPalette
        open={paletteOpen}
        profiles={profiles}
        onClose={() => setPaletteOpen(false)}
        onAction={handleCommand}
      />

      <KuaishouAccountWizard
        open={wizardOpen}
        onClose={() => setWizardOpen(false)}
        onCreated={() => void refresh()}
      />

      <Prompt
        open={modal.kind === "import-passphrase"}
        title="Import profile archive"
        description="Choose a .mzar file. Provide the passphrase used at export time."
        label="Passphrase"
        inputType="password"
        placeholder="Passphrase"
        confirmLabel="Choose file & import"
        onSubmit={importProfile}
        onCancel={() => setModal({ kind: "none" })}
      />

      <Prompt
        open={modal.kind === "export-passphrase"}
        title="Export profile archive"
        description="Choose a passphrase to encrypt the archive. You'll need it to import this profile elsewhere. Minimum 8 characters."
        label="New passphrase"
        inputType="password"
        placeholder="At least 8 characters"
        confirmLabel="Encrypt & save…"
        onSubmit={(p) => {
          if (modal.kind === "export-passphrase") {
            return exportProfile(modal.profileId, p);
          }
          return Promise.resolve();
        }}
        onCancel={() => setModal({ kind: "none" })}
      />

      <Confirm
        open={modal.kind === "delete-confirm"}
        title="Delete this profile?"
        description="Cookies, login state, and on-disk data will be erased permanently. This cannot be undone."
        confirmLabel="Yes, delete"
        destructive
        onConfirm={() => {
          if (modal.kind === "delete-confirm") void deleteProfile(modal.profileId);
        }}
        onCancel={() => setModal({ kind: "none" })}
      />

      {toast && (
        <div
          className="fixed right-6 px-4 py-3 rounded-lg text-sm surface-material"
          style={{
            bottom: 56,
            boxShadow: "inset 0 0 0 1px var(--border), 0 20px 60px rgba(0,0,0,0.5)",
            animation: "mz-slide-up 200ms ease-out",
          }}
        >
          {toast}
        </div>
      )}
    </div>
    </KuaishouIdentityProvider>
  );
}
