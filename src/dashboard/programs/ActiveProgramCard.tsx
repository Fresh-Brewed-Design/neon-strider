import { ChartNoAxesColumn, Pause, Play, SkipForward, Square } from "lucide-react";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton, Label, Panel } from "../ui";
import { cx, send } from "../util";
import { describeActive, waitHeadline, waitReason } from "./format";

/** The Live page's STEP_MAP switch: whether the step map shows in place of the stats. */
export interface StepMapToggle {
  open: boolean;
  onToggle: () => void;
}

/**
 * The running program, with pause/resume, skip and end. Renders nothing when
 * idle. With `stepMap` (the Live page) it also offers the STEP_MAP switch.
 */
export function ActiveProgramCard({ stepMap }: { stepMap?: StepMapToggle }) {
  const s = useAppSnapshot();
  const a = s?.active_program;
  if (!a) return null;
  // Waiting for GO: the next step would start a still belt, so nothing moves until GO.
  const wait = a.wait;

  return (
    <Panel
      className={cx(
        // The status lines sit in a row of their own under the buttons, so the buttons never squeeze them onto two lines.
        "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 px-4 py-2",
        wait ? "border-l-4 border-neon-yellow bg-neon-yellow/10" : "border-neon-yellow/30",
      )}
    >
      <div className="min-w-0">
        {wait ? (
          <div role="alert" className="break-words font-digital text-xs font-bold tracking-widest text-neon-yellow">{waitHeadline(wait)}</div>
        ) : (
          <div className="flex min-w-0 items-baseline gap-2" data-testid="program-label-line">
            <Label className="text-neon-yellow/70">{a.paused ? "Program paused" : "Program running"}</Label>
            {a.phase === "walking" && (
              <span className="shrink-0 whitespace-nowrap font-mono text-xs text-white/40">
                {a.speed_kmh.toFixed(1)}{" "}km/h · {a.incline_pct.toFixed(1)}%
              </span>
            )}
          </div>
        )}
        <div className="line-clamp-2 break-words font-digital text-base font-bold leading-tight tracking-wider text-neon-yellow">{a.name}</div>
      </div>
      <div className="flex items-center gap-1">
        {stepMap && (
          <GhostButton
            onClick={stepMap.onToggle}
            aria-pressed={stepMap.open}
            className={cx("flex items-center gap-1.5", stepMap.open && "bg-neon-yellow/10 text-neon-yellow")}
            title={stepMap.open ? "Show the stats in place of the step map." : "Show every step of the program in place of the stats."}
          >
            <ChartNoAxesColumn size={14} /> Map
          </GhostButton>
        )}
        {wait ? (
          <button
            type="button"
            onClick={() => send({ type: "program_resume" })}
            className="flex items-center gap-1.5 rounded bg-neon-yellow px-3 py-1.5 font-digital text-xs font-bold tracking-widest text-black shadow-[0_0_14px_rgba(255,243,0,0.6)] hover:shadow-[0_0_20px_rgba(255,243,0,0.9)]"
            title="Start the belt and the next walk"
          >
            <Play size={14} /> GO
          </button>
        ) : a.paused ? (
          <GhostButton onClick={() => send({ type: "program_resume" })} className="flex items-center gap-1.5 text-neon-yellow">
            <Play size={14} /> Resume
          </GhostButton>
        ) : (
          <GhostButton onClick={() => send({ type: "program_pause" })} className="flex items-center gap-1.5">
            <Pause size={14} /> Pause
          </GhostButton>
        )}
        <GhostButton onClick={() => send({ type: "program_skip" })} className="flex items-center gap-1.5" title="Skip to the next step">
          <SkipForward size={14} /> Skip
        </GhostButton>
        <GhostButton
          onClick={() => send({ type: "program_end" })}
          className="flex items-center gap-1.5 hover:text-neon-magenta"
          title="End the program and go back to the earlier speed."
        >
          <Square size={14} /> End
        </GhostButton>
      </div>
      <div className="col-span-2 min-w-0">
        <div className="font-mono text-xs text-white/60">{describeActive(a)}</div>
        {wait && <div className="font-mono text-xs text-white/70">{waitReason(wait)}</div>}
      </div>
    </Panel>
  );
}
