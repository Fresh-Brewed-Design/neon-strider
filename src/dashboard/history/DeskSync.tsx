import { useEffect, useState } from "react";
import { dispatch, type DeskSync } from "../../ipc";
import { PLAIN } from "../../names";
import { Label, Panel, PlainLine } from "../ui";
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

function reducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Today's light pulsing on: once when Rust says today just counted
 * (`today_lit`), never under reduced motion. Either way the card tells Rust
 * it was shown, so no later page open plays it again today.
 */
function useTodayLight(lit: boolean): [boolean, () => void] {
  const [pulse, setPulse] = useState(false);
  const [seenLit, setSeenLit] = useState(false);
  if (lit !== seenLit) {
    setSeenLit(lit);
    if (lit && !reducedMotion()) setPulse(true);
  }
  useEffect(() => {
    if (!lit) return;
    dispatch({ type: "desk_sync_light_shown" }).catch((e) => console.warn("[desk-sync] light not marked shown", e));
  }, [lit]);
  return [pulse, () => setPulse(false)];
}

/** Walked days among the recent days the PC was used, from Rust's desk-day count. */
export function DeskSyncPanel({ sync }: { sync: DeskSync }) {
  const [pulse, endPulse] = useTodayLight(sync.today_lit);
  const lastIndex = sync.days.length - 1;
  return (
    <Panel className="flex flex-wrap items-center gap-2 p-3">
      <div className="shrink-0">
        <Label>DESK_SYNC</Label>
        <PlainLine text={PLAIN.DESK_SYNC} />
        <div className="flex items-baseline gap-1">
          <span className="font-digital text-xl text-neon-cyan">{sync.walked}</span>
          <span className="font-mono text-xs text-white/60">/ {sync.desk_days}</span>
        </div>
      </div>
      {/* The sentence keeps 10rem beside the count; with less room (a large Windows text size)
          the day marks wrap under it. */}
      <p
        className="min-w-0 flex-1 basis-40 font-mono text-xs leading-snug text-white/60"
        title="A desk day is a day you used the PC. Days away from the PC don't count."
      >
        {deskSyncText(sync)}
      </p>
      {(sync.days.length > 0 || sync.today_open) && (
        <ul aria-label="Recent desk days" className="flex shrink-0 items-center gap-0.5">
          {sync.days.map((d, i) => {
            // Today is the last day once it counts.
            const lighting = pulse && i === lastIndex && d.walked;
            return (
              <li
                key={d.date}
                title={dayTitle(d.date, d.walked)}
                aria-label={dayTitle(d.date, d.walked)}
                data-lighting={lighting || undefined}
                onAnimationEnd={lighting ? endPulse : undefined}
                className={cx(
                  "h-3 w-2.5 rounded-sm",
                  d.walked ? "bg-neon-cyan shadow-[0_0_6px_rgba(0,243,255,0.6)]" : "bg-white/15",
                  lighting && "desk-sync-light",
                )}
              />
            );
          })}
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
