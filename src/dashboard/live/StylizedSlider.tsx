import { sliderValue } from "./sliderValue";

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
}: {
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (val: number) => void;
  color: "cyan" | "magenta";
}) {
  const isCyan = color === "cyan";
  // The 16 px thumb travels the track minus its own width so it never pokes past either end.
  const pct = max > min ? Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100)) : 0;

  return (
    <div className="flex flex-col gap-2 w-full group">
      <div className="relative h-6 flex items-center">
        <div className="absolute inset-0 h-1 bg-white/5 top-1/2 -translate-y-1/2 rounded-full overflow-hidden">
          <div
            className={`h-full ${isCyan ? "bg-neon-cyan" : "bg-neon-magenta"}`}
            style={{ width: `${((value - min) / (max - min)) * 100}%` }}
          />
        </div>

        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(sliderValue(e.target.value, min, max))}
          className="absolute inset-0 w-full h-full opacity-0 cursor-pointer z-10"
        />

        <div
          className="absolute h-4 w-4 rounded-sm border-2 pointer-events-none z-0"
          style={{
            left: `calc(${pct}% - ${pct * 0.16}px)`,
            borderColor: isCyan ? "var(--color-neon-cyan)" : "var(--color-neon-magenta)",
            boxShadow: isCyan ? "0 0 10px rgba(0,243,255,0.3)" : "0 0 10px rgba(255,0,255,0.3)",
            backgroundColor: "black",
          }}
        />
      </div>

      <div className="flex justify-between px-1">
        {[0, 1, 2, 3, 4].map((i) => (
          <div key={i} className="h-1 w-[1px] bg-white/10" />
        ))}
      </div>
    </div>
  );
}
