import { sparkGeometry, type SparkData } from "./spark";

const WIDTH = 100;
const HEIGHT = 24;

/**
 * A thin line of recent values in the current text colour, with no axes. It
 * stretches to the box `className` gives it; the stroke keeps its width.
 */
export function Sparkline({ data, className }: { data: SparkData; className?: string }) {
  const g = sparkGeometry(data, WIDTH, HEIGHT);
  if (!g) return null;
  return (
    <svg
      className={className}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      preserveAspectRatio="none"
      aria-hidden="true"
      data-testid="sparkline"
    >
      {g.band && <rect data-testid="spark-band" x={0} y={g.band.y} width={WIDTH} height={g.band.height} fill="currentColor" opacity={0.16} />}
      {g.line && (
        <path
          d={g.line}
          fill="none"
          stroke="currentColor"
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeLinejoin="round"
          vectorEffect="non-scaling-stroke"
        />
      )}
    </svg>
  );
}
