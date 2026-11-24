import type { Change, WeeklySummary, WeekStats } from "../../ipc";
import { Label, Panel } from "../ui";
import { cx } from "../util";
import { formatDuration } from "../../format";
import { formatChange, type ChangeTone } from "./weekly";

const TONE_CLASS: Record<ChangeTone, string> = {
  up: "text-emerald-400",
  down: "text-neon-magenta",
  flat: "text-white/60",
  new: "text-neon-cyan",
};

function WeekCell({ title, week, change }: { title: string; week: WeekStats; change?: Change }) {
  const chip = change ? formatChange(change) : null;
  return (
    <div className="min-w-0">
      <Label>{title}</Label>
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span className="font-digital text-xl text-neon-cyan">{week.distance_km.toFixed(1)}</span>
        <span className="font-mono text-xs text-white/60">km</span>
        {chip && <span className={cx("font-mono text-xs font-bold", TONE_CLASS[chip.tone])}>{chip.text}</span>}
      </div>
      <div className="font-mono text-xs text-white/60">
        {week.active_days} / 7 days · {formatDuration(week.duration_s)} · {week.sessions}{" "}
        {week.sessions === 1 ? "session" : "sessions"}
      </div>
    </div>
  );
}

/** This week and last week, from Rust's weekly summary. */
export function WeeklyPanel({ weekly }: { weekly: WeeklySummary }) {
  return (
    <Panel className="grid grid-cols-2 gap-3 p-3">
      <WeekCell title="This week" week={weekly.this_week} />
      <div className="border-l border-white/10 pl-3">
        <WeekCell title="Last week" week={weekly.last_week} change={weekly.distance_change} />
      </div>
    </Panel>
  );
}

function SummaryStat({ label, value, unit }: { label: string; value: string; unit?: string }) {
  return (
    <div className="min-w-0">
      <Label>{label}</Label>
      <div className="flex items-baseline gap-1">
        <span className="font-digital text-xl text-neon-cyan">{value}</span>
        {unit && <span className="font-mono text-xs text-white/60">{unit}</span>}
      </div>
    </div>
  );
}

/** Totals of the sessions currently listed. */
export function TotalsPanel({
  meters,
  seconds,
  kcal,
  count,
}: {
  meters: number;
  seconds: number;
  kcal: number;
  count: number;
}) {
  return (
    <Panel className="grid grid-cols-4 gap-3 p-3">
      <SummaryStat label="Distance" value={(meters / 1000).toFixed(1)} unit="km" />
      <SummaryStat label="Time" value={formatDuration(seconds)} />
      <SummaryStat label="Sessions" value={String(count)} />
      <SummaryStat label="Energy" value={String(Math.round(kcal))} unit="kcal" />
    </Panel>
  );
}
