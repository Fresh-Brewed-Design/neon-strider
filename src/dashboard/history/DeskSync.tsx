import type { DeskSync } from "../../ipc";
import { Label, Panel } from "../ui";
import { cx } from "../util";
import { deskSyncText } from "./deskSyncText";

function dayTitle(date: string, walked: boolean): string {
  const label = new Date(`${date}T00:00:00`).toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  });
  return `${label}: ${walked ? "walked" : "at the desk, no walk"}`;
}

/** Walked days among the recent days the PC was used, from Rust's desk-day count. */
export function DeskSyncPanel({ sync }: { sync: DeskSync }) {
  return (
    <Panel className="flex items-center gap-2 p-3">
      <div className="shrink-0">
        <Label>DESK_SYNC</Label>
        <div className="flex items-baseline gap-1">
          <span className="font-digital text-xl text-neon-cyan">{sync.walked}</span>
          <span className="font-mono text-xs text-white/60">/ {sync.desk_days}</span>
        </div>
      </div>
      <p
        className="min-w-0 flex-1 font-mono text-xs leading-snug text-white/60"
        title="A desk day is a day you used the PC. Days away from the PC don't count."
      >
        {deskSyncText(sync)}
      </p>
      {(sync.days.length > 0 || sync.today_open) && (
        <ul aria-label="Recent desk days" className="flex shrink-0 items-center gap-0.5">
          {sync.days.map((d) => (
            <li
              key={d.date}
              title={dayTitle(d.date, d.walked)}
              aria-label={dayTitle(d.date, d.walked)}
              className={cx(
                "h-3 w-2.5 rounded-sm",
                d.walked ? "bg-neon-cyan shadow-[0_0_6px_rgba(0,243,255,0.6)]" : "bg-white/15",
              )}
            />
          ))}
          {sync.today_open && (
            <li
              title="Today: not walked yet"
              aria-label="Today: not walked yet"
              className="h-3 w-2.5 rounded-sm border border-dashed border-neon-cyan/60"
            />
          )}
        </ul>
      )}
    </Panel>
  );
}
