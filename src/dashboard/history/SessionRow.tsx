import type { ReactNode } from "react";
import {
  AlertCircle,
  ChevronDown,
  ChevronRight,
  CheckCheck,
  CheckCircle2,
  CloudUpload,
  FileDown,
  FileSearch,
  Loader2,
  Trash2,
} from "lucide-react";
import { ActionStatus, InlineAction } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import type { SavedFile } from "../../ipc";
import type { MergedSession } from "../../services/sessionMerge";
import { GhostButton, Panel } from "../ui";
import { cx } from "../util";
import { formatDuration, km } from "../../format";
import { SessionScope } from "./SessionScope";

export type UploadState = "idle" | "uploading" | "success" | "error";

function Stat({ value, unit, tone }: { value: ReactNode; unit: string; tone?: string }) {
  return (
    <span className="flex items-baseline gap-1 whitespace-nowrap">
      <span className={cx("font-digital text-sm", tone ?? "text-white/90")}>{value}</span>
      {unit && <span className="font-mono text-[11px] text-white/40">{unit}</span>}
    </span>
  );
}

function MarkUploaded({ pending, onClick, className }: { pending: boolean; onClick: () => void; className?: string }) {
  return (
    <button
      type="button"
      disabled={pending}
      onClick={onClick}
      className={cx(
        "flex items-center gap-1 font-bold uppercase tracking-[0.12em] text-neon-cyan hover:underline disabled:opacity-40",
        className,
      )}
    >
      {pending ? <Loader2 size={12} className="animate-spin" /> : <CheckCheck size={12} />}
      Mark as on Strava
    </button>
  );
}

/**
 * One (possibly merged) session: its totals, Strava upload and save buttons,
 * and when expanded, the speed and incline chart and details.
 */
export function SessionRow({
  session: s,
  expanded,
  stravaLinked,
  uploadState,
  live = false,
  error = null,
  step = null,
  note = null,
  saved = false,
  onToggle,
  onUpload,
  onSave,
  onOpenFolder,
  onMarkUploaded,
  onInspect,
  onDelete,
}: {
  session: MergedSession;
  expanded: boolean;
  stravaLinked: boolean;
  uploadState: UploadState;
  /** The session is still being recorded, so it can't be uploaded yet. */
  live?: boolean;
  /** Why the last upload failed, in plain words. */
  error?: string | null;
  /** What the running upload is doing, for the spinner's tooltip. */
  step?: string | null;
  /** A short result for this row (upload finished, preview failed). */
  note?: { ok: boolean; text: string } | null;
  /** Saved as a file for a manual Strava upload, not yet marked as on Strava. */
  saved?: boolean;
  onToggle: () => void;
  onUpload: () => void;
  onSave: () => Promise<SavedFile>;
  onOpenFolder: () => Promise<unknown>;
  onMarkUploaded: () => Promise<unknown>;
  onInspect: () => void;
  onDelete: () => void;
}) {
  const uploaded = !!s.strava_id || uploadState === "success";
  const save = useAction();
  const mark = useAction();
  const folder = useAction();
  const showSavedLine = saved && !uploaded;
  const time = new Date(s.start_time).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const wpm = s.neural_ticks > 0 ? Math.round((s.neural_heat / s.neural_ticks) * 2.4) : 0;
  const rawDelta =
    s.raw_distance_meters > 0 && s.distance_meters > 0
      ? Math.round((s.raw_distance_meters / s.distance_meters - 1) * 100)
      : 0;

  return (
    <Panel className={cx("overflow-hidden", expanded && "border-neon-cyan/40")}>
      <div className="flex items-center gap-1 pr-2">
        <button
          type="button"
          aria-expanded={expanded}
          onClick={onToggle}
          className="flex min-w-0 flex-1 items-center gap-3 px-3 py-2.5 text-left transition-colors hover:bg-white/[0.03]"
        >
          {expanded ? (
            <ChevronDown size={14} className="shrink-0 text-neon-cyan" />
          ) : (
            <ChevronRight size={14} className="shrink-0 text-white/40" />
          )}
          <span className="shrink-0 whitespace-nowrap font-mono text-xs text-white/60">{time}</span>
          {/* Wraps to a second line rather than overlapping when the row is narrow. */}
          <span className="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-4 gap-y-0.5">
            <Stat value={km(s.distance_meters)} unit="km" tone="text-neon-cyan" />
            <Stat value={formatDuration(s.duration_seconds)} unit="" />
            <Stat value={Math.round(s.calories)} unit="kcal" />
            <Stat value={s.avg_speed > 0 ? s.avg_speed.toFixed(1) : "-"} unit="km/h" />
          </span>
          {s.mergedIds.length > 1 && (
            <span
              title={`${s.mergedIds.length} sessions joined into one walk`}
              className="shrink-0 rounded border border-white/15 px-1.5 font-mono text-[11px] text-white/50"
            >
              ×{s.mergedIds.length}
            </span>
          )}
        </button>

        <div className="flex shrink-0 items-center">
          {stravaLinked && (
            <GhostButton
              aria-label={uploaded ? "Uploaded to Strava" : "Upload to Strava"}
              title={
                uploaded
                  ? "Uploaded to Strava"
                  : live
                    ? "Still recording. Upload once the walk has ended."
                    : uploadState === "uploading"
                      ? (step ?? "Uploading to Strava…")
                      : uploadState === "error"
                        ? `${error ?? "Upload failed."} Click to try again.`
                        : "Upload to Strava"
              }
              disabled={uploaded || live || uploadState === "uploading"}
              onClick={onUpload}
              className={cx(
                "px-2!",
                uploaded && "text-emerald-400! opacity-100!",
                uploadState === "error" && "text-red-400!",
              )}
            >
              {uploadState === "uploading" ? (
                <Loader2 size={15} className="animate-spin" />
              ) : uploaded ? (
                <CheckCircle2 size={15} />
              ) : uploadState === "error" ? (
                <AlertCircle size={15} />
              ) : (
                <CloudUpload size={15} />
              )}
            </GhostButton>
          )}
          {!uploaded && (
            <GhostButton
              aria-label="Save for Strava"
              title={
                live
                  ? "Still recording. Save once the walk has ended."
                  : "Save the session as a TCX file for Strava's upload page, which works without a linked account."
              }
              disabled={live || save.pending}
              onClick={() =>
                void save.run(onSave, {
                  done: (f) => `Saved ${f.file_name}. Drop it on the Strava page that just opened.`,
                })
              }
              className={cx("px-2!", saved && "text-neon-cyan!")}
            >
              {save.pending ? <Loader2 size={15} className="animate-spin" /> : <FileDown size={15} />}
            </GhostButton>
          )}
          <GhostButton aria-label="Preview TCX" title="Preview the TCX file" onClick={onInspect} className="px-2!">
            <FileSearch size={15} />
          </GhostButton>
          <GhostButton
            aria-label="Delete session"
            title="Delete"
            onClick={onDelete}
            className="px-2! hover:bg-red-500/10! hover:text-red-400!"
          >
            <Trash2 size={15} />
          </GhostButton>
        </div>
      </div>

      {(error || note || save.pending || save.error || save.done || showSavedLine || mark.error || folder.error) && (
        <div className="flex flex-col gap-1 border-t border-white/5 px-3 py-1.5">
          {uploadState !== "uploading" && error && (
            <ActionStatus error={error}>
              {stravaLinked && !live && <InlineAction onClick={onUpload}>Try again</InlineAction>}
            </ActionStatus>
          )}
          {!error && note && (
            <ActionStatus error={note.ok ? null : note.text} done={note.ok ? note.text : null} />
          )}
          <ActionStatus pending={save.pending && "Writing the file…"} error={save.error} done={save.done}>
            {save.done && <InlineAction onClick={() => void folder.run(onOpenFolder)}>Open folder</InlineAction>}
          </ActionStatus>
          {showSavedLine && !save.done && !save.pending && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] text-white/55">
              <span>Saved for Strava. After uploading the file to Strava:</span>
              <MarkUploaded pending={mark.pending} onClick={() => void mark.run(onMarkUploaded)} />
            </div>
          )}
          {save.done && showSavedLine && (
            <MarkUploaded
              pending={mark.pending}
              onClick={() => void mark.run(onMarkUploaded)}
              className="w-fit font-mono text-[11px]"
            />
          )}
          <ActionStatus error={mark.error ?? folder.error} />
        </div>
      )}

      {expanded && (
        <div className="border-t border-white/10 px-3 pb-3 pt-3">
          <SessionScope session={s} />
          <dl className="mt-2 flex flex-wrap gap-x-5 gap-y-1 font-mono text-[11px] text-white/50">
            {s.steps > 0 && <div>Steps <span className="text-neon-magenta">{Math.floor(s.steps)}</span></div>}
            {s.avg_incline > 0 && <div>Avg incline <span className="text-white/80">{s.avg_incline.toFixed(1)}%</span></div>}
            {s.avg_heart_rate !== null && (
              <div>
                Avg heart rate <span className="text-white/80">{s.avg_heart_rate} bpm</span>
              </div>
            )}
            {s.max_heart_rate !== null && (
              <div>
                Max heart rate <span className="text-white/80">{s.max_heart_rate} bpm</span>
              </div>
            )}
            {rawDelta !== 0 && (
              <div>
                Deck reported <span className="text-white/80">{km(s.raw_distance_meters)} km</span> ({rawDelta > 0 ? "+" : ""}
                {rawDelta}% before calibration)
              </div>
            )}
            {wpm > 0 && <div>Typing <span className="text-white/80">{wpm} WPM</span></div>}
            {uploaded && <div className="text-emerald-400">On Strava</div>}
          </dl>
        </div>
      )}
    </Panel>
  );
}
