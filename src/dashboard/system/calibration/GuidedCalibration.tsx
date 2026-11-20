import { useState } from "react";
import { Check, Trash2 } from "lucide-react";
import {
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
import { GhostButton, Hint, Label, NeonButton } from "../../ui";
import { cx } from "../../util";
import { INPUT_CLASS, NumberField } from "../fields";
import { calibrationRows, chrono, runRowFor, type CalibrationRow } from "./guided";

/**
 * Labs: the guided calibration. One row per speed; RUN (his press) sets the
 * belt to it, COUNT and DONE time a count of belt loops in Rust, and Rust
 * works out the point from what he counted. REPEAT re-runs one saved point.
 * RUN ALL walks every row as a sweep (`snapshot.calibration.sweep`): saving
 * or skipping one speed leaves the belt alone, and the next waits for RUN.
 * The run panel follows `snapshot.calibration.run`, so it is back where it was
 * when the dashboard reopens mid-run. STOP is never disabled.
 */
export function GuidedCalibration() {
  const snap = useAppSnapshot();
  const points = useTreadmillStore((s) => s.calibrationPoints);
  const beltLength = useTreadmillStore((s) => s.beltLength);
  const updateSettings = useTreadmillStore((s) => s.updateSettings);
  const removePoint = useTreadmillStore((s) => s.removeCalibrationPoint);
  // Which RUN was pressed, only for its status line; the open run itself is Rust's.
  const [pressed, setPressed] = useState<number | null>(null);
  const runAction = useAction();
  const cal = snap?.calibration;
  if (!cal) return null;

  const run = cal.run;
  // The run's speed always has a row, even if its point was deleted mid-run.
  const rows = calibrationRows(points, [...cal.limits.default_targets_kmh, ...(run ? [run.target_kmh] : [])]);
  const runRow = run ? runRowFor(rows, run.target_kmh) : null;
  const sweep = cal.sweep;

  const start = (row: CalibrationRow) => {
    setPressed(row.targetKmh);
    void runAction.run(() => dispatch({ type: "calibration_run_start", target_kmh: row.targetKmh }));
  };

  return (
    <div className="space-y-3">
      <div className="font-digital text-xs font-bold uppercase tracking-[0.18em] text-neon-cyan">
        NEON STRIDER // GROUND_TRUTH
      </div>
      <Hint>
        Mark the belt, press RUN, then press COUNT as the mark passes once the belt is steady. Count loops for at least a
        minute, and steps to measure your stride.
      </Hint>
      <NumberField
        label="BELT_LOOP"
        hint="The whole belt's length, once around."
        value={beltLength}
        min={cal.limits.min_belt_cm}
        max={cal.limits.max_belt_cm}
        unit="cm"
        onCommit={(v) => updateSettings({ beltLength: v })}
      />
      {sweep ? (
        <SweepPanel sweep={sweep} running={run !== null} />
      ) : (
        !run && (
          <div>
            <NeonButton
              className="w-full py-2"
              disabled={runAction.pending}
              onClick={() => {
                setPressed(-1);
                void runAction.run(() =>
                  dispatch({ type: "calibration_sweep_start", targets_kmh: rows.map((r) => r.targetKmh) }),
                );
              }}
            >
              RUN ALL
            </NeonButton>
            <Hint className="mt-1">RUN ALL runs every speed below in turn, slowest first.</Hint>
            {pressed === -1 && (
              <ActionStatus
                pending={runAction.pending && "Setting the belt speed…"}
                error={runAction.error}
                className="mt-1"
              />
            )}
          </div>
        )
      )}
      <div className="space-y-2">
        {rows.map((row) => (
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
                  </>
                ) : (
                  <span className="font-mono text-[11px] uppercase tracking-[0.15em] text-white/40">Uncalibrated</span>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-1">
                <NeonButton
                  onClick={() => start(row)}
                  disabled={runAction.pending || sweep !== null}
                  className="px-3 py-1.5"
                  tone={row.point ? "magenta" : "cyan"}
                >
                  {row.point ? "REPEAT" : "RUN"}
                </NeonButton>
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
            {pressed === row.targetKmh && (
              <ActionStatus
                pending={runAction.pending && "Setting the belt speed…"}
                error={runAction.error}
                className="mt-2"
              />
            )}
            {run && row === runRow && (
              <RunPanel
                run={run}
                row={row}
                limits={cal.limits}
                rawKmh={snap.deck.raw_speed_kmh}
                beltLength={beltLength}
                inSweep={sweep !== null}
                onClosed={() => {
                  setPressed(null);
                  runAction.reset();
                }}
              />
            )}
          </div>
        ))}
      </div>
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

/** One run: SPIN_UP, COUNT_WINDOW, then TALLY and the point it gives. */
function RunPanel({
  run,
  row,
  limits,
  rawKmh,
  beltLength,
  inSweep,
  onClosed,
}: {
  run: CalibrationRunView;
  row: CalibrationRow;
  limits: CalibrationLimits;
  rawKmh: number;
  beltLength: number;
  inSweep: boolean;
  onClosed: () => void;
}) {
  const step = useAction();
  const tally = useAction();
  const [loops, setLoops] = useState("");
  const [steps, setSteps] = useState("");
  const addPoint = useTreadmillStore((s) => s.addCalibrationPoint);
  const updatePoint = useTreadmillStore((s) => s.updateCalibrationPoint);

  // In a sweep, closing a run marks its speed saved or skipped.
  const close = (saved = false) => {
    const command = inSweep
      ? ({ type: "calibration_sweep_next", saved } as const)
      : ({ type: "calibration_run_close" } as const);
    void dispatch(command).finally(onClosed);
  };
  const save = () => {
    const point = run.result?.point;
    if (!point) return;
    if (row.index !== null) updatePoint(row.index, point);
    else addPoint(point);
    close(true);
  };
  const compute = () => {
    const stepCount = steps.trim() === "" ? null : Math.round(Number(steps));
    void tally.run(() =>
      dispatch({
        type: "calibration_run_count",
        count: { method: "loops", loops: Number(loops), belt_length_cm: beltLength, steps: stepCount },
      }),
    );
  };

  const phaseLabel = { settling: "SPIN_UP", counting: "COUNT_WINDOW", counted: "TALLY" }[run.phase];
  const r = run.result;

  return (
    <div className="mt-3 space-y-3 border-t border-white/10 pt-3">
      <div className="flex items-end justify-between gap-3">
        <Label className="text-neon-yellow">{phaseLabel}</Label>
        <div className="flex items-start gap-4">
          <Readout label="DECK_NOW" value={`${rawKmh.toFixed(1)} km/h`} className="text-neon-cyan" />
          <Readout
            label="CHRONO"
            value={chrono(run.elapsed_s)}
            className={run.phase === "counting" ? "text-neon-yellow" : "text-white"}
          />
          <StopButton />
        </div>
      </div>

      {run.phase === "settling" && (
        <div>
          <Hint>When the belt is steady at {run.target_kmh.toFixed(1)} km/h, press COUNT as the mark passes.</Hint>
          <div className="mt-2 flex gap-2">
            <NeonButton
              className="flex-1 py-2"
              disabled={step.pending}
              onClick={() => void step.run(() => dispatch({ type: "calibration_count_start" }))}
            >
              COUNT
            </NeonButton>
            <GhostButton onClick={() => close()}>{inSweep ? "SKIP" : "Close"}</GhostButton>
          </div>
          <ActionStatus pending={step.pending} error={step.error} className="mt-1" />
        </div>
      )}

      {run.phase === "counting" && (
        <div>
          <Hint>
            Count each pass of the mark. Press DONE as it passes on your last loop, after at least{" "}
            {limits.min_run_s.toFixed(0)} s.
          </Hint>
          <NeonButton
            tone="magenta"
            className="mt-2 w-full py-3"
            disabled={step.pending}
            onClick={() => void step.run(() => dispatch({ type: "calibration_count_stop" }))}
          >
            DONE
          </NeonButton>
          <ActionStatus pending={step.pending} error={step.error} className="mt-1" />
        </div>
      )}

      {run.phase === "counted" && (
        <div className="space-y-3">
          <Hint>
            Timed at {(run.reported_kmh ?? 0).toFixed(2)} km/h on the deck. Enter the loops counted; steps are optional
            (at least {limits.min_steps}).
          </Hint>
          <div className="flex flex-wrap items-end gap-3">
            <label className="block">
              <Label>BELT_LOOPS</Label>
              <input
                type="number"
                min={0}
                step={0.5}
                value={loops}
                onChange={(e) => setLoops(e.target.value)}
                className={cx(INPUT_CLASS, "mt-1 block w-24")}
              />
            </label>
            <label className="block">
              <Label>STEP_COUNT</Label>
              <input
                type="number"
                min={0}
                step={1}
                value={steps}
                onChange={(e) => setSteps(e.target.value)}
                className={cx(INPUT_CLASS, "mt-1 block w-24")}
              />
            </label>
            <NeonButton className="px-3 py-1.5" disabled={tally.pending || loops.trim() === ""} onClick={compute}>
              COMPUTE
            </NeonButton>
          </div>
          <ActionStatus pending={tally.pending} error={tally.error} />
          {r && (
            <div className="space-y-2">
              <div className="flex flex-wrap gap-4 rounded border border-white/5 bg-white/5 p-2">
                <Readout label="FACTOR" value={`${r.point.distance_scale.toFixed(3)}x`} className="text-neon-magenta" />
                <Readout label="STRIDE" value={`${r.point.stride_length_cm.toFixed(1)} cm`} className="text-white" />
                <Readout label="TRUE_VELOCITY" value={`${r.real_kmh.toFixed(2)} km/h`} className="text-neon-cyan" />
                <Readout label="DISPLACEMENT" value={`${r.real_m.toFixed(1)} m`} className="text-white" />
                {r.cadence_spm !== null && (
                  <Readout label="CADENCE" value={`${r.cadence_spm.toFixed(0)} /min`} className="text-white" />
                )}
              </div>
              {!r.stride_measured && <Hint>No steps counted, so the calibrated stride is kept.</Hint>}
            </div>
          )}
          <div className="flex gap-2">
            <NeonButton tone="green" className="flex-1 py-2" disabled={!r} onClick={save}>
              {row.index !== null ? "REPLACE POINT" : "SAVE POINT"}
            </NeonButton>
            <GhostButton onClick={() => close()}>{inSweep ? "SKIP" : "Discard"}</GhostButton>
          </div>
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
