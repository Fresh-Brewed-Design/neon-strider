import { dispatch, type CalibrationLimits, type CalibrationRunView, type StepRunView } from "../../../ipc";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { GhostButton, Hint, Label, NeonButton } from "../../ui";
import { chrono } from "./guided";

/** The line under the step counter about taps dropped against the median, if any. */
function droppedSteps(dropped: number): string | null {
  if (dropped <= 0) return null;
  return dropped === 1 ? "1 footstep dropped as a miss or a double." : `${dropped} footsteps dropped as misses or doubles.`;
}

/**
 * GAIT_SYNC, the footstep phase after a belt run is measured (Rust:
 * `calibration/steps.rs`). TAP STEPS starts it; TAP, Space and Alt+Space then
 * time footsteps instead of the belt mark, and Rust ends it by itself once the
 * steps agree. Rust saves the stride by itself, the same way as the belt's
 * point, with UNDO; SKIP leaves the stride as it was. Nothing here moves the belt.
 */
export function StepPanel({ run, limits }: { run: CalibrationRunView; limits: CalibrationLimits }) {
  const steps = run.steps;
  const start = useAction();
  // The speed the deck ran, which a start at the treadmill can set apart from RUN's.
  const ranKmh = run.result?.point.speed ?? run.target_kmh;
  const kmh = ranKmh.toFixed(1);
  // The stride joins the point the belt run saved; once UNDO took that back, TAP AGAIN on the belt comes first.
  if (steps === null && !run.saved) return null;
  return (
    <div className="space-y-2 rounded border border-neon-cyan/20 bg-white/[0.02] p-2">
      <Label className="text-neon-yellow">GAIT_SYNC</Label>
      {steps === null ? (
        <>
          <Hint>Tap each footstep, left and right, to measure your stride at {kmh} km/h.</Hint>
          <NeonButton
            className="w-full py-2"
            disabled={start.pending}
            onClick={() => void start.run(() => dispatch({ type: "calibration_steps_start" }))}
          >
            TAP STEPS
          </NeonButton>
          <ActionStatus pending={start.pending} error={start.error} />
        </>
      ) : steps.result === null ? (
        <StepTapping steps={steps} limits={limits} />
      ) : (
        <StepResult steps={steps} kmh={ranKmh} />
      )}
    </div>
  );
}

function Readout({ label, value }: { label: string; value: string }) {
  return (
    <div className="space-y-0.5">
      <div className="font-mono text-[11px] uppercase text-white/50">{label}</div>
      <div className="font-digital text-sm text-white">{value}</div>
    </div>
  );
}

function StepTapping({ steps, limits }: { steps: StepRunView; limits: CalibrationLimits }) {
  const tap = useAction();
  const skip = useAction();
  const progress = Math.min(1, steps.kept / limits.min_steps);
  const dropped = droppedSteps(steps.dropped);
  return (
    <div className="space-y-2">
      <Hint>Tap each footstep, left and right; GAIT_SYNC ends by itself once the steps agree.</Hint>
      <div className="flex flex-wrap gap-4">
        <Readout label="STEPS" value={String(steps.steps)} />
        <Readout label="CADENCE" value={steps.cadence_spm === null ? "--" : `${steps.cadence_spm.toFixed(0)} /min`} />
        <Readout label="STRIDE" value={steps.stride_cm === null ? "--" : `${steps.stride_cm.toFixed(1)} cm`} />
        <Readout label="CHRONO" value={chrono(steps.elapsed_s)} />
      </div>
      <button
        type="button"
        onPointerDown={(e) => {
          // The press, not the release, is the moment the foot landed.
          if (e.button !== 0) return;
          void tap.run(() => dispatch({ type: "calibration_tap" }));
        }}
        className="w-full select-none rounded-lg border-2 border-neon-cyan/70 bg-neon-cyan/10 py-6 font-digital text-2xl font-bold tracking-[0.3em] text-neon-cyan transition-colors active:bg-neon-cyan/30"
      >
        TAP
      </button>
      <ActionStatus pending={false} error={tap.error} />
      <div
        className="h-1.5 overflow-hidden rounded bg-white/10"
        role="progressbar"
        aria-label="Agreeing footsteps"
        aria-valuemin={0}
        aria-valuemax={limits.min_steps}
        aria-valuenow={Math.min(steps.kept, limits.min_steps)}
      >
        <div className="h-full bg-neon-cyan transition-all" style={{ width: `${progress * 100}%` }} />
      </div>
      <div className="font-mono text-[11px] text-white/60">
        {steps.kept} of {limits.min_steps} agreeing times between footsteps.
      </div>
      {dropped && <Hint>{dropped}</Hint>}
      {steps.note && <div className="font-mono text-[11px] text-neon-yellow">{steps.note}</div>}
      <div className="flex items-center justify-end gap-2">
        <Hint>SKIP keeps the stride as it was.</Hint>
        <GhostButton
          disabled={skip.pending}
          onClick={() => void skip.run(() => dispatch({ type: "calibration_steps_skip" }))}
        >
          SKIP
        </GhostButton>
      </div>
      <ActionStatus pending={skip.pending} error={skip.error} />
    </div>
  );
}

/** The measured stride, saved by Rust, with UNDO and TAP AGAIN. */
function StepResult({ steps, kmh }: { steps: StepRunView; kmh: number }) {
  const undo = useAction();
  const again = useAction();
  const r = steps.result;
  if (!r) return null;
  const speed = kmh.toFixed(1);
  return (
    <div className="space-y-2">
      <div className="font-digital text-sm text-white">
        {steps.undone
          ? `UNDO restored the ${speed} km/h point to the belt run's result.`
          : `Stride ${r.point.stride_length_cm.toFixed(1)} cm at ${(r.cadence_spm ?? 0).toFixed(0)} steps a minute for ${speed} km/h; saved.`}
      </div>
      <div className="flex gap-2">
        {steps.saved && (
          <NeonButton
            tone="magenta"
            className="flex-1 py-2"
            disabled={undo.pending}
            onClick={() => void undo.run(() => dispatch({ type: "calibration_steps_undo" }))}
          >
            UNDO
          </NeonButton>
        )}
        <GhostButton
          disabled={again.pending}
          onClick={() => void again.run(() => dispatch({ type: "calibration_steps_start" }))}
        >
          TAP AGAIN
        </GhostButton>
      </div>
      <ActionStatus pending={undo.pending} error={undo.error} />
      <ActionStatus pending={again.pending} error={again.error} />
    </div>
  );
}
