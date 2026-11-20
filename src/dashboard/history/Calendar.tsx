import { useMemo } from "react";
import { cx } from "../util";
import { dayKey, km } from "../../format";

/** A year of weeks: 52 plus the one in progress. */
const WEEKS = 53;

function opacityFor(meters: number): number {
  if (meters > 5000) return 1;
  if (meters > 2500) return 0.6;
  if (meters > 500) return 0.35;
  return 0.2;
}

/**
 * A year of days, one column per week, shaded by distance walked.
 * Clicking a day narrows the session list to it; clicking it again clears.
 */
export function Calendar({
  metersByDay,
  selected,
  onSelect,
}: {
  metersByDay: Map<string, number>;
  selected: string | null;
  onSelect: (day: string | null) => void;
}) {
  const { days, months, today } = useMemo(() => {
    const now = new Date();
    const end = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    end.setDate(end.getDate() + (6 - end.getDay())); // Saturday of this week
    const start = new Date(end);
    start.setDate(start.getDate() - (WEEKS * 7 - 1));

    const cells: { key: string; date: Date; future: boolean }[] = [];
    const labels: { name: string; col: number }[] = [];
    const monthName = (d: Date) => d.toLocaleDateString([], { month: "short" });
    for (let i = 0; i < WEEKS * 7; i++) {
      const d = new Date(start);
      d.setDate(start.getDate() + i);
      cells.push({ key: dayKey(d.getTime()), date: d, future: d.getTime() > now.getTime() });
      if (d.getDate() === 1) labels.push({ name: monthName(d), col: Math.floor(i / 7) });
    }
    // Name the opening month too, unless the next label would crowd it.
    if (labels.length === 0 || labels[0].col >= 4) labels.unshift({ name: monthName(start), col: 0 });
    return { days: cells, months: labels, today: dayKey(now.getTime()) };
  }, []);

  return (
    <div>
      <div className="mb-1 grid gap-x-px" style={{ gridTemplateColumns: `repeat(${WEEKS}, minmax(0, 1fr))` }}>
        {months.map((m) => (
          <span
            key={m.col}
            className="whitespace-nowrap font-mono text-[11px] leading-none text-white/45"
            style={{ gridColumnStart: m.col + 1 }}
          >
            {m.name}
          </span>
        ))}
      </div>
      <div
        className="grid grid-flow-col grid-rows-7 gap-px"
        style={{ gridTemplateColumns: `repeat(${WEEKS}, minmax(0, 1fr))` }}
        role="group"
        aria-label="Distance by day, past year"
      >
        {days.map(({ key, date, future }) => {
          const meters = metersByDay.get(key) ?? 0;
          const isSelected = key === selected;
          return (
            <button
              key={key}
              type="button"
              disabled={future || meters === 0}
              aria-pressed={isSelected}
              aria-label={`${date.toLocaleDateString()}: ${km(meters)} km`}
              title={`${date.toLocaleDateString()}: ${km(meters)} km`}
              onClick={() => onSelect(isSelected ? null : key)}
              className={cx(
                "aspect-square rounded-[2px]",
                future ? "bg-transparent" : meters > 0 ? "bg-neon-cyan" : "bg-white/[0.06]",
                key === today && "outline outline-1 outline-white/70",
                isSelected && "outline outline-2 outline-neon-magenta",
                meters > 0 && !future && "cursor-pointer hover:brightness-150",
              )}
              style={meters > 0 ? { opacity: opacityFor(meters) } : undefined}
            />
          );
        })}
      </div>
      <div className="mt-1.5 flex items-center justify-end gap-1.5 font-mono text-[11px] text-white/45">
        <span>Less</span>
        {[0.2, 0.35, 0.6, 1].map((o) => (
          <span key={o} className="h-2.5 w-2.5 rounded-[2px] bg-neon-cyan" style={{ opacity: o }} />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}
