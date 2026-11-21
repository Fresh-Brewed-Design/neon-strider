import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, Plus, Trash2, X } from "lucide-react";
import {
  DEFAULT_APP_CONFIG,
  dispatch,
  type CalibrationLimits,
  type CalibrationRunView,
  type CalibrationSweepView,
  type SweepMark,
} from "../../../ipc";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { useAppSnapshot } from "../../../state/useAppState";
import { useTreadmillStore } from "../../../store/treadmillStore";
import { useGuidedTask } from "../../guided/useGuidedTask";
import { GhostButton, Hint, Label, NeonButton } from "../../ui";
import { cx, send } from "../../util";
import { INPUT_CLASS } from "../fields";
import {
  calibrationRows,
  chrono,
  deckVerdict,
  droppedLine,
  listedTarget,
  runRowFor,
  settingSteps,
  type CalibrationRow,
} from "./guided";
import { PointsAside, StartFresh } from "./FreshStart";
import { GuidePanel, TopSpeedQuestion } from "./GuidePanel";
import { StepPanel } from "./StepPanel";

/** Which way a run belongs to a walk: RUN ALL's sweep, the guided walk, or neither. */
type Walk = "sweep" | "guide" | null;

/**
 * GROUND_TRUTH, the guided calibration. The belt loop's length comes first,
 * asked once per treadmill (`snapshot.calibration.belt_cm`). Then one
 * question, the fastest the user walks: Rust plans the guided walk
 * (`snapshot.calibration.guide`) from the slowest speed to that one, with
 * midpoint checks, and each speed waits for RUN. The target list and RUN ALL
 * are the advanced way in. Each row is one speed: RUN (the user's press) sets the belt to it, and the user taps
 * as the belt mark passes (TAP, Space in the dashboard, Alt+Space anywhere).
 * Rust timestamps the taps and ends the run once they agree, with the point
 * it gives and saves it by itself, averaged into the saved point at that speed
 * if there is one. UNDO puts the points back as they were before the run, until
 * the next run starts or the panel closes. ADD RUN times a saved point again.
 * RUN ALL walks every row as a sweep (`snapshot.calibration.sweep`): a saved
 * or skipped speed leaves the belt alone, and the next waits for RUN. The run panel follows
 * `snapshot.calibration.run`, so it is back where it was when the dashboard
 * reopens mid-run. STOP is never disabled. The target list is a Rust setting
 * (`snapshot.calibration.targets`): ADD_SPEED and remove go through Rust.
 * While a run is open its card (status, TAP, counter, STOP) sits at the top,
 * above the walk and the rows, so the 760x600 window shows it without
 * scrolling, and each new phase scrolls it into view. A walk's RUN is the one
 * way on, so the rows' own RUN buttons hide while a walk is open.
 * START FRESH sets the treadmill's points aside before a new walk, and while
 * a set is aside (`snapshot.calibration.aside_count`) RESTORE and KEEP NEW
 * settle it; Rust keeps both sets.
 */
export function GuidedCalibration() {
  const snap = useAppSnapshot();
  const points = useTreadmillStore((s) => s.calibrationPoints);
  const removePoint = useTreadmillStore((s) => s.removeCalibrationPoint);
  // Which RUN was pressed, only for its status line; the open run itself is Rust's.
  const [pressed, setPressed] = useState<number | null>(null);
  const runAction = useAction();
  // Which remove was pressed, for its status line under that row.
  const [removing, setRemoving] = useState<number | null>(null);
  const removeAction = useAction();
  // BELT_LOOP's form is open again after CHANGE.
  const [editingBelt, setEditingBelt] = useState(false);
  // ADVANCED // TARGET_LIST is open (view state only).
  const [advanced, setAdvanced] = useState(false);
  const minKmh = useTreadmillStore((s) => s.minSpeed);
  const maxKmh = useTreadmillStore((s) => s.maxSpeed);
  const cal = snap?.calibration;
  const activeRef = useRef<HTMLDivElement>(null);
  const phaseKey = cal?.run
    ? `${cal.run.target_kmh}:${cal.run.phase}:${cal.run.steps?.phase ?? ""}:${cal.run.saved}`
    : null;
  // Each new phase brings the open run's card into view.
  useEffect(() => {
    if (phaseKey !== null) activeRef.current?.scrollIntoView?.({ block: "start", behavior: "smooth" });
  }, [phaseKey]);
  // The dashboard stays open and on top while a run, sweep or walk is open (#139).
  const active = Boolean(cal?.run || cal?.sweep || cal?.guide);
  useGuidedTask("ground_truth", active);
  // A long walk outlasts the pin's half hour, so each new speed starts its clock again.
  const runKmh = cal?.run?.target_kmh ?? null;
  useEffect(() => {
    if (active && runKmh !== null) send({ type: "guided_task_start", task: "ground_truth" });
  }, [active, runKmh]);
  if (!cal) return null;

  const run = cal.run;
  // The speed the deck ran: a start at the treadmill can differ from the one RUN asked.
  const ranKmh = run ? (run.result?.point.speed ?? run.target_kmh) : null;
  // The run's speed always has a row, even if its point was deleted mid-run.
  const rows = calibrationRows(points, [...cal.targets.map((t) => t.kmh), ...(ranKmh !== null ? [ranKmh] : [])]);
  const runRow = ranKmh !== null ? runRowFor(rows, ranKmh) : null;
  // The deck's own settings for the top speed stepper: DECK_PROFILE's, else the min and max settings.
  const deckSteps = snap.deck.speed_steps.map((st) => st.hw_kmh);
  const topSteps = deckSteps.length > 1 ? deckSteps : settingSteps(minKmh, maxKmh);
  const sweep = cal.sweep;
  const guide = cal.guide;
  const walk: Walk = guide ? "guide" : sweep ? "sweep" : null;
  const beltSet = cal.belt_cm !== null;
  const running = run !== null && run.phase !== "measured";

  const start = (row: CalibrationRow) => {
    setPressed(row.targetKmh);
    void runAction.run(() => dispatch({ type: "calibration_run_start", target_kmh: row.targetKmh }));
  };

  return (
    <div className="space-y-3">
      <div className="font-digital text-xs font-bold uppercase tracking-[0.18em] text-neon-cyan">
        NEON STRIDER // GROUND_TRUTH
      </div>
      {run && (
        <div ref={activeRef} className="hacker-panel scroll-mt-2 rounded-lg border-neon-cyan/30 bg-white/[0.02] p-3">
          <Readout label="DECK_VELOCITY" value={`${(ranKmh ?? run.target_kmh).toFixed(1)} km/h`} className="text-neon-cyan" />
          <RunPanel
            run={run}
            limits={cal.limits}
            rawKmh={snap.deck.raw_speed_kmh}
            walk={walk}
            onClosed={() => {
              setPressed(null);
              runAction.reset();
            }}
          />
        </div>
      )}
      {!run &&
        (!beltSet || editingBelt ? (
          <BeltLoopStep
            limits={cal.limits}
            current={cal.belt_cm}
            treadmillKnown={cal.belt_treadmill}
            onSaved={() => setEditingBelt(false)}
          />
        ) : (
          <div className="flex items-center justify-between gap-3">
            <Readout label="BELT_LOOP" value={`${(cal.belt_cm ?? 0).toFixed(0)} cm`} className="text-white" />
            <GhostButton onClick={() => setEditingBelt(true)}>CHANGE</GhostButton>
          </div>
        ))}
      {beltSet && !run && (
        <Hint>
          Stick a strip of tape across the belt as a mark, press RUN, and tap each time the mark passes.
        </Hint>
      )}
      {beltSet && guide && <GuidePanel guide={guide} running={running} stop={<StopButton />} />}
      {beltSet && sweep && <SweepPanel sweep={sweep} running={running} />}
      {beltSet && !guide && !sweep && !run && <TopSpeedQuestion steps={topSteps} />}
      {cal.aside_count !== null && !run && <PointsAside count={cal.aside_count} newPoints={points.length} />}
      {beltSet && cal.belt_treadmill && cal.aside_count === null && points.length > 0 && !guide && !sweep && !run && (
        <StartFresh />
      )}
      <Label>POINTS</Label>
      <div className="space-y-2">
        {rows.map((row) => {
          const listed = listedTarget(cal.targets, row.targetKmh);
          const outOfRange = listed !== null && !listed.runnable;
          const removable = listed !== null && !row.point && !(run && row === runRow);
          return (
          <div key={row.targetKmh} className="hacker-panel rounded-lg border-white/5 bg-white/[0.02] p-3">
            <div className="flex items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-4">
                <Readout label="DECK_VELOCITY" value={`${row.targetKmh.toFixed(1)} km/h`} className="text-neon-cyan" />
                {row.point ? (
                  <>
                    <Readout label="STRIDE" value={`${row.point.stride_length_cm.toFixed(1)} cm`} className="text-white" />
                    <Readout
                      label="FACTOR"
                      value={`${row.point.distance_scale.toFixed(3)}x`}
                      className="text-neon-magenta"
                    />
                    <Readout label="RUNS" value={String(row.point.runs ?? 1)} className="text-white" />
                  </>
                ) : (
                  <span className="font-mono text-[11px] uppercase tracking-[0.15em] text-white/40">Uncalibrated</span>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-1">
                {walk === null && (
                  <NeonButton
                    onClick={() => start(row)}
                    disabled={runAction.pending || outOfRange || !beltSet}
                    className="px-3 py-1.5"
                    tone={row.point ? "magenta" : "cyan"}
                  >
                    {row.point ? "ADD RUN" : "RUN"}
                  </NeonButton>
                )}
                {removable && (
                  <button
                    type="button"
                    disabled={removeAction.pending}
                    onClick={() => {
                      setRemoving(row.targetKmh);
                      void removeAction.run(() => dispatch({ type: "calibration_target_remove", kmh: row.targetKmh }));
                    }}
                    aria-label={`Remove ${row.targetKmh.toFixed(1)} km/h from the target list`}
                    title="Remove from the target list"
                    className="p-2 text-white/50 transition-colors hover:text-neon-magenta"
                  >
                    <X size={14} />
                  </button>
                )}
                {row.point && row.index !== null && (
                  <button
                    type="button"
                    onClick={() => removePoint(row.index as number)}
                    aria-label={`Delete the ${row.targetKmh.toFixed(1)} km/h point`}
                    title="Delete"
                    className="p-2 text-white/50 transition-colors hover:text-neon-magenta"
                  >
                    <Trash2 size={14} />
                  </button>
                )}
              </div>
            </div>
            {outOfRange && (
              <Hint className="mt-2">
                {row.targetKmh.toFixed(1)} km/h is outside the treadmill's speed range, so RUN ALL skips it.
              </Hint>
            )}
            {pressed === row.targetKmh && (
              <ActionStatus
                pending={runAction.pending && "Setting the belt speed…"}
                error={runAction.error}
                className="mt-2"
              />
            )}
            {removing === row.targetKmh && (
              <ActionStatus pending={removeAction.pending} error={removeAction.error} className="mt-2" />
            )}
          </div>
          );
        })}
      </div>
      <button
        type="button"
        aria-expanded={advanced}
        onClick={() => setAdvanced((v) => !v)}
        className="flex items-center gap-1 font-mono text-[11px] font-bold uppercase tracking-[0.15em] text-white/60 transition-colors hover:text-neon-cyan"
      >
        {advanced ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        ADVANCED // TARGET_LIST
      </button>
      {advanced && (
        <div className="space-y-3 border-l border-white/10 pl-3">
          <Hint>The target list picks the speeds by hand, and RUN ALL runs each one, slowest first.</Hint>
          {beltSet && !walk && !run && (
            <div>
              <NeonButton
                className="w-full py-2"
                disabled={runAction.pending}
                onClick={() => {
                  setPressed(-1);
                  void runAction.run(() => dispatch({ type: "calibration_sweep_start" }));
                }}
              >
                RUN ALL
              </NeonButton>
              {pressed === -1 && (
                <ActionStatus
                  pending={runAction.pending && "Setting the belt speed…"}
                  error={runAction.error}
                  className="mt-1"
                />
              )}
            </div>
          )}
          <AddSpeed />
        </div>
      )}
    </div>
  );
}

/** The old app-wide belt length's default: a stored value other than it was typed in, so it can prefill the form. */
const OLD_DEFAULT_BELT_CM = DEFAULT_APP_CONFIG.belt_length;

/** BELT_LOOP: asked once per treadmill, with a short guide. Rust checks and keeps it. */
function BeltLoopStep({
  limits,
  current,
  treadmillKnown,
  onSaved,
}: {
  limits: CalibrationLimits;
  current: number | null;
  treadmillKnown: boolean;
  onSaved: () => void;
}) {
  const oldBelt = useTreadmillStore((s) => s.beltLength);
  const prefill = current ?? (oldBelt !== OLD_DEFAULT_BELT_CM ? oldBelt : null);
  const [value, setValue] = useState(prefill === null ? "" : String(prefill));
  const save = useAction();
  const submit = () => {
    if (value.trim() === "" || save.pending) return;
    void save.run(() => dispatch({ type: "calibration_belt_set", cm: Number(value) })).then((r) => {
      if (r !== undefined) onSaved();
    });
  };
  return (
    <div className="hacker-panel space-y-2 rounded-lg border-neon-cyan/20 bg-white/[0.02] p-3">
      <Label className="text-neon-yellow">BELT_LOOP</Label>
      <Hint>GROUND_TRUTH times the belt, so it needs the belt loop's length once for each treadmill.</Hint>
      <ol className="list-decimal space-y-1 pl-5 font-mono text-[11px] text-white/70">
        <li>With the treadmill off, stick a strip of tape across the belt as a mark.</li>
        <li>Measure along the top from the mark, pushing the belt round by hand until the mark comes back.</li>
        <li>Enter the whole loop in cm, a little over twice the deck's length.</li>
      </ol>
      <div className="flex flex-wrap items-end gap-2">
        <label className="block">
          <Label>LOOP_LENGTH</Label>
          <input
            type="number"
            min={limits.min_belt_cm}
            max={limits.max_belt_cm}
            step={1}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
            className={cx(INPUT_CLASS, "mt-1 block w-28")}
            aria-label="Belt loop length in cm"
          />
        </label>
        <span className="pb-2 font-mono text-[11px] text-white/50">cm</span>
        <NeonButton className="px-3 py-1.5" disabled={save.pending || value.trim() === ""} onClick={submit}>
          SAVE
        </NeonButton>
      </div>
      <ActionStatus pending={save.pending} error={save.error} />
      {!treadmillKnown && <Hint>Connect the treadmill first, so the belt length is kept for that treadmill.</Hint>}
    </div>
  );
}

/** ADD_SPEED: Rust checks the speed and keeps the list. */
function AddSpeed() {
  const add = useAction();
  const [value, setValue] = useState("");
  const submit = () => {
    if (value.trim() === "" || add.pending) return;
    void add
      .run(() => dispatch({ type: "calibration_target_add", kmh: Number(value) }), { done: "Added to the target list." })
      .then((r) => {
        if (r !== undefined) setValue("");
      });
  };
  return (
    <div>
      <div className="flex flex-wrap items-end gap-2">
        <label className="block">
          <Label>ADD_SPEED</Label>
          <input
            type="number"
            min={0}
            step={0.1}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
            className={cx(INPUT_CLASS, "mt-1 block w-24")}
          />
        </label>
        <GhostButton disabled={add.pending || value.trim() === ""} onClick={submit}>
          <Plus size={14} className="mr-1 inline" />
          ADD
        </GhostButton>
      </div>
      <Hint className="mt-1">ADD_SPEED takes deck km/h in 0.1 steps.</Hint>
      <ActionStatus pending={add.pending} error={add.error} done={add.done} className="mt-1" />
    </div>
  );
}

function Readout({ label, value, className }: { label: string; value: string; className: string }) {
  return (
    <div className="space-y-0.5">
      <div className="font-mono text-[11px] uppercase text-white/50">{label}</div>
      <div className={cx("font-digital text-sm", className)}>{value}</div>
    </div>
  );
}

/** Space in a field types a space; anywhere else in the window it is a tap. */
function typingInto(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/** One run: SPIN_UP, TAPPING until Rust's taps agree, then MEASURED and the point it gives. */
function RunPanel({
  run,
  limits,
  rawKmh,
  walk,
  onClosed,
}: {
  run: CalibrationRunView;
  limits: CalibrationLimits;
  rawKmh: number;
  walk: Walk;
  onClosed: () => void;
}) {
  const inSweep = walk !== null;
  const tap = useAction();
  const again = useAction();
  const undo = useAction();
  const listening = run.phase !== "measured";
  // GAIT_SYNC's footsteps take the same keys once the belt is measured.
  const keysListening = listening || (run.steps !== null && run.steps.phase !== "measured");
  const sendTap = tap.run;

  // Space taps while the dashboard has focus; Alt+Space reaches Rust's input
  // hook from any window, so the two never count one press twice.
  useEffect(() => {
    if (!keysListening) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== " " || e.altKey || e.ctrlKey || e.metaKey || typingInto(e.target)) return;
      // Also keeps a focused button from treating Space as its own click.
      e.preventDefault();
      if (e.repeat) return;
      void sendTap(() => dispatch({ type: "calibration_tap" }));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keysListening, sendTap]);

  // In a sweep or the walk, closing a run that has no saved point skips its speed.
  const close = () => {
    const command =
      walk === "sweep"
        ? ({ type: "calibration_sweep_next", saved: false } as const)
        : walk === "guide"
          ? ({ type: "calibration_guide_skip" } as const)
          : ({ type: "calibration_run_close" } as const);
    void dispatch(command).finally(onClosed);
  };

  const phaseLabel = { settling: "SPIN_UP", tapping: "TAPPING", measured: "MEASURED" }[run.phase];
  const r = run.result;
  const dropped = droppedLine(run.dropped);
  const progress = Math.min(1, run.kept / limits.min_agreeing);

  return (
    <div className="mt-3 space-y-3 border-t border-white/10 pt-3">
      <div className="flex items-end justify-between gap-3">
        <Label className="text-neon-yellow">{phaseLabel}</Label>
        <div className="flex items-start gap-4">
          <Readout label="DECK_NOW" value={`${rawKmh.toFixed(1)} km/h`} className="text-neon-cyan" />
          <Readout
            label="TRUE_VELOCITY"
            value={run.measured_kmh === null ? "--" : `${run.measured_kmh.toFixed(2)} km/h`}
            className="text-neon-magenta"
          />
          <Readout label="PASSES" value={String(run.passes)} className="text-white" />
          <Readout
            label="CHRONO"
            value={chrono(run.elapsed_s)}
            className={run.phase === "tapping" ? "text-neon-yellow" : "text-white"}
          />
          <StopButton />
        </div>
      </div>

      {listening && (
        <div className="space-y-2">
          <Hint>
            {run.phase === "settling"
              ? `When the belt is steady at ${run.target_kmh.toFixed(1)} km/h, tap each time the mark passes.`
              : "Keep tapping each pass of the mark; the run ends by itself once the times agree."}
          </Hint>
          <button
            type="button"
            onPointerDown={(e) => {
              // The press, not the release, is the moment the mark passed.
              if (e.button !== 0) return;
              void tap.run(() => dispatch({ type: "calibration_tap" }));
            }}
            className="w-full select-none rounded-lg border-2 border-neon-cyan/70 bg-neon-cyan/10 py-6 font-digital text-2xl font-bold tracking-[0.3em] text-neon-cyan transition-colors active:bg-neon-cyan/30"
          >
            TAP
          </button>
          <ActionStatus pending={false} error={tap.error} />
          <Hint>Space taps in the dashboard, and Alt+Space taps from any window.</Hint>
          <div
            className="h-1.5 overflow-hidden rounded bg-white/10"
            role="progressbar"
            aria-label="Agreeing passes"
            aria-valuemin={0}
            aria-valuemax={limits.min_agreeing}
            aria-valuenow={Math.min(run.kept, limits.min_agreeing)}
          >
            <div className="h-full bg-neon-cyan transition-all" style={{ width: `${progress * 100}%` }} />
          </div>
          <div className="font-mono text-[11px] text-white/60">
            {run.kept} of {limits.min_agreeing} agreeing times between passes.
          </div>
          {run.reported_kmh !== null && run.measured_kmh !== null && (
            <div className="font-mono text-[11px] text-white/60">
              The deck shows {run.reported_kmh.toFixed(1)} km/h; the belt measures {run.measured_kmh.toFixed(2)} km/h.
            </div>
          )}
          {dropped && <Hint>{dropped}</Hint>}
          {run.note && <div className="font-mono text-[11px] text-neon-yellow">{run.note}</div>}
          <div className="flex justify-end">
            <GhostButton onClick={close}>{inSweep ? "SKIP" : "Close"}</GhostButton>
          </div>
        </div>
      )}

      {r && (
        <div className="space-y-3">
          <div className="font-digital text-sm text-white">
            {deckVerdict(r.point.speed, r.real_kmh, r.reported_kmh, run.saved ? "saved" : "undone")}
          </div>
          <div className="flex flex-wrap gap-4 rounded border border-white/5 bg-white/5 p-2">
            <Readout label="TRUE_VELOCITY" value={`${r.real_kmh.toFixed(2)} km/h`} className="text-neon-cyan" />
            <Readout label="DECK_VELOCITY" value={`${r.reported_kmh.toFixed(2)} km/h`} className="text-white" />
            <Readout label="FACTOR" value={`${r.point.distance_scale.toFixed(3)}x`} className="text-neon-magenta" />
            <Readout label="PASSES" value={String(run.passes)} className="text-white" />
          </div>
          {dropped && <Hint>{dropped}</Hint>}
          <div className="flex gap-2">
            {run.saved && (
              <NeonButton
                tone="magenta"
                className="flex-1 py-2"
                disabled={undo.pending}
                onClick={() => void undo.run(() => dispatch({ type: "calibration_undo" }))}
              >
                UNDO
              </NeonButton>
            )}
            <GhostButton
              disabled={again.pending}
              onClick={() => void again.run(() => dispatch({ type: "calibration_tap_reset" }))}
            >
              TAP AGAIN
            </GhostButton>
            {/* In a sweep or the walk, the panel above has RUN and SKIP for the speed. */}
            {!inSweep && <GhostButton onClick={close}>Close</GhostButton>}
          </div>
          <ActionStatus pending={undo.pending} error={undo.error} />
          <ActionStatus pending={again.pending} error={again.error} />
          <StepPanel run={run} limits={limits} />
        </div>
      )}
    </div>
  );
}

/** STOP: the belt stops, as with the HUD's STOP. Never disabled. */
function StopButton() {
  const stop = useAction();
  return (
    <div className="flex flex-col items-end">
      <button
        type="button"
        onClick={() => void stop.run(() => dispatch({ type: "stop" }))}
        className="rounded border border-red-500/70 bg-red-500/10 px-3 py-1.5 font-digital text-xs font-bold tracking-[0.15em] text-red-400 transition-colors hover:bg-red-500/25"
      >
        STOP
      </button>
      <ActionStatus pending={stop.pending && "Stopping the belt…"} error={stop.error} className="mt-1" />
    </div>
  );
}

const MARK_CLASS: Record<SweepMark | "current", string> = {
  saved: "border-emerald-400/60 text-emerald-400",
  skipped: "border-white/10 text-white/40 line-through",
  pending: "border-white/15 text-white/50",
  current: "border-neon-yellow/70 text-neon-yellow",
};

/** RUN ALL's progress: one chip per speed, then RUN on the next speed or the tally. */
function SweepPanel({ sweep, running }: { sweep: CalibrationSweepView; running: boolean }) {
  const next = useAction();
  const end = useAction();
  const done = sweep.current === null;
  const savedCount = sweep.marks.filter((m) => m === "saved").length;
  const total = sweep.targets_kmh.length;
  const position = sweep.current === null ? total : sweep.current + 1;
  const nextKmh = sweep.current === null ? null : sweep.targets_kmh[sweep.current];
  return (
    <div className="hacker-panel space-y-3 rounded-lg border-neon-yellow/20 bg-white/[0.02] p-3">
      <div className="flex items-center justify-between gap-3">
        <Label className="text-neon-yellow">
          SWEEP {position}/{total}
        </Label>
        <GhostButton
          disabled={end.pending}
          onClick={() => void end.run(() => dispatch({ type: "calibration_sweep_end" }))}
        >
          {done ? "Close" : "END SWEEP"}
        </GhostButton>
      </div>
      <ol className="flex flex-wrap gap-1.5" aria-label="Sweep speeds">
        {sweep.targets_kmh.map((kmh, i) => {
          const mark = sweep.marks[i];
          const key = i === sweep.current ? "current" : mark;
          return (
            <li
              key={kmh}
              className={cx("flex items-center gap-1 rounded border px-2 py-1 font-digital text-xs", MARK_CLASS[key])}
            >
              {mark === "saved" && <Check size={12} aria-hidden />}
              {kmh.toFixed(1)}
              <span className="sr-only">{key}</span>
            </li>
          );
        })}
      </ol>
      <ActionStatus pending={false} error={end.error} />
      {done && (
        <Hint>
          Sweep complete: {savedCount} of {total} speeds saved.
        </Hint>
      )}
      {nextKmh !== null && !running && (
        <div>
          <Hint>RUN sets the belt to the next speed, {nextKmh.toFixed(1)} km/h.</Hint>
          <div className="mt-2 flex items-start gap-2">
            <NeonButton
              className="flex-1 py-2"
              disabled={next.pending}
              onClick={() => void next.run(() => dispatch({ type: "calibration_sweep_run" }))}
            >
              RUN {nextKmh.toFixed(1)} km/h
            </NeonButton>
            <GhostButton
              disabled={next.pending}
              onClick={() => void next.run(() => dispatch({ type: "calibration_sweep_next", saved: false }))}
            >
              SKIP
            </GhostButton>
            <StopButton />
          </div>
          <ActionStatus pending={next.pending && "Setting the belt speed…"} error={next.error} className="mt-1" />
        </div>
      )}
    </div>
  );
}
