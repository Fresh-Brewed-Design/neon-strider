import { useState } from "react";
import { Pencil, Play, Trash2 } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch, type Command, type FitToDeckOffer, type FitToTimeOption, type Program } from "../../ipc";
import { GhostButton, NeonButton, Panel } from "../ui";
import { cx, post } from "../util";
import { FitToDeck } from "./FitToDeck";
import { describePlan, describeProgram } from "./format";

/** The `program_run` a press sends: the picked length, or none to run the program as saved. */
function runCommand(id: number, minutes: number | null): Command {
  return minutes === null ? { type: "program_run", id } : { type: "program_run", id, minutes };
}

/**
 * One program on the Programs page. FIT_TO_TIME (#258) sits beside Run: Rust
 * lists the lengths the program may run at once (`snapshot.fit_to_time`),
 * the first choice runs it as saved, and the pick goes with the Run press.
 * The belt only starts on that press; the saved program never changes.
 */
export function ProgramRow({
  program: p,
  running,
  lastRun,
  connected,
  fitToDeck,
  fitToTime,
  onEdit,
  onDelete,
}: {
  program: Program;
  running: boolean;
  /** Rust's `snapshot.program_last_run_id` names this program (#336). */
  lastRun: boolean;
  connected: boolean;
  fitToDeck: FitToDeckOffer | null;
  fitToTime: FitToTimeOption | null;
  onEdit: () => void;
  /** Asks before deleting; `onConfirm` sends the delete. */
  onDelete: (onConfirm: () => void) => void;
}) {
  const [picked, setPicked] = useState<number | null>(null);
  const run = useAction();
  const remove = useAction();
  // A pick Rust no longer offers (the program was edited) falls back to the saved length.
  const minutes = picked !== null && fitToTime?.lengths.includes(picked) ? picked : null;
  const note = minutes !== null ? (fitToTime?.note.split("\n") ?? []) : [];
  return (
    <Panel className={cx("flex flex-col gap-1.5 px-4 py-3", running && "border-neon-yellow/40")}>
      {/* The text keeps at least 12rem beside the buttons; with less room (a large Windows text
          size) the buttons wrap under it, so the name never breaks one letter per line. */}
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-0 flex-1 basis-48" title={describePlan(p)}>
          <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
            <div className="min-w-0 break-words font-digital text-sm font-bold leading-tight tracking-wider text-white">{p.name}</div>
            {/* The running row already reads Running, so the tag only marks a program at rest. */}
            {lastRun && !running && (
              <span
                className="shrink-0 rounded border border-neon-cyan/60 px-1 font-mono text-[11px] font-bold leading-tight tracking-[0.12em] text-neon-cyan"
                title="The program run last"
              >
                LAST_RUN
              </span>
            )}
          </div>
          <div className="font-mono text-xs text-white/60">{describeProgram(p)}</div>
          {p.note && <div className="text-xs leading-snug text-white/60">{p.note}</div>}
          <FitToDeck offer={fitToDeck} />
        </div>
        <div className="ml-auto flex max-w-full flex-wrap items-end justify-end gap-3">
          <GhostButton aria-label={`Edit ${p.name}`} title={`Edit ${p.name}`} className="min-h-6 min-w-6" onClick={onEdit}>
            <Pencil size={14} />
          </GhostButton>
          <GhostButton
            aria-label={`Delete ${p.name}`}
            title={`Delete ${p.name}`}
            className="ml-2 min-h-6 min-w-6 hover:text-neon-magenta"
            disabled={remove.pending}
            onClick={() => onDelete(() => void remove.run(post({ type: "program_delete", id: p.id })))}
          >
            <Trash2 size={14} />
          </GhostButton>
          {fitToTime && (
            <label className="flex shrink-0 flex-col gap-0.5">
              <span className="font-mono text-[11px] font-bold uppercase leading-none tracking-[0.2em] text-white/60">Length</span>
            <select
              aria-label={`Length of ${p.name}`}
              title={`How long ${p.name} runs`}
              className="h-9 w-24 shrink-0 rounded border border-white/15 bg-black/40 px-1.5 font-mono text-xs text-white focus:border-neon-cyan focus:outline-none disabled:opacity-40"
              value={minutes ?? ""}
              disabled={running || run.pending}
              onChange={(e) => {
                setPicked(e.target.value === "" ? null : Number(e.target.value));
                run.reset();
              }}
            >
              <option value="">{fitToTime.saved_label}</option>
              {fitToTime.lengths.map((m) => (
                <option key={m} value={m}>
                  {m} min
                </option>
              ))}
            </select>
            </label>
          )}
          <NeonButton
            tone="yellow"
            className="flex h-9 w-24 shrink-0 items-center justify-center gap-1.5"
            disabled={!connected || running || run.pending}
            title={connected ? describePlan(p) : "Connect the treadmill first"}
            onClick={() => void run.run(() => dispatch(runCommand(p.id, minutes)))}
          >
            {running ? "Running" : (
              <>
                <Play size={14} /> Run
              </>
            )}
          </NeonButton>
        </div>
      </div>
      {note.length > 0 && !running && (
        <div className="font-mono text-xs leading-snug text-white/60" data-testid="fit-to-time-note">
          {note.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      )}
      <ActionStatus pending={remove.pending} error={remove.error} className="justify-end text-right" />
      <ActionStatus pending={run.pending} error={run.error} className="justify-end text-right" />
    </Panel>
  );
}
