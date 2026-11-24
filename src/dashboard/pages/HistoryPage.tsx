import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionStatus } from "../../components/ActionStatus";
import { errorText, useAction } from "../../components/useAction";
import { dispatch, type EchoProgram, type SavedFile } from "../../ipc";
import { SessionRepository } from "../../services/db/repositories/sessionRepository";
import type { Session } from "../../services/db/schema";
import { mergeSessions, type MergedSession } from "../../services/sessionMerge";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { Calendar } from "../history/Calendar";
import { DeskSyncPanel } from "../history/DeskSync";
import { groupByDay, metersByDay, RANGES, totals, visibleSessions, type Range } from "../history/sessions";
import { SessionRow } from "../history/SessionRow";
import { TotalsPanel, WeeklyPanel } from "../history/Summary";
import { TcxModal } from "../history/TcxModal";
import { TodayPanel } from "../history/Today";
import { useSessionUploads } from "../history/useSessionUploads";
import { WallClockPanel } from "../history/WallClock";
import { GhostButton, Label, Page, Panel } from "../ui";
import { cx } from "../util";

/** Walks loaded at once: three a day for a year fill the 53-week calendar, and 1000 rows stay quick. */
const LOAD_LIMIT = 1000;
/** Until the first snapshot: one constant, so it never reads as a change. */
const NO_IDS: number[] = [];

export function HistoryPage() {
  const refreshTrigger = useTreadmillStore((s) => s.historyRefreshTrigger);
  const thresholdMinutes = useTreadmillStore((s) => s.sessionMergeThresholdMinutes);
  const confirm = useTreadmillStore((s) => s.confirm);
  const openProgram = useTreadmillStore((s) => s.openProgram);
  const activeSessionId = useAppState((s) => s.session.active_id);
  const weekly = useAppState((s) => s.weekly);
  const deskSync = useAppState((s) => s.desk_sync);
  const wallClock = useAppState((s) => s.wall_clock);
  const todayTimeline = useAppState((s) => s.today_timeline);
  const savedIds = useAppState((s) => s.strava.saved_session_ids) ?? NO_IDS;

  const [raw, setRaw] = useState<Session[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloads, setReloads] = useState(0);
  const [range, setRange] = useState<Range>("7d");
  const [day, setDay] = useState<string | null>(null);
  const [openId, setOpenId] = useState<number | null>(null);
  const [tcx, setTcx] = useState<{ session: MergedSession; text: string } | null>(null);
  const exportAction = useAction();
  const reload = useCallback(() => setReloads((n) => n + 1), []);
  const uploads = useSessionUploads(reload);

  useEffect(() => {
    let alive = true;
    SessionRepository.getSessions(LOAD_LIMIT)
      .then((rows) => {
        if (!alive) return;
        setRaw(rows);
        setLoadError(null);
      })
      .catch((e) => {
        console.error("[history] load failed", e);
        if (alive) setLoadError(String(e));
      });
    return () => {
      alive = false;
    };
  }, [refreshTrigger, reloads]);

  // Newest first.
  const sessions = useMemo(() => (raw ? mergeSessions(raw, thresholdMinutes).reverse() : null), [raw, thresholdMinutes]);
  const byDay = useMemo(() => metersByDay(sessions ?? []), [sessions]);
  const visible = useMemo(() => visibleSessions(sessions ?? [], range, day), [sessions, range, day]);
  const sum = useMemo(() => totals(visible), [visible]);
  const groups = useMemo(() => groupByDay(visible), [visible]);

  const inspect = async (session: MergedSession, row: number) => {
    try {
      const text = await invoke<string>("strava_tcx_preview", { sessionIds: session.mergedIds });
      setTcx({ session, text });
    } catch (e) {
      uploads.setNote(row, { ok: false, text: `Preview failed: ${errorText(e)}` });
    }
  };

  const remove = (session: MergedSession, row: number) => {
    const n = session.mergedIds.length;
    confirm({
      title: "Delete session",
      message: `Delete ${n > 1 ? `${n} joined records` : "the session"} from the local archive? Deleted sessions can't be restored.`,
      onConfirm: async () => {
        try {
          for (const id of session.mergedIds) await SessionRepository.deleteSession(id);
        } catch (e) {
          uploads.setNote(row, { ok: false, text: `Delete failed: ${errorText(e)}` });
        }
        if (openId === session.id) setOpenId(null);
        reload();
      },
    });
  };

  // Always every saved session, whatever time frame is showing.
  const exportCsv = () =>
    void exportAction.run(() => invoke<string>("export_sessions_csv"), {
      done: (path) => `Exported sessions to ${path}`,
    });

  const rangeLabel = day ? new Date(`${day}T00:00:00`).toLocaleDateString() : RANGES.find((r) => r.id === range)?.label;

  return (
    <Page
      codename="SESSION_ARCHIVE"
      title="History"
      actions={
        <>
          <GhostButton
            onClick={exportCsv}
            disabled={exportAction.pending}
            title="Export every saved session to a CSV file in Downloads, regardless of the time frame selected"
          >
            {exportAction.pending ? "Exporting…" : "Export CSV"}
          </GhostButton>
          <div role="group" aria-label="Time frame" className="flex overflow-hidden rounded border border-white/10">
            {RANGES.map((r) => (
              <button
                key={r.id}
                type="button"
                aria-pressed={!day && range === r.id}
                onClick={() => {
                  setRange(r.id);
                  setDay(null);
                }}
                className={cx(
                  "px-2.5 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.12em] transition-colors",
                  !day && range === r.id ? "bg-neon-cyan/15 text-neon-cyan" : "text-white/45 hover:bg-white/5 hover:text-white",
                )}
              >
                {r.label}
              </button>
            ))}
          </div>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <ActionStatus
          pending={exportAction.pending && "Exporting every session…"}
          error={exportAction.error && `Could not export sessions: ${exportAction.error}`}
          done={exportAction.done}
        />
        {weekly && <WeeklyPanel weekly={weekly} />}
        {deskSync && <DeskSyncPanel sync={deskSync} />}
        {todayTimeline && <TodayPanel timeline={todayTimeline} />}
        {wallClock && <WallClockPanel clock={wallClock} />}
        <TotalsPanel meters={sum.meters} seconds={sum.seconds} kcal={sum.kcal} count={visible.length} />

        <Panel className="p-3">
          <div className="mb-2 flex items-center justify-between">
            <Label>Past year</Label>
            <span className="font-mono text-[11px] text-white/40">Click a day to show only its sessions</span>
          </div>
          <Calendar metersByDay={byDay} selected={day} onSelect={setDay} />
        </Panel>

        <div className="flex items-center justify-between">
          <Label>Sessions · {rangeLabel}</Label>
          {day && (
            <button
              type="button"
              onClick={() => setDay(null)}
              className="font-mono text-[11px] text-neon-magenta hover:underline"
            >
              Clear day filter
            </button>
          )}
        </div>

        {loadError ? (
          <p className="py-6 text-center font-mono text-xs text-red-400">Could not load sessions: {loadError}</p>
        ) : !sessions ? (
          <p className="py-6 text-center font-mono text-xs text-white/40">Loading sessions…</p>
        ) : groups.length === 0 ? (
          <p className="py-6 text-center font-mono text-xs text-white/40">
            {sessions.length === 0
              ? "No sessions recorded yet. Every walk shows up in History."
              : "No sessions in this time frame."}
          </p>
        ) : (
          groups.map((g) => (
            <section key={g.key} className="flex flex-col gap-1.5">
              <h2 className="px-1 font-mono text-xs font-bold tracking-[0.08em] text-white/60">{g.heading}</h2>
              {g.items.map((s) => {
                const row = s.id ?? 0;
                const up = uploads.rowUpload(s, row);
                return (
                  <SessionRow
                    key={row}
                    session={s}
                    expanded={openId === s.id}
                    stravaLinked={uploads.stravaConnected}
                    uploadState={up.state}
                    error={up.error}
                    step={up.step}
                    note={uploads.noteFor(row)}
                    saved={s.mergedIds.some((id) => savedIds.includes(id))}
                    live={activeSessionId != null && s.mergedIds.includes(activeSessionId)}
                    onToggle={() => setOpenId(openId === s.id ? null : (s.id ?? null))}
                    onUpload={() => uploads.upload(s, row)}
                    onSave={() => invoke<SavedFile>("strava_save_for_strava", { sessionIds: s.mergedIds })}
                    onOpenFolder={() => dispatch({ type: "strava_open_save_folder" })}
                    onMarkUploaded={async () => {
                      await dispatch({ type: "strava_mark_uploaded", session_ids: s.mergedIds });
                      reload();
                    }}
                    onInspect={() => void inspect(s, row)}
                    onDelete={() => remove(s, row)}
                    onEcho={() => invoke<EchoProgram>("program_from_session", { sessionIds: s.mergedIds })}
                    onOpenProgram={openProgram}
                  />
                );
              })}
            </section>
          ))
        )}
      </div>

      {tcx && <TcxModal session={tcx.session} tcx={tcx.text} onClose={() => setTcx(null)} />}
    </Page>
  );
}
