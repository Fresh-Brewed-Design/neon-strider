import { memo } from "react";

// Lit segments a to g, clockwise from the top, then the middle bar.
const SEGMENTS: Record<string, boolean[]> = {
  "0": [true, true, true, true, true, true, false],
  "1": [false, true, true, false, false, false, false],
  "2": [true, true, false, true, true, false, true],
  "3": [true, true, true, true, false, false, true],
  "4": [false, true, true, false, false, true, true],
  "5": [true, false, true, true, false, true, true],
  "6": [true, false, true, true, true, true, true],
  "7": [true, true, true, false, false, false, false],
  "8": [true, true, true, true, true, true, true],
  "9": [true, true, true, true, false, true, true],
  "-": [false, false, false, false, false, false, true],
};
const BLANK = [false, false, false, false, false, false, false];

const PATHS = [
  "M10 5 L40 5 L45 10 L40 15 L10 15 L5 10 Z",
  "M42 10 L47 15 L47 38 L42 43 L37 38 L37 15 Z",
  "M42 45 L47 50 L47 73 L42 78 L37 73 L37 50 Z",
  "M10 73 L40 73 L45 78 L40 83 L10 83 L5 78 Z",
  "M8 45 L13 50 L13 73 L8 78 L3 73 L3 50 Z",
  "M8 10 L13 15 L13 38 L8 43 L3 38 L3 15 Z",
  "M10 39 L40 39 L45 44 L40 49 L10 49 L5 44 Z",
];

const INACTIVE = "rgba(255, 255, 255, 0.02)";

const SegmentedDigit = memo(function SegmentedDigit({ value, color }: { value: string; color: string }) {
  const segments = SEGMENTS[value] ?? BLANK;
  return (
    <svg
      viewBox="0 0 50 90"
      className="w-full h-full"
      style={{ filter: segments.some(Boolean) ? `drop-shadow(0 0 4px ${color})` : "none", pointerEvents: "none" }}
    >
      <g strokeWidth="0" fillRule="evenodd">
        {PATHS.map((d, i) => (
          <path key={i} d={d} fill={segments[i] ? color : INACTIVE} />
        ))}
      </g>
    </svg>
  );
});

/** A number drawn as seven-segment digits, right-aligned to `length` digits before the point. */
export const SegmentedDisplay = memo(function SegmentedDisplay({
  value,
  length = 2,
  color = "#00f3ff",
  className = "",
  precision = 1,
}: {
  value: number;
  length?: number;
  color?: string;
  className?: string;
  precision?: number;
}) {
  const chars = value.toFixed(precision).padStart(length + (precision > 0 ? 1 : 0), " ").split("");

  return (
    <div className={`flex items-center justify-center gap-2 ${className} overflow-visible`}>
      {chars.map((char, i) =>
        char === "." ? (
          <div key={i} className="flex flex-col justify-end pb-4 shrink-0">
            <div className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: color, boxShadow: `0 0 10px ${color}` }} />
          </div>
        ) : (
          <div key={i} className="h-full aspect-[5/8] shrink-0">
            <SegmentedDigit value={char} color={color} />
          </div>
        ),
      )}
    </div>
  );
});
