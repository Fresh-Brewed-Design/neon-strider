import { useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { dispatch, type CalibrationGuideView, type GuideMark } from "../../../ipc";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { GhostButton, Hint, Label, NeonButton } from "../../ui";
import { cx } from "../../util";
import { checkLine, guideSummary } from "./guided";

/**
 * The one question the guided walk asks: the fastest setting the user walks
 * at, as the treadmill's display shows it (the deck's own number, no unit).
 * `steps` are the settings the deck runs, slowest first; the first is the
 * walk's slowest speed, so the top starts at the second. The stepper starts
 * at the deck's top. START only plans the speeds in Rust
 * (`calibration_guide_start`); the belt waits for RUN.
 */
export function TopSpeedQuestion({ steps }: { steps: number[] }) {
  const choices = steps.slice(1);
  const [index, setIndex] = useState(choices.length - 1);
  const at = Math.min(Math.max(index, 0), choices.length - 1);
  const value: number | undefined = choices[at];
  const start = useAction();
  const submit = () => {
    if (value === undefined || start.pending) return;
    void start.run(() => dispatch({ type: "calibration_guide_start", top_kmh: value }));
  };
  return (
    <div className="hacker-panel space-y-2 rounded-lg border-neon-cyan/20 bg-white/[0.02] p-3">
      <Label className="text-neon-yellow">TOP_SPEED</Label>
      <div className="font-digital text-sm text-white">Fastest setting you'll walk at, as the treadmill shows it.</div>
      <Hint>GROUND_TRUTH times the slowest speed and your top speed, then checks the speed halfway between.</Hint>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          className={STEP_CLASS}
          disabled={at <= 0}
          onClick={() => setIndex(at - 1)}
          aria-label="Lower top speed setting"
        >
          -
        </button>
        <output className="w-14 text-center font-digital text-lg text-neon-cyan" aria-label="Top speed setting">
          {value === undefined ? "--" : value.toFixed(1)}
        </output>
        <button
          type="button"
          className={STEP_CLASS}
          disabled={at >= choices.length - 1}
          onClick={() => setIndex(at + 1)}
          aria-label="Raise top speed setting"
        >
          +
        </button>
        <NeonButton className="px-3 py-1.5" disabled={start.pending || value === undefined} onClick={submit}>
          START
        </NeonButton>
      </div>
      <ActionStatus pending={start.pending} error={start.error} />
    </div>
  );
}

const STEP_CLASS =
  "rounded border border-white/15 px-3 py-1.5 font-digital text-sm text-white transition-colors hover:border-neon-cyan/60 disabled:opacity-30";

const MARK_CLASS: Record<GuideMark | "next", string> = {
  saved: "border-emerald-400/60 text-emerald-400",
  skipped: "border-white/10 text-white/40 line-through",
  pending: "border-white/15 text-white/50",
  next: "border-neon-yellow/70 text-neon-yellow",
};

/**
 * The guided walk (`snapshot.calibration.guide`): one chip per speed, the next
 * speed's RUN and SKIP, each midpoint check in plain words, and the summary
 * once every speed is done. RUN is the user's press for each speed; nothing
 * else here moves the belt. STOP is always on screen while a speed waits.
 */
export function GuidePanel({
  guide,
  running,
  stop,
}: {
  guide: CalibrationGuideView;
  running: boolean;
  stop: ReactNode;
}) {
  const next = useAction();
  const end = useAction();
  const done = guide.next_kmh === null;
  const nextIsCheck = guide.checks.some((c) => c.verdict === "pending" && c.mid_kmh === guide.next_kmh);
  return (
    <div className="hacker-panel space-y-3 rounded-lg border-neon-yellow/20 bg-white/[0.02] p-3">
      <div className="flex items-center justify-between gap-3">
        <Label className="text-neon-yellow">
          WALK {guide.slow_kmh.toFixed(1)} TO {guide.top_kmh.toFixed(1)} KM/H
        </Label>
        <GhostButton
          disabled={end.pending}
          onClick={() => void end.run(() => dispatch({ type: "calibration_guide_end" }))}
        >
          {done ? "Close" : "END WALK"}
        </GhostButton>
      </div>
      <ol className="flex flex-wrap gap-1.5" aria-label="Walk speeds">
        {guide.speeds.map((s) => {
          const key = s.kmh === guide.next_kmh ? "next" : s.mark;
          return (
            <li
              key={s.kmh}
              className={cx("flex items-center gap-1 rounded border px-2 py-1 font-digital text-xs", MARK_CLASS[key])}
            >
              {s.mark === "saved" && <Check size={12} aria-hidden />}
              {s.kmh.toFixed(1)}
              <span className="sr-only">{key}</span>
            </li>
          );
        })}
      </ol>
      <ActionStatus pending={false} error={end.error} />
      {guide.note && <div className="font-mono text-[11px] text-neon-yellow">{guide.note}</div>}
      {guide.checks.length > 0 && (
        <ul className="space-y-1 font-mono text-[11px] text-white/70" aria-label="Midpoint checks">
          {guide.checks.map((c) => (
            <li key={`${c.lo_kmh}-${c.hi_kmh}`}>{checkLine(c, guide.tolerance_pct)}</li>
          ))}
        </ul>
      )}
      {done && (
        <div className="space-y-1 rounded border border-emerald-400/30 bg-emerald-400/5 p-2">
          {guideSummary(guide).map((line) => (
            <div key={line} className="font-mono text-[11px] text-white">
              {line}
            </div>
          ))}
        </div>
      )}
      {guide.next_kmh !== null && !running && (
        <div>
          <Hint>
            {nextIsCheck
              ? "A check run measures the distance error halfway between two calibrated speeds."
              : `RUN sets the belt to the next speed, ${guide.next_kmh.toFixed(1)} km/h.`}
          </Hint>
          <div className="mt-2 flex items-start gap-2">
            <NeonButton
              className="flex-1 py-2"
              disabled={next.pending}
              onClick={() => void next.run(() => dispatch({ type: "calibration_guide_run" }))}
            >
              RUN {guide.next_kmh.toFixed(1)} km/h
            </NeonButton>
            <GhostButton
              disabled={next.pending}
              onClick={() => void next.run(() => dispatch({ type: "calibration_guide_skip" }))}
            >
              SKIP
            </GhostButton>
            {stop}
          </div>
          <ActionStatus pending={next.pending && "Setting the belt speed…"} error={next.error} className="mt-1" />
        </div>
      )}
    </div>
  );
}
