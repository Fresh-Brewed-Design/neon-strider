import { useCallback, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionStatus } from "../../components/ActionStatus";
import { errorText, useAction } from "../../components/useAction";
import { dispatch, type EchoProgram, type SavedFile } from "../../ipc";
import { SessionRepository } from "../../services/db/repositories/sessionRepository";
import { mergeSessions, type MergedSession } from "../../services/sessionMerge";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { Calendar } from "../history/Calendar";
import { DeskSyncPanel } from "../history/DeskSync";
import { groupByDay, RANGES, visibleSessions, type Range } from "../history/sessions";
import { SessionRow } from "../history/SessionRow";
import { TotalsPanel, WeeklyPanel } from "../history/Summary";
import { TcxModal } from "../history/TcxModal";
import { TodayPanel } from "../history/Today";
import { monthName, useHistoryWalks } from "../history/useHistoryWalks";
import { useSessionUploads } from "../history/useSessionUploads";
import { WallClockPanel } from "../history/WallClock";
import { GhostButton, Label, NeonButton, OptionPicker, Page, Panel } from "../ui";
import { cx } from "../util";

/** What each time frame's totals are called. */
const TOTALS_HEADING: Record<Range, string> = {
  "7d": "Last 7 days",
  "30d": "Last 30 days",
  "1y": "Past year",
  all: "All time",
};

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

  const [reloads, setReloads] = useState(0);
  const [range, setRange] = useState<Range>("7d");
  const [day, setDay] = useState<string | null>(null);
  const [openId, setOpenId] = useState<number | null>(null);
  const [tcx, setTcx] = useState<{ session: MergedSession; text: string } | null>(null);
  const exportAction = useAction();
  const reload = useCallback(() => setReloads((n) => n + 1), []);
  const uploads = useSessionUploads(reload);

  const walks = useHistoryWalks(range, day, `${refreshTrigger}:${reloads}:${thresholdMinutes}`);
  const { raw, stats, loadError, older } = walks;

  // Newest first.
  const sessions = useMemo(() => (raw ? mergeSessions(raw, thresholdMinutes).reverse() : null), [raw, thresholdMinutes]);
  const byDay = useMemo(() => new Map((stats?.meters_by_day ?? []).map((d) => [d.day, d.meters])), [stats]);
  const visible = useMemo(() => visibleSessions(sessions ?? [], range, day), [sessions, range, day]);
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
      confirmLabel: "Delete",
      tone: "danger",
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

  // Nothing was ever recorded: the page leads with that, and the cards of zeros stay out.
  const nothingYet = sessions !== null && sessions.length === 0 && stats !== null && stats.oldest_start_ms == null && !loadError;
  const totalsHeading = day ? new Date(`${day}T00:00:00`).toLocaleDateString() : (TOTALS_HEADING[range] ?? "");
  const rangeLabel = day ? new Date(`${day}T00:00:00`).toLocaleDateString() : RANGES.find((r) => r.id === range)?.label;

  return (
    <Page
      codename="SESSION_ARCHIVE"
      title="History"
      actions={
        <>
          <NeonButton
            className="h-7 px-3"
            onClick={exportCsv}
            disabled={exportAction.pending}
            title="Saves every session to a CSV file in Downloads, whatever time frame shows"
          >
            {exportAction.pending ? "Exporting…" : "Export CSV"}
          </NeonButton>
          <OptionPicker
            label="Time frame"
            className="flex flex-wrap overflow-hidden rounded border border-white/10"
            options={RANGES.map((r) => ({ value: r.id, label: r.label }))}
            value={day ? null : range}
            onChange={(next) => {
              setRange(next);
              setDay(null);
            }}
            optionClassName={(active) =>
              cx(
                "px-2.5 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.12em] transition-colors",
                active ? "bg-neon-cyan/15 text-neon-cyan" : "text-white/60 hover:bg-white/5 hover:text-white",
              )
            }
          />
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <ActionStatus
          pending={exportAction.pending && "Exporting every session…"}
          error={exportAction.error && `Could not export sessions: ${exportAction.error}`}
          done={exportAction.done}
        />
        {nothingYet && (
          <Panel className="p-4 text-center" >
            <p className="font-mono text-xs text-white/80" data-testid="history-empty">
              No walks yet. ENGAGE on Live or run a program to record one.
            </p>
          </Panel>
        )}
        {weekly && !nothingYet && <WeeklyPanel weekly={weekly} />}
        {deskSync && (!nothingYet || deskSync.desk_days > 0) && <DeskSyncPanel sync={deskSync} />}
        {todayTimeline && <TodayPanel timeline={todayTimeline} />}
        {wallClock && (!nothingYet || wallClock.desk_days > 0) && <WallClockPanel clock={wallClock} />}
        {stats && !nothingYet && (
          <TotalsPanel
            heading={totalsHeading}
            meters={stats.totals.meters}
            seconds={stats.totals.seconds}
            kcal={stats.totals.kcal}
            count={stats.totals.sessions}
          />
        )}

        {!nothingYet && (
          <Panel className="p-3">
            <div className="mb-2 flex flex-wrap items-center justify-between gap-x-3">
              <Label>Past year</Label>
              <span className="font-mono text-xs text-white/60">Click a day to show only its sessions</span>
            </div>
            <Calendar metersByDay={byDay} selected={day} onSelect={setDay} />
          </Panel>
        )}

        {!nothingYet && (
        <div className="flex flex-wrap items-center justify-between gap-x-3">
          <Label>Sessions · {rangeLabel}</Label>
          {day && (
            <button
              type="button"
              onClick={() => setDay(null)}
              className="font-mono text-[11px] text-neon-cyan hover:underline"
            >
              Clear day filter
            </button>
          )}
        </div>
        )}

        {loadError ? (
          <p className="py-6 text-center font-mono text-xs text-red-400">Could not load sessions: {loadError}</p>
        ) : !sessions || walks.dayLoading ? (
          <p className="py-6 text-center font-mono text-xs text-white/60">Loading sessions…</p>
        ) : nothingYet ? null : groups.length === 0 ? (
          <p className="py-6 text-center font-mono text-xs text-white/60">No sessions in the chosen time frame.</p>
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

        {sessions && (walks.olderToLoad || older.pending || older.done || older.error) && (
          <div data-testid="history-older" className="flex flex-col items-start gap-1.5 pt-1">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
              {walks.olderToLoad && (
                <GhostButton onClick={walks.loadOlder} disabled={older.pending}>
                  Load older walks
                </GhostButton>
              )}
              <span className="font-mono text-xs text-white/60">
                Walks listed back to {monthName(walks.loadedFrom)}.
              </span>
            </div>
            <ActionStatus
              pending={older.pending && "Loading older walks…"}
              error={older.error && `Could not load older walks: ${older.error}`}
              done={older.done}
            />
          </div>
        )}
      </div>

      {tcx && <TcxModal session={tcx.session} tcx={tcx.text} onClose={() => setTcx(null)} />}
    </Page>
  );
}
