import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { Minus, Plus } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
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
      className="flex h-10 w-9 shrink-0 items-center justify-center rounded border @max-[14rem]:h-9 @max-[14rem]:w-8 border-white/10 text-white/60 transition-colors hover:border-white/30 hover:text-white disabled:opacity-30"
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
  status = null,
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
  /** Sends the value; a rejection shows under the control. */
  onSet: (v: number) => Promise<unknown> | void;
  onNudge: (up: boolean) => Promise<unknown> | void;
  /** Moves a chosen value to one the deck runs, so the slider lands where the belt will. */
  snap?: (v: number) => number;
  /** What the deck said about the target this control sent (DECK_ADJUSTED / DECK_REFUSED). */
  note?: string | null;
  /** A labelled mark on the slider (STRIDE_FIT on speed). */
  mark?: SliderMark | null;
  /** A short reading beside the pin in the title row (the STASIS clock on Velocity); the row keeps its height. */
  status?: ReactNode;
}) {
  const [value, setDraft] = useDraft(target);
  // The last press, drag or scroll: its refusal shows under the presets.
  const sent = useAction();
  const nudge = (up: boolean) => void sent.run(async () => onNudge(up));
  const wheel = useWheelSteps<HTMLDivElement>(nudge, disabled);
  const hex = tone === "cyan" ? "#00f3ff" : "#ff00ff";
  const set = (v: number) => {
    const snapped = snap(v);
    setDraft(snapped);
    void sent.run(async () => onSet(snapped));
  };

  // A preset the deck skips is left out.
  const shown = presets.filter((p) => p >= min - 1e-6 && p <= max + 1e-6 && Math.abs(snap(p) - p) < 1e-6);
  // In a narrow panel (two side by side at a large Windows text size) six presets take two rows of three.
  const cols = { "--cols": shown.length, "--cols-narrow": shown.length > 4 ? Math.ceil(shown.length / 2) : shown.length } as CSSProperties;

  return (
    // The panel is a container so its own width decides the narrow layout: two panels side by side
    // at 150 % Windows text get under 14rem each, where the padding, readout and nudges shrink.
    <Panel className="@container">
      <div className="flex flex-col gap-2 p-3 @max-[14rem]:gap-1.5 @max-[14rem]:p-2">
        <div className="flex items-center justify-between gap-x-2">
          <Label>{title}</Label>
          <div className="flex min-w-0 items-center gap-x-2">
            {status}
            <PinToggle widget={pin.widget} pinned={pin.pinned} />
          </div>
        </div>

        <div className="flex items-center gap-2 @max-[14rem]:gap-1">
          <Nudge label={`${title} down`} disabled={disabled} onClick={() => nudge(false)} icon={<Minus size={16} />} />
          {/* Disabled means no treadmill is linked: the reading is the last one it sent, so it
              recedes as the HUD's readings do. */}
          <div
            ref={wheel}
            title={wheelHint}
            className={cx("flex min-w-0 flex-1 items-end justify-center gap-2 @max-[14rem]:gap-1", disabled && "opacity-45")}
          >
            <SegmentedDisplay
              value={actual}
              length={2}
              color={hex}
              className={actual >= 9.95 ? "h-9 max-w-full @max-[14rem]:h-7" : "h-12 max-w-full @max-[14rem]:h-9"}
            />
            <span className="pb-1 font-mono text-xs text-white/60">{unit}</span>
          </div>
          <Nudge label={`${title} up`} disabled={disabled} onClick={() => nudge(true)} icon={<Plus size={16} />} />
        </div>

        {/* The slider's own caption: it sets the target, which the big reading only follows. */}
        <div className={cx("flex flex-col gap-1", disabled && "opacity-30")}>
          <div className="flex flex-wrap items-baseline justify-between gap-x-2 font-mono text-[11px] uppercase leading-none tracking-[0.15em] text-white/60">
            <span>Set target</span>
            <span>
              <span className={cx("font-digital text-xs leading-none", TONE_TEXT[tone])}>{value.toFixed(1)}</span>{" "}
              <span className="normal-case">{unit}</span>
            </span>
          </div>
          <StylizedSlider
            min={min}
            max={max}
            step={step}
            value={value}
            onChange={set}
            color={tone}
            mark={mark}
            label={`${title} target`}
            disabled={disabled}
          />
        </div>

        <div
          style={cols}
          className="grid grid-cols-[repeat(var(--cols),minmax(0,1fr))] gap-1 @max-[14rem]:grid-cols-[repeat(var(--cols-narrow),minmax(0,1fr))]"
        >
          {shown.map((p) => {
            // The preset the target is on shows as pressed: filled, with a bar along its foot.
            const current = Math.abs(value - p) < step / 2 + 1e-6;
            return (
              <button
                key={p}
                type="button"
                disabled={disabled}
                aria-pressed={current}
                onClick={() => set(p)}
                className={cx(
                  "min-h-8 min-w-0 rounded border py-1 font-mono text-xs font-bold transition-colors disabled:opacity-30",
                  current
                    ? tone === "cyan"
                      ? "border-neon-cyan bg-neon-cyan/20 text-neon-cyan shadow-[inset_0_-3px_0_var(--color-neon-cyan)]"
                      : "border-neon-magenta bg-neon-magenta/20 text-neon-magenta shadow-[inset_0_-3px_0_var(--color-neon-magenta)]"
                    : "border-white/10 text-white/60 hover:border-white/25 hover:text-white",
                )}
              >
                {formatPreset(p)}
              </button>
            );
          })}
        </div>
        <ActionStatus error={sent.error ?? note} />
      </div>
    </Panel>
  );
}
