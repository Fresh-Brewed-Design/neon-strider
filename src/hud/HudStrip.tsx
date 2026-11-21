import { Fragment } from "react";
import type { AppSnapshot, HudWidget } from "../ipc";
import { describeActive } from "../dashboard/programs/format";
import { startFailure } from "../state/linkStatus";
import { statWidget } from "../widgets/registry";
import { Sparkline } from "../widgets/Sparkline";
import { HudNote } from "./HudNote";
import { EngageButton } from "./HudPanel";
import { useHudSend } from "./useHudSend";

/** Four corners pointing out: show the whole HUD. */
function ExpandIcon() {
  return (
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
      <path
        d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="square"
      />
    </svg>
  );
}

/**
 * The panel collapsed during stasis (`hud_view` "strip"): under the header,
 * today's total (DAILY_TOTAL), expand (the whole panel until the belt next
 * moves) and Engage.
 */
export function StripRow({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const daily = statWidget("DAILY");
  const total = daily?.read(snapshot);
  return (
    <div className="hud-strip">
      <div className="hud-strip-row">
        {daily && total && (
          <div className={`hud-strip-total accent-${daily.accent}`} title={daily.codename}>
            <span className="hud-label">{daily.label}</span>
            <span className="hud-value hud-value-sm">
              {total.value}
              {total.unit && <span className="hud-unit">{total.unit}</span>}
            </span>
          </div>
        )}
        <button
          type="button"
          className="hud-btn hud-btn-expand"
          aria-label="Show the whole HUD"
          title="Show the whole HUD until the belt moves again"
          onClick={() => void send({ type: "hud_expand" })}
        >
          <ExpandIcon />
        </button>
        <EngageButton snapshot={snapshot} send={send} />
      </div>
      <HudNote error={error ?? startFailure(snapshot)} />
    </div>
  );
}

const RUN_STATS: HudWidget[] = ["VELOCITY", "TIMER", "DISTANCE"];

/**
 * The panel minified by the user (`hud_view` "minified"): speed, time and
 * distance, heart rate with a sensor picked, the program step when one runs,
 * then expand, Engage/Stasis and Dump. It stays until expanded again.
 */
/** The program line as whole segments; each keeps its trailing dot so a wrap
 *  never starts a line with one. */
function programSegments(name: string, detail: string): string[] {
  return [name, ...detail.split(" · ")];
}

/**
 * PULSE_RATE on the running strip, only while a heart rate sensor is picked:
 * the bpm, then its spark chart in the rest of the row. Without a sensor the
 * strip stays as it was.
 */
function PulseRow({ snapshot }: { snapshot: AppSnapshot }) {
  const def = statWidget("HEART_RATE");
  if (!def || snapshot.heart_rate.sensor === null) return null;
  const { value, unit } = def.read(snapshot);
  const spark = def.spark?.(snapshot);
  return (
    <div className={`hud-run-pulse accent-${def.accent}`} title={def.codename} data-testid="hud-run-pulse">
      <div className="hud-strip-total">
        <span className="hud-label">{def.label}</span>
        <span className="hud-value hud-value-sm">
          {value}
          {unit && <span className="hud-unit">{unit}</span>}
        </span>
      </div>
      {spark && <Sparkline data={spark} className="hud-spark hud-run-spark" />}
    </div>
  );
}

export function RunStripRow({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const { deck, connection } = snapshot;
  const a = snapshot.active_program;
  const stopped = deck.status === "Stopped" || deck.status === "Stopping";
  return (
    <div className="hud-strip hud-run-strip">
      <div className="hud-run-stats">
        {RUN_STATS.map((id) => {
          const def = statWidget(id);
          if (!def) return null;
          const { value, unit } = def.read(snapshot);
          return (
            <div key={id} className={`hud-strip-total accent-${def.accent}`} title={def.codename}>
              <span className="hud-label">{def.label}</span>
              <span className="hud-value hud-value-sm">
                {value}
                {unit && <span className="hud-unit">{unit}</span>}
              </span>
            </div>
          );
        })}
      </div>
      <PulseRow snapshot={snapshot} />
      {a && (
        <div className="hud-run-program accent-yellow">
          <span className="hud-label">{a.paused ? "PAUSED" : "PROGRAM"}</span>
          <span className="hud-sub hud-run-segments">
            {programSegments(a.name, describeActive(a)).map((seg, i, all) => (
              <Fragment key={i}>
                {i > 0 && " "}
                <span className={i === 0 ? "hud-run-seg hud-run-seg-name" : "hud-run-seg"} title={i === 0 ? a.name : undefined}>
                  {i < all.length - 1 ? `${seg} ·` : seg}
                </span>
              </Fragment>
            ))}
          </span>
        </div>
      )}
      <div className="hud-strip-row">
        <button
          type="button"
          className="hud-btn hud-btn-expand"
          aria-label="Show the whole HUD"
          title="Show the whole HUD (Alt+Insert)"
          onClick={() => void send({ type: "hud_set_minified", minified: false })}
        >
          <ExpandIcon />
        </button>
        <EngageButton snapshot={snapshot} send={send} />
        <button
          type="button"
          className="hud-btn hud-btn-stop accent-magenta"
          disabled={!connection.connected || stopped}
          onClick={() => void send({ type: "stop" })}
        >
          DUMP
        </button>
      </div>
      <HudNote error={error ?? startFailure(snapshot)} />
    </div>
  );
}
