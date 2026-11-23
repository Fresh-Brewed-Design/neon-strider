import type { TimelineMark, TodayTimeline } from "../../ipc";
import { cx } from "../util";
import { Label, Panel } from "../ui";
import { hourTicks, MARK_WORDS, runTitle, todayText } from "./todayText";

const MARK_CLASS: Record<TimelineMark, string> = {
  walked: "bg-neon-cyan shadow-[0_0_6px_rgba(0,243,255,0.6)]",
  desk: "bg-white/35",
  away: "bg-white/5",
};

/** Legend order: the two marks that mean PC use first. */
const LEGEND: TimelineMark[] = ["walked", "desk", "away"];

/** Where a minute sits along the strip, in percent. */
function at(t: TodayTimeline, minute: number): number {
  const span = Math.max(1, t.now_min - t.start_min);
  return ((minute - t.start_min) / span) * 100;
}

/** Today from the first PC input to now: Rust's runs of away, at the PC and walking. */
export function TodayPanel({ timeline }: { timeline: TodayTimeline }) {
  const ticks = hourTicks(timeline);
  return (
    <Panel className="flex flex-col gap-2 p-3">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <Label>Today</Label>
        <p className="min-w-0 flex-1 font-mono text-[11px] leading-snug text-white/60">{todayText(timeline)}</p>
        <ul aria-label="Legend" className="flex flex-wrap gap-x-3 gap-y-1">
          {LEGEND.map((m) => (
            <li key={m} className="flex items-center gap-1.5 font-mono text-[11px] text-white/50">
              <span aria-hidden className={cx("h-2.5 w-2.5 rounded-sm", MARK_CLASS[m])} />
              {MARK_WORDS[m]}
            </li>
          ))}
        </ul>
      </div>
      <div>
        <ul aria-label="Today's timeline" className="relative h-3.5 overflow-hidden rounded-sm bg-white/5">
          {timeline.runs.map((run) => {
            const title = runTitle(run);
            const left = at(timeline, run.start_min);
            return (
              <li
                key={run.start_min}
                title={title}
                aria-label={title}
                className={cx("absolute inset-y-0", MARK_CLASS[run.mark])}
                style={{ left: `${left}%`, width: `${at(timeline, run.end_min) - left}%` }}
              />
            );
          })}
        </ul>
        <div aria-hidden className="relative mt-1 h-[1.1em] font-mono text-[11px] leading-none text-white/40">
          {ticks.map((h) => {
            const pct = at(timeline, h * 60);
            // Labels at the ends stay inside the panel.
            const shift = pct < 6 ? "0%" : pct > 94 ? "-100%" : "-50%";
            return (
              <span
                key={h}
                className="absolute top-0 whitespace-nowrap"
                style={{ left: `${pct}%`, transform: `translateX(${shift})` }}
              >
                {String(h).padStart(2, "0")}
              </span>
            );
          })}
        </div>
      </div>
    </Panel>
  );
}
