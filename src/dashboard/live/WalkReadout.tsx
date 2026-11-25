import type { AppSnapshot, HudWidget } from "../../ipc";
import { STAT_WIDGETS } from "../../widgets/registry";
import { cx, TONE_TEXT } from "../util";

/** The walk's stats STEP_MAP keeps in view: time, distance and today's distance and walking time. */
const READOUT_IDS: HudWidget[] = ["TIMER", "DISTANCE", "DAILY"];
const READOUT = READOUT_IDS.map((id) => STAT_WIDGETS.find((w) => w.id === id)!);

/**
 * One line of the walk's stats for STEP_MAP's header, read from the same
 * widgets as the stat tiles, so the map can stay open without hiding them.
 */
export function WalkReadout({ s }: { s: AppSnapshot }) {
  return (
    <span role="group" aria-label="Walk stats" className="flex flex-wrap items-baseline gap-x-3 font-mono text-[11px]">
      {READOUT.map((w) => {
        const { value, unit } = w.read(s);
        const detail = w.detail?.(s);
        return (
          <span key={w.id} className="whitespace-nowrap" title={w.codename}>
            <span className="text-white/60">{w.label}</span>{" "}
            <span className={cx("font-digital tracking-wider", TONE_TEXT[w.accent])}>{value}</span>
            {unit && <span className="text-white/60"> {unit}</span>}
            {detail && <span className="text-white/60"> · {detail}</span>}
          </span>
        );
      })}
    </span>
  );
}
