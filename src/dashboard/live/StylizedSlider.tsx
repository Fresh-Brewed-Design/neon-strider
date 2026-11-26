import { sliderValue, thumbCentre, trackPct, type SliderMark } from "./sliderValue";

/**
 * A range input drawn as a neon track and square thumb. The real input sits on
 * top, invisible, so keyboard and pointer behave like any range input.
 */
export function StylizedSlider({
  min,
  max,
  step,
  value,
  onChange,
  color,
  mark = null,
  label,
  disabled = false,
}: {
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (val: number) => void;
  color: "cyan" | "magenta";
  mark?: SliderMark | null;
  /** The control's accessible name. */
  label: string;
  /** Offline: the input is really disabled, so the keyboard can't move it either. */
  disabled?: boolean;
}) {
  const isCyan = color === "cyan";
  // The 16 px thumb travels the track minus its own width so it never pokes past either end.
  const pct = trackPct(value, min, max);
  // A mark outside the range has no place on the track (a fit above a top lowered since).
  const markPct = mark && mark.value >= min - 1e-6 && mark.value <= max + 1e-6 ? trackPct(mark.value, min, max) : null;

  return (
    // With a mark, its 11 px label row sits under the track, 1 px below it.
    <div className="flex w-full flex-col gap-px group">
      <div className="relative h-6 flex items-center">
        <div className="absolute inset-0 h-1 bg-white/5 top-1/2 -translate-y-1/2 rounded-full overflow-hidden">
          <div
            className={`h-full ${isCyan ? "bg-neon-cyan" : "bg-neon-magenta"}`}
            style={{ width: `${((value - min) / (max - min)) * 100}%` }}
          />
        </div>

        {markPct !== null && (
          <div
            data-testid="slider-mark"
            className="absolute top-1/2 h-4 w-[2px] -translate-x-1/2 -translate-y-1/2 bg-neon-yellow/80 pointer-events-none"
            style={{ left: thumbCentre(markPct) }}
          />
        )}

        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          disabled={disabled}
          aria-label={label}
          onChange={(e) => onChange(sliderValue(e.target.value, min, max))}
          className="peer absolute inset-0 w-full h-full opacity-0 cursor-pointer disabled:cursor-not-allowed z-10"
        />

        <div
          className="absolute h-4 w-4 rounded-sm border-2 pointer-events-none z-0 peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-white"
          style={{
            left: `calc(${pct}% - ${pct * 0.16}px)`,
            borderColor: isCyan ? "var(--color-neon-cyan)" : "var(--color-neon-magenta)",
            boxShadow: isCyan ? "0 0 10px rgba(0,243,255,0.3)" : "0 0 10px rgba(255,0,255,0.3)",
            backgroundColor: "black",
          }}
        />
      </div>

      {markPct !== null && mark && (
        // Near either end the mark's label lines up with that end instead of
        // centring, so it never pokes past the track.
        <div className="relative h-[11px]">
          <span
            title={mark.title}
            className="absolute top-0 whitespace-nowrap font-mono text-[11px] leading-none text-neon-yellow"
            style={{
              left: thumbCentre(markPct),
              transform: `translateX(${markPct < 15 ? "-8px" : markPct > 85 ? "calc(-100% + 8px)" : "-50%"})`,
            }}
          >
            {mark.label}
          </span>
        </div>
      )}
    </div>
  );
}
