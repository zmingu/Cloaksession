import { useState, type JSX } from "react";

import { Button } from "../atoms/Button";
import { useT } from "../../i18n/LanguageProvider";
import { scenes } from "../../lib/scenes";
import type {
  Scene,
  SceneGroupIdPatch,
  SceneLineAction,
  SceneTriggerMode,
} from "../../types";

type ConfirmKind = "create" | "deleteScene" | "deleteLine" | "play" | "stop" | null;

/**
 * C-group: 场景剧本.
 *
 * Scene / SceneLine CRUD plus play/stop over the sub-account pool.
 * Every write is two-step confirmed. The `group_id` tri-state is
 * explicit: keep (omit) / clear (null) / set (new id) — callers can
 * never confuse "leave alone" with "clear".
 */
export function CScenePlayPanel(): JSX.Element {
  const t = useT();
  const [list, setList] = useState<Scene[] | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [createName, setCreateName] = useState("");
  const [createTrigger, setCreateTrigger] = useState<SceneTriggerMode>("relative-time");
  const [groupMode, setGroupMode] = useState<SceneGroupIdPatch["mode"]>("keep");
  const [groupValue, setGroupValue] = useState("");
  const [lineMessage, setLineMessage] = useState("");
  const [lineOffset, setLineOffset] = useState("10");
  const [lineAction, setLineAction] = useState<SceneLineAction>("danmaku");
  const [dynamicPool, setDynamicPool] = useState(false);
  const [confirming, setConfirming] = useState<ConfirmKind>(null);
  const [pendingLineId, setPendingLineId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selected = list?.find((s) => s.id === selectedId) ?? null;

  async function refresh(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const items = await scenes.list();
      setList(items);
      if (selectedId != null && !items.some((s) => s.id === selectedId)) setSelectedId(null);
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  function groupPatch(): SceneGroupIdPatch {
    if (groupMode === "clear") return { mode: "clear" };
    if (groupMode === "set") return { mode: "set", groupId: groupValue.trim() };
    return { mode: "keep" };
  }

  async function doCreate(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const created = await scenes.create(createName.trim());
      await scenes.update(created.id, { triggerMode: createTrigger, groupId: groupPatch() });
      setCreateName("");
      setNotice(t("biz.scene.createdToast"));
      await refresh();
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  }

  async function doDeleteScene(): Promise<void> {
    if (selectedId == null) return;
    setBusy(true);
    setError(null);
    try {
      await scenes.delete(selectedId);
      setSelectedId(null);
      await refresh();
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  }

  async function doAddLine(): Promise<void> {
    if (selectedId == null) return;
    setBusy(true);
    setError(null);
    try {
      await scenes.addLine(selectedId, lineMessage.trim(), Number(lineOffset), lineAction);
      setLineMessage("");
      await refresh();
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function doDeleteLine(): Promise<void> {
    if (pendingLineId == null) return;
    setBusy(true);
    setError(null);
    try {
      await scenes.deleteLine(pendingLineId);
      await refresh();
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
      setPendingLineId(null);
    }
  }

  async function moveLine(lineId: number, dir: -1 | 1): Promise<void> {
    if (selected == null) return;
    const ids = selected.lines.map((l) => l.id);
    const idx = ids.indexOf(lineId);
    const swap = idx + dir;
    if (idx < 0 || swap < 0 || swap >= ids.length) return;
    [ids[idx], ids[swap]] = [ids[swap], ids[idx]];
    setBusy(true);
    setError(null);
    try {
      await scenes.reorderLines(selected.id, ids);
      setNotice(t("biz.scene.line.reorderedToast"));
      await refresh();
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
    }
  }

  async function doPlay(): Promise<void> {
    if (selectedId == null) return;
    setBusy(true);
    setError(null);
    try {
      const started = await scenes.play(selectedId, { allowDynamicPool: dynamicPool });
      setNotice(t("biz.scene.playedToast", { n: String(started.scheduledCount) }));
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  }

  async function doStop(): Promise<void> {
    if (selectedId == null) return;
    setBusy(true);
    setError(null);
    try {
      await scenes.stop(selectedId);
      setNotice(t("biz.scene.stoppedToast"));
    } catch (e) {
      setError(t("biz.scene.failedToast", { detail: String(e) }));
    } finally {
      setBusy(false);
      setConfirming(null);
    }
  }

  const lineOffsetValue = Number(lineOffset);
  const canAddLine =
    selectedId != null && lineMessage.trim() !== "" && Number.isFinite(lineOffsetValue) && lineOffsetValue >= 0;

  return (
    <section aria-label={t("biz.scene.title")} className="space-y-4">
      <p className="text-[12px] text-slate-400">{t("biz.scene.desc")}</p>

      <div className="flex items-center gap-2">
        <input
          aria-label={t("biz.scene.create.name")}
          className="rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
          value={createName}
          onChange={(e) => setCreateName(e.target.value)}
        />
        <select
          aria-label={t("biz.scene.trigger.label")}
          className="rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
          value={createTrigger}
          onChange={(e) => setCreateTrigger(e.target.value as SceneTriggerMode)}
        >
          <option value="relative-time">{t("biz.scene.trigger.relative")}</option>
          <option value="local-time">{t("biz.scene.trigger.local")}</option>
        </select>
        <select
          aria-label={t("biz.scene.group.label")}
          className="rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
          value={groupMode}
          onChange={(e) => setGroupMode(e.target.value as SceneGroupIdPatch["mode"])}
        >
          <option value="keep">{t("biz.scene.group.keep")}</option>
          <option value="clear">{t("biz.scene.group.clear")}</option>
          <option value="set">{t("biz.scene.group.set")}</option>
        </select>
        {groupMode === "set" && (
          <input
            aria-label={t("biz.scene.group.setPlaceholder")}
            className="rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
            value={groupValue}
            onChange={(e) => setGroupValue(e.target.value)}
          />
        )}
        {!confirming && (
          <Button variant="primary" size="sm" disabled={busy || createName.trim() === ""} onClick={() => setConfirming("create")}>
            {t("biz.scene.create.button")}
          </Button>
        )}
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => void refresh()}>
          {t("biz.scene.list.refresh")}
        </Button>
      </div>

      {confirming === "create" && (
        <div role="group" aria-label={t("biz.scene.confirm.createTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
          <p className="text-[12px] text-slate-300">{t("biz.scene.confirm.createBody", { name: createName.trim() })}</p>
          <div className="flex gap-2">
            <Button variant="primary" size="sm" disabled={busy} onClick={() => void doCreate()}>
              {t("biz.scene.confirm.confirm")}
            </Button>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirming(null)}>
              {t("biz.scene.confirm.cancel")}
            </Button>
          </div>
        </div>
      )}

      <div aria-label={t("biz.scene.list.title")}>
        {list == null || list.length === 0 ? (
          <p className="text-[12px] text-slate-500">{t("biz.scene.list.empty")}</p>
        ) : (
          <ul className="space-y-1">
            {list.map((scene) => (
              <li key={scene.id} className="flex items-center gap-2 text-[12px]">
                <button
                  type="button"
                  className={scene.id === selectedId ? "text-slate-100 underline" : "text-slate-300"}
                  onClick={() => setSelectedId(scene.id)}
                >
                  {scene.name} ({scene.lines.length})
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {selected == null ? (
        <p className="text-[12px] text-slate-500">{t("biz.scene.detail.nobody")}</p>
      ) : (
        <div className="space-y-3" aria-label={t("biz.scene.detail.lines")}>
          <ul className="space-y-1">
            {selected.lines.map((line, idx) => (
              <li key={line.id} className="flex flex-wrap items-center gap-2 text-[12px] text-slate-300">
                <span className="min-w-0 flex-1 break-all">
                  [{line.timeOffsetSec}s/{line.actionType}] {line.message}
                </span>
                <Button variant="ghost" size="sm" disabled={busy || idx === 0} onClick={() => void moveLine(line.id, -1)}>
                  {t("biz.scene.line.moveUp")}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy || idx === selected.lines.length - 1}
                  onClick={() => void moveLine(line.id, 1)}
                >
                  {t("biz.scene.line.moveDown")}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={t("biz.scene.line.deleteAria", { n: String(idx + 1) })}
                  onClick={() => {
                    setPendingLineId(line.id);
                    setConfirming("deleteLine");
                  }}
                >
                  {t("biz.scene.line.delete")}
                </Button>
              </li>
            ))}
          </ul>

          <div className="flex flex-wrap items-center gap-2">
            <input
              aria-label={t("biz.scene.line.message")}
              className="flex-1 rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
              value={lineMessage}
              onChange={(e) => setLineMessage(e.target.value)}
            />
            <input
              aria-label={t("biz.scene.line.offset")}
              className="w-20 rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
              value={lineOffset}
              inputMode="numeric"
              onChange={(e) => setLineOffset(e.target.value)}
            />
            <select
              aria-label={t("biz.scene.line.action")}
              className="rounded-md bg-white/5 px-2 py-1.5 text-[12px] text-slate-100"
              value={lineAction}
              onChange={(e) => setLineAction(e.target.value as SceneLineAction)}
            >
              <option value="danmaku">{t("biz.scene.line.action.danmaku")}</option>
              <option value="like">{t("biz.scene.line.action.like")}</option>
              <option value="follow">{t("biz.scene.line.action.follow")}</option>
            </select>
            <Button variant="secondary" size="sm" disabled={busy || !canAddLine} onClick={() => void doAddLine()}>
              {t("biz.scene.line.add")}
            </Button>
          </div>

          <div className="flex items-center gap-2">
            <label className="flex items-center gap-1 text-[12px] text-slate-300">
              <input type="checkbox" checked={dynamicPool} onChange={(e) => setDynamicPool(e.target.checked)} />
              {t("biz.scene.play.dynamicPool")}
            </label>
            {confirming == null && (
              <>
                <Button variant="primary" size="sm" disabled={busy} onClick={() => setConfirming("play")}>
                  {t("biz.scene.play.button")}
                </Button>
                <Button variant="danger" size="sm" disabled={busy} onClick={() => setConfirming("stop")}>
                  {t("biz.scene.stop.button")}
                </Button>
                <Button variant="danger" size="sm" disabled={busy} onClick={() => setConfirming("deleteScene")}>
                  {t("biz.scene.scene.delete")}
                </Button>
              </>
            )}
          </div>

          {confirming === "deleteLine" && (
            <div role="group" aria-label={t("biz.scene.confirm.deleteLineTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
              <p className="text-[12px] text-slate-300">{t("biz.scene.confirm.deleteLineBody")}</p>
              <div className="flex gap-2">
                <Button variant="danger" size="sm" disabled={busy} onClick={() => void doDeleteLine()}>
                  {t("biz.scene.confirm.confirm")}
                </Button>
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => { setConfirming(null); setPendingLineId(null); }}>
                  {t("biz.scene.confirm.cancel")}
                </Button>
              </div>
            </div>
          )}
          {confirming === "play" && (
            <div role="group" aria-label={t("biz.scene.confirm.playTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
              <p className="text-[12px] text-slate-300">
                {t("biz.scene.confirm.playBody", { name: selected.name, n: String(selected.lines.length) })}
              </p>
              <div className="flex gap-2">
                <Button variant="primary" size="sm" disabled={busy} onClick={() => void doPlay()}>
                  {t("biz.scene.confirm.confirm")}
                </Button>
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirming(null)}>
                  {t("biz.scene.confirm.cancel")}
                </Button>
              </div>
            </div>
          )}
          {confirming === "stop" && (
            <div role="group" aria-label={t("biz.scene.confirm.stopTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
              <p className="text-[12px] text-slate-300">{t("biz.scene.confirm.stopBody")}</p>
              <div className="flex gap-2">
                <Button variant="danger" size="sm" disabled={busy} onClick={() => void doStop()}>
                  {t("biz.scene.confirm.confirm")}
                </Button>
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirming(null)}>
                  {t("biz.scene.confirm.cancel")}
                </Button>
              </div>
            </div>
          )}
          {confirming === "deleteScene" && (
            <div role="group" aria-label={t("biz.scene.confirm.deleteSceneTitle")} className="rounded-lg border border-amber-400/25 p-3 space-y-2">
              <p className="text-[12px] text-slate-300">{t("biz.scene.confirm.deleteSceneBody", { name: selected.name })}</p>
              <div className="flex gap-2">
                <Button variant="danger" size="sm" disabled={busy} onClick={() => void doDeleteScene()}>
                  {t("biz.scene.confirm.confirm")}
                </Button>
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => setConfirming(null)}>
                  {t("biz.scene.confirm.cancel")}
                </Button>
              </div>
            </div>
          )}
        </div>
      )}

      {notice && <p role="status" className="text-[12px] text-emerald-300">{notice}</p>}
      {error && <p role="alert" className="text-[12px] text-red-300">{error}</p>}
    </section>
  );
}
