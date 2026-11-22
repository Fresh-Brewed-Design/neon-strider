import type { WallClock } from "../../ipc";
import { Label, Panel } from "../ui";
import { hourLabel, wallClockText } from "./wallClockText";

/** An hour label under every third cell, from the first: enough to read the row without crowding it. */
const LABEL_EVERY = 3;

function cellTitle(hour: number, count: number, days: number): string {
  if (count === 0) return `${hourLabel(hour)}: no walks`;
  return `${hourLabel(hour)}: walked on ${count} of ${days} desk ${days === 1 ? "day" : "days"}`;
}

/** Which hours of the desk day are usually walked, from Rust's hourly counts. */
export function WallClockPanel({ clock }: { clock: WallClock }) {
  const hours: number[] = [];
  for (let h = clock.first_hour; h <= clock.last_hour; h++) hours.push(h);
  const columns = { gridTemplateColumns: `repeat(${hours.length}, minmax(0, 1fr))` };
  const days = clock.desk_days;
  return (
    <Panel className="flex flex-col gap-2 p-3">
      <div className="flex items-baseline gap-3">
        <Label>WALL_CLOCK</Label>
        <p className="min-w-0 flex-1 font-mono text-[11px] leading-snug text-white/60">{wallClockText(clock)}</p>
      </div>
      {days > 0 && (
        <div>
          <ul aria-label="Walked hours" className="grid gap-1" style={columns}>
            {hours.map((h) => {
              const count = clock.hours[h] ?? 0;
              const share = count / days;
              const title = cellTitle(h, count, days);
              return (
                <li
                  key={h}
                  title={title}
                  aria-label={title}
                  className={count > 0 ? "h-3.5 rounded-sm bg-neon-cyan" : "h-3.5 rounded-sm bg-white/10"}
                  style={
                    count > 0
                      ? {
                          // A day's walk always shows; every day walked is full strength.
                          opacity: 0.25 + 0.75 * share,
                          boxShadow: share >= 0.5 ? "0 0 6px rgba(0,243,255,0.6)" : undefined,
                        }
                      : undefined
                  }
                />
              );
            })}
          </ul>
          <div aria-hidden className="mt-1 grid gap-1" style={columns}>
            {hours.map((h, i) => (
              <span key={h} className="whitespace-nowrap font-mono text-[11px] leading-none text-white/40">
                {i % LABEL_EVERY === 0 ? String(h).padStart(2, "0") : ""}
              </span>
            ))}
          </div>
        </div>
      )}
    </Panel>
  );
}
