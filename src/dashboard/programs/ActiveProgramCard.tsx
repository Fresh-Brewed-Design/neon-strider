import { CalendarClock, ChartNoAxesColumn, Pause, Play, SkipForward, Square } from "lucide-react";
import type { ReactNode } from "react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction, useBackendAction } from "../../components/useAction";
import { isMoving, startOf } from "../../state/linkStatus";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton, Joined, Label, Panel } from "../ui";
import { cx, post } from "../util";
import { describeActive, describeTimeLeft, waitHeadline, waitReason } from "./format";

/** The Live page's STEP_MAP switch: whether the step map shows in place of the stats. */
export interface StepMapToggle {
  open: boolean;
  onToggle: () => void;
}

/**
 * The Live page's choice of STEP_MAP or the stats under the program: a two-way switch, drawn
 * apart from HOLD, SKIP and END because it changes the view, not the program.
 */
function ViewSwitch({ open, onToggle }: StepMapToggle) {
  const option = (on: boolean) =>
    cx(
      "flex items-center gap-1 px-2 uppercase leading-[14px] transition-colors",
      on ? "bg-neon-yellow/15 text-neon-yellow" : "text-white/60 hover:bg-white/5 hover:text-white",
    );
  return (
    <div
      role="group"
      aria-label="Step map or stats"
      className="flex shrink-0 overflow-hidden rounded border border-white/15 font-mono text-[11px] font-bold tracking-[0.12em]"
    >
      <button
        type="button"
        aria-pressed={open}
        onClick={open ? undefined : onToggle}
        className={option(open)}
        title="Show every step of the program in place of the stats."
      >
        <ChartNoAxesColumn size={12} aria-hidden /> Map
      </button>
      <button
        type="button"
        aria-pressed={!open}
        onClick={open ? onToggle : undefined}
        className={cx(option(!open), "border-l border-white/15")}
        title="Show the stats in place of the step map."
      >
        Stats
      </button>
    </div>
  );
}

/** What HOLD or GO does to the program, in plain words beside the codename (as on the HUD). */
function ButtonSub({ children }: { children: string }) {
  return <span className="font-mono text-[11px] font-normal normal-case tracking-normal opacity-70">{children}</span>;
}

/**
 * A codename over its plain words, as on the Live page's Engage, so four buttons fit beside the
 * program's name at 760 px (#428). The space between them stays for copying and screen readers.
 */
function Stack({ children }: { children: ReactNode }) {
  return <span className="flex flex-col items-center leading-tight">{children}</span>;
}

/**
 * The running program, with pause/resume, skip and end. Renders nothing when
 * idle. With `stepMap` (the Live page) it also offers the STEP_MAP switch.
 */
export function ActiveProgramCard({ stepMap }: { stepMap?: StepMapToggle }) {
  const s = useAppSnapshot();
  // GO, Pause, Skip and End share one status line under the buttons. GO waits for the deck to
  // answer like Engage: a start it made, and its failure, show here and not under Engage.
  const control = useAction();
  const own = s ? startOf(s, "program_go") : null;
  const go = useBackendAction(own?.result ?? null, own?.pending ?? false);
  const a = s?.active_program;
  if (!s || !a) return null;
  const goWaiting = (go.pending || (own?.pending ?? false)) && !isMoving(s.deck);
  const busy = control.pending || goWaiting;
  const pressGo = () => void go.run(() => post({ type: "program_resume" })());
  // Waiting for GO: the next step would start a still belt, so nothing moves until GO.
  const wait = a.wait;
  const timeLeft = describeTimeLeft(a);

  return (
    <Panel
      className={cx(
        // The status lines sit in a row of their own under the buttons, so the buttons never squeeze them onto two lines.
        // The name keeps at least 10rem beside the buttons; with less room (a large Windows text
        // size) the buttons wrap under it instead of crushing it.
        "flex flex-wrap items-center gap-x-2 px-3 py-2",
        wait ? "border-l-4 border-neon-yellow bg-neon-yellow/10" : "border-neon-yellow/30",
      )}
    >
      <div className="min-w-0 flex-1 basis-40">
        {wait ? (
          <div role="alert" className="break-words font-digital text-xs font-bold tracking-wider text-neon-yellow">{waitHeadline(wait)}</div>
        ) : (
          <div className="flex min-w-0 flex-wrap items-baseline gap-x-2" data-testid="program-label-line">
            <Label className="shrink-0 whitespace-nowrap !tracking-[0.12em] text-neon-yellow/70">{a.paused ? "Program paused" : "Program running"}</Label>
            {a.phase === "walking" && (
              <span className="shrink-0 whitespace-nowrap font-mono text-xs text-white/60">
                {a.speed_kmh.toFixed(1)}{" "}km/h · {a.incline_pct.toFixed(1)}%
              </span>
            )}
          </div>
        )}
        <div className="line-clamp-2 break-words font-digital text-base font-bold leading-tight tracking-wider text-neon-yellow">{a.name}</div>
      </div>
      {/* Four buttons beside the name at 760 px: GO and HOLD stack their words, and Skip, Pass and End keep their padding small (#428). */}
      <div className="flex flex-wrap items-center">
        {wait ? (
          <button
            type="button"
            disabled={busy}
            onClick={pressGo}
            className="flex items-center gap-1.5 rounded bg-neon-yellow px-2 py-0.5 disabled:opacity-60 font-digital text-xs font-bold tracking-widest text-black shadow-[0_0_14px_rgba(255,243,0,0.6)] hover:shadow-[0_0_20px_rgba(255,243,0,0.9)]"
            title="Start the belt and the next walk"
          >
            <Play size={14} /> <Stack>GO <span className="font-mono text-[11px] font-normal normal-case tracking-normal">start belt</span></Stack>
          </button>
        ) : a.paused ? (
          <GhostButton
            disabled={busy}
            onClick={pressGo}
            className="flex items-center gap-1.5 text-neon-yellow"
            title="Resume the program."
          >
            <Play size={14} /> <Stack>GO <ButtonSub>resume program</ButtonSub></Stack>
          </GhostButton>
        ) : (
          <GhostButton
            disabled={control.pending}
            onClick={() => void control.run(post({ type: "program_pause" }))}
            className="flex items-center gap-1.5"
            title="Hold the program."
          >
            <Pause size={14} /> <Stack>HOLD <ButtonSub>pause program</ButtonSub></Stack>
          </GhostButton>
        )}
        <GhostButton
          disabled={control.pending}
          onClick={() => void control.run(post({ type: "program_skip" }))}
          className="flex items-center gap-1 px-1.5"
          title="Skip to the next step"
        >
          <SkipForward size={14} /> Skip
        </GhostButton>
        {/* PASS: only a repeating program has a cycle to sit out. */}
        {a.repeat_every_minutes != null && (
          <GhostButton
            disabled={control.pending}
            onClick={() => void control.run(post({ type: "program_pass" }))}
            className="flex items-center gap-1 px-1.5"
            title="Skip one cycle and keep the schedule."
          >
            <CalendarClock size={14} /> Pass
          </GhostButton>
        )}
        <GhostButton
          disabled={control.pending}
          onClick={() => void control.run(post({ type: "program_end" }))}
          className="flex items-center gap-1 px-1.5 hover:text-neon-yellow"
          title="End the program and go back to the earlier speed."
        >
          <Square size={14} /> End
        </GhostButton>
      </div>
      <ActionStatus
        pending={goWaiting && "Waiting for the treadmill to start…"}
        error={control.error ?? go.error ?? own?.failure}
        className="basis-full justify-end text-right"
      />
      {/* The step lines, with the Live page's view switch at their end, a row apart from the program's buttons. */}
      <div className="flex min-w-0 basis-full flex-wrap items-end justify-between gap-x-3 gap-y-1">
        <div className="min-w-0 flex-1 basis-48">
          {/* The step and the time left share a line while they fit, and wrap apart when they do not. */}
          <div className="flex flex-wrap gap-x-3">
            <Joined className="font-mono text-xs text-white/60" text={describeActive(a)} />
            {timeLeft && (
              <div data-testid="program-time-left">
                <Joined className="font-mono text-xs text-neon-yellow/70" text={timeLeft} />
              </div>
            )}
          </div>
          {wait && <div className="font-mono text-xs text-white/70">{waitReason(wait)}</div>}
          {/* What PASS did, in Rust's words, under the buttons until the next cycle begins. */}
          {!wait && a.pass_note && (
            <div role="status" data-testid="program-pass-note" className="font-mono text-xs text-emerald-400">
              {a.pass_note.sentence}
            </div>
          )}
        </div>
        {stepMap && <ViewSwitch {...stepMap} />}
      </div>
    </Panel>
  );
}
