import type { AppSnapshot, HudWidget } from "../../ipc";
import { QuotaBar } from "../../widgets/QuotaBar";
import { STAT_WIDGETS, statTitle } from "../../widgets/registry";
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
    // With no treadmill linked the stats are the last ones it sent, dimmed as on the HUD.
    <span
      role="group"
      aria-label="Walk stats"
      className={cx("flex flex-wrap items-baseline gap-x-3 font-mono text-[11px]", !s.connection.connected && "opacity-45")}
    >
      {READOUT.map((w) => {
        const { value, of, unit } = w.read(s);
        const detail = w.detail?.(s);
        const quota = w.quota?.(s);
        return (
          <span key={w.id} className={cx("whitespace-nowrap", quota && "relative")} title={statTitle(w)}>
            <span className="text-white/60">{w.label}</span>{" "}
            <span className={cx("font-digital tracking-wider", TONE_TEXT[w.accent])}>{value}</span>
            {of && <span className="text-white/60"> {of}</span>}
            {unit && <span className="text-white/60"> {unit}</span>}
            {detail && <span className="text-white/60"> · {detail}</span>}
            {/* Under the reading, out of the flow, so the header keeps its height. */}
            {quota && <QuotaBar quota={quota} className={cx("absolute inset-x-0 -bottom-0.5 h-px overflow-hidden", TONE_TEXT[w.accent])} />}
          </span>
        );
      })}
    </span>
  );
}
