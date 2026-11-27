import { useMemo, useRef, useState, type KeyboardEvent } from "react";
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

  // One tab stop for the whole grid: the cell that was last focused (today at first) takes it.
  const lastIndex = useMemo(() => days.reduce((last, d, i) => (d.future ? last : i), 0), [days]);
  const [active, setActive] = useState<number | null>(null);
  const cells = useRef<(HTMLButtonElement | null)[]>([]);
  const stop = Math.min(active ?? lastIndex, lastIndex);

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    // Columns are weeks and rows are days, so left and right move a week and up and down a day.
    const step: Record<string, number> = { ArrowUp: -1, ArrowDown: 1, ArrowLeft: -7, ArrowRight: 7 };
    let to: number | null = null;
    if (e.key in step) to = i + step[e.key];
    else if (e.key === "Home") to = 0;
    else if (e.key === "End") to = lastIndex;
    if (to === null) return;
    e.preventDefault();
    to = Math.min(lastIndex, Math.max(0, to));
    setActive(to);
    cells.current[to]?.focus();
  };

  return (
    <div>
      <div className="mb-1 grid gap-x-px" style={{ gridTemplateColumns: `repeat(${WEEKS}, minmax(0, 1fr))` }}>
        {months.map((m) => (
          <span
            key={m.col}
            className="whitespace-nowrap font-mono text-xs leading-none text-white/60"
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
        {days.map(({ key, date, future }, i) => {
          const meters = metersByDay.get(key) ?? 0;
          const isSelected = key === selected;
          const empty = future || meters === 0;
          return (
            <button
              key={key}
              ref={(el) => {
                cells.current[i] = el;
              }}
              type="button"
              tabIndex={i === stop ? 0 : -1}
              aria-disabled={empty || undefined}
              aria-pressed={isSelected}
              onFocus={() => setActive(i)}
              onKeyDown={(e) => onKeyDown(e, i)}
              aria-label={`${date.toLocaleDateString()}: ${km(meters)} km`}
              title={`${date.toLocaleDateString()}: ${km(meters)} km`}
              onClick={() => {
                if (!empty) onSelect(isSelected ? null : key);
              }}
              className={cx(
                "aspect-square rounded-[2px] focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:opacity-100! focus-visible:outline-white",
                future ? "bg-transparent" : meters > 0 ? "bg-neon-cyan" : "bg-white/[0.06]",
                key === today && "outline outline-1 outline-white/70",
                isSelected && "outline outline-2 outline-offset-1 outline-neon-cyan",
                !empty && "cursor-pointer hover:brightness-150",
              )}
              style={meters > 0 ? { opacity: opacityFor(meters) } : undefined}
            />
          );
        })}
      </div>
      <div className="mt-1.5 flex items-center justify-end gap-1.5 font-mono text-xs text-white/60">
        <span>Less</span>
        {[0.2, 0.35, 0.6, 1].map((o) => (
          <span key={o} className="h-2.5 w-2.5 rounded-[2px] bg-neon-cyan" style={{ opacity: o }} />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}
