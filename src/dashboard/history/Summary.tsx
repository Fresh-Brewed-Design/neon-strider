import type { ReactNode } from "react";
import type { Change, WeeklySummary, WeekStats } from "../../ipc";
import { Joined, Label, Panel } from "../ui";
import { cx } from "../util";
import { formatDuration } from "../../format";
import { formatChange, type ChangeTone } from "./weekly";

const TONE_CLASS: Record<ChangeTone, string> = {
  up: "text-emerald-400",
  down: "text-neon-yellow",
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
      <Joined
        className="font-mono text-xs text-white/60"
        text={`${week.active_days} / 7 days · ${formatDuration(week.duration_s)} · ${week.sessions} ${week.sessions === 1 ? "session" : "sessions"}`}
      />
    </div>
  );
}

/** This week and last week, from Rust's weekly summary. */
export function WeeklyPanel({ weekly }: { weekly: WeeklySummary }) {
  return (
    // Side by side while each week gets 10rem; a large Windows text size stacks them.
    <Panel className="grid grid-cols-[repeat(auto-fit,minmax(10rem,1fr))] gap-3 p-3">
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

/** Rust's totals for a time frame or day. */
export interface Totals {
  meters: number;
  seconds: number;
  kcal: number;
  sessions: number;
}

/**
 * Totals of every saved walk in the time frame or day, summed by Rust, under the name of that range.
 * `picker` (the time frame tabs) sits in its heading row, on the block it changes; until Rust's
 * totals arrive (`null`) the values read `--` and the tabs stay where they are.
 */
export function TotalsPanel({
  totals,
  heading,
  picker,
}: {
  /** The range the totals cover, as the page names it ("Last 7 days"). */
  heading: string;
  picker?: ReactNode;
  totals: Totals | null;
}) {
  const t = totals;
  return (
    // One row of four while each total gets 6rem (its label and a value like 12h 30m); a large
    // Windows text size wraps them onto more rows.
    <Panel className="grid grid-cols-[repeat(auto-fit,minmax(6rem,1fr))] gap-3 p-3">
      <div className="col-span-full flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
        <Label className="text-neon-cyan">{heading}</Label>
        {picker}
      </div>
      <SummaryStat label="Distance" value={t ? (t.meters / 1000).toFixed(1) : "--"} unit="km" />
      <SummaryStat label="Time" value={t ? formatDuration(t.seconds) : "--"} />
      <SummaryStat label="Sessions" value={t ? String(t.sessions) : "--"} />
      <SummaryStat label="Energy" value={t ? String(Math.round(t.kcal)) : "--"} unit="kcal" />
    </Panel>
  );
}
