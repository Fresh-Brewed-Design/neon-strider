import { useEffect, useRef, useState, type ReactNode } from "react";
import { Minus, Plus } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import type { HudWidget } from "../../ipc";
import { Label, Panel } from "../ui";
import { cx, TONE_TEXT } from "../util";
import { useWheelSteps } from "../../hud/useWheelSteps";
import { PinToggle } from "./PinToggle";
import { SegmentedDisplay } from "./SegmentedDisplay";
import type { SliderMark } from "./sliderValue";
import { StylizedSlider } from "./StylizedSlider";

/**
 * While the user drags a slider, show their value rather than the engine's
 * target, which lags by a snapshot tick and would make the thumb jump back.
 */
function useDraft(live: number) {
  const [draft, setDraft] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const set = (v: number) => {
    setDraft(v);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setDraft(null), 1500); // a few snapshots after the last change, so Rust's value has arrived
  };
  return [draft ?? live, set] as const;
}

function Nudge({ label, icon, disabled, onClick }: { label: string; icon: ReactNode; disabled: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className="flex h-10 w-9 shrink-0 items-center justify-center rounded border border-white/10 text-white/60 transition-colors hover:border-white/30 hover:text-white disabled:opacity-30"
    >
      {icon}
    </button>
  );
}

/** One controlled value (speed or tilt): the live reading, nudge buttons, a slider and presets. */
export function Axis({
  title,
  tone,
  actual,
  unit,
  target,
  min,
  max,
  step,
  presets,
  formatPreset,
  disabled,
  onSet,
  onNudge,
  snap = (v) => v,
  note,
  pin,
  mark = null,
  wheelHint,
}: {
  pin: { widget: HudWidget; pinned: boolean };
  /** Tooltip on the readout, where the mouse wheel nudges the value. */
  wheelHint?: string;
  title: string;
  tone: "cyan" | "magenta";
  actual: number;
  unit: string;
  target: number;
  min: number;
  max: number;
  step: number;
  presets: number[];
  formatPreset: (v: number) => string;
  disabled: boolean;
  onSet: (v: number) => void;
  onNudge: (up: boolean) => void;
  /** Moves a chosen value to one the deck runs, so the slider lands where the belt will. */
  snap?: (v: number) => number;
  /** What the deck said about the target this control sent (DECK_ADJUSTED / DECK_REFUSED). */
  note?: string | null;
  /** A labelled mark on the slider (STRIDE_FIT on speed). */
  mark?: SliderMark | null;
}) {
  const [value, setDraft] = useDraft(target);
  const wheel = useWheelSteps<HTMLDivElement>(onNudge, disabled);
  const hex = tone === "cyan" ? "#00f3ff" : "#ff00ff";
  const set = (v: number) => {
    const snapped = snap(v);
    setDraft(snapped);
    onSet(snapped);
  };

  return (
    <Panel className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center justify-between gap-x-2">
        <Label>{title}</Label>
        <span className="flex flex-wrap items-center gap-1 font-mono text-xs uppercase tracking-[0.15em] text-white/60">
          target <span className={cx("font-digital text-xs", TONE_TEXT[tone])}>{value.toFixed(1)}</span> {unit}
          <PinToggle widget={pin.widget} pinned={pin.pinned} />
        </span>
      </div>

      <div className="flex items-center gap-2">
        <Nudge label={`${title} down`} disabled={disabled} onClick={() => onNudge(false)} icon={<Minus size={16} />} />
        <div ref={wheel} title={wheelHint} className="flex min-w-0 flex-1 items-end justify-center gap-2">
          <SegmentedDisplay
            value={actual}
            length={2}
            color={hex}
            className={actual >= 9.95 ? "h-9 max-w-full" : "h-12 max-w-full"}
          />
          <span className="pb-1 font-mono text-xs text-white/60">{unit}</span>
        </div>
        <Nudge label={`${title} up`} disabled={disabled} onClick={() => onNudge(true)} icon={<Plus size={16} />} />
      </div>

      <div className={cx(disabled && "pointer-events-none opacity-30")}>
        <StylizedSlider min={min} max={max} step={step} value={value} onChange={set} color={tone} mark={mark} />
      </div>

      <div className="flex flex-wrap gap-1">
        {presets
          // A preset the deck skips is left out.
          .filter((p) => p >= min - 1e-6 && p <= max + 1e-6 && Math.abs(snap(p) - p) < 1e-6)
          .map((p) => (
            <button
              key={p}
              type="button"
              disabled={disabled}
              onClick={() => set(p)}
              className={cx(
                "flex-1 min-h-8 rounded border py-1 font-mono text-xs font-bold transition-colors disabled:opacity-30",
                Math.abs(value - p) < step / 2 + 1e-6
                  ? tone === "cyan"
                    ? "border-neon-cyan/60 bg-neon-cyan/15 text-neon-cyan"
                    : "border-neon-magenta/60 bg-neon-magenta/15 text-neon-magenta"
                  : "border-white/10 text-white/60 hover:border-white/25 hover:text-white",
              )}
            >
              {formatPreset(p)}
            </button>
          ))}
      </div>
      <ActionStatus error={note} />
    </Panel>
  );
}
