import { dispatch, SETTING_LIMITS, type SpeedDriftSnapshot } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { roundSpeed, snapToDeck } from "../../state/deckSpeeds";
import { useAppSnapshot } from "../../state/useAppState";
import { Hint, Label } from "../ui";
import { cx } from "../util";
import { NumberField, Section, Toggle } from "./fields";
import { bandWith, type Band } from "./speedDrift";

const L = SETTING_LIMITS;

type Patch = Partial<Pick<SpeedDriftSnapshot, "enabled" | "low_kmh" | "high_kmh" | "every_minutes">>;

/** What speed drift is doing, in Rust's words. */
function SpeedDriftLive({ drift }: { drift: SpeedDriftSnapshot }) {
  const on = drift.state === "drifting";
  return (
    <div>
      <div className="mb-1 flex flex-wrap items-baseline justify-between gap-x-3">
        <Label>Right now</Label>
        {drift.speed_kmh !== null && (
          <span className={cx("font-digital text-sm", on ? "text-neon-cyan" : "text-white/60")}>
            {drift.speed_kmh.toFixed(1)} km/h
          </span>
        )}
      </div>
      <Hint aria-live="polite">{drift.message}</Hint>
    </div>
  );
}

/** Speed drift (#251): vary a long manual walk's speed within a small band. */
export function SpeedDriftSection() {
  const s = useAppSnapshot();
  const toggle = useAction();
  if (!s) return null;
  const d = s.speed_drift;
  const save = (patch: Patch) =>
    dispatch({
      type: "speed_drift_set_config",
      enabled: d.enabled,
      low_kmh: d.low_kmh,
      high_kmh: d.high_kmh,
      every_minutes: d.every_minutes,
      ...patch,
    });
  const saveEnd = (end: keyof Band, kmh: number) => {
    const band = bandWith({ low: d.low_kmh, high: d.high_kmh }, end, kmh);
    return save({ low_kmh: band.low, high_kmh: band.high });
  };
  const snap = (kmh: number) => roundSpeed(snapToDeck(kmh, s.deck));
  const fit = s.deck.stride_fit_kmh;

  return (
    <Section
      id="sys-drift"
      title="Speed drift"
      description="Speed drift moves the belt speed a little within a band on a long manual walk."
    >
      <div>
        <Toggle
          label="Speed drift"
          tone="cyan"
          checked={d.enabled}
          onChange={(enabled) => void toggle.run(() => save({ enabled }))}
          description="Speed drift only adjusts a belt already running at a speed you set."
          hint="A speed you set by hand ends speed drift until the next walk."
        />
        <ActionStatus pending={toggle.pending} error={toggle.error} className="mt-1" />
        <Hint className="mt-1">Programs, pauses, calls and Neural flow take over from speed drift.</Hint>
      </div>
      {d.enabled && (
        <>
          <SpeedDriftLive drift={d} />
          <NumberField
            label="Slowest speed"
            unit="km/h"
            min={L.speed_drift_kmh.min}
            max={L.speed_drift_kmh.max}
            step={0.1}
            value={d.low_kmh}
            snap={snap}
            hint={`The fastest speed moves too, to keep the band ${L.speed_drift_width_kmh.min} to ${L.speed_drift_width_kmh.max} km/h wide.`}
            onCommit={(kmh) => saveEnd("low", kmh)}
          />
          <NumberField
            label="Fastest speed"
            unit="km/h"
            min={L.speed_drift_kmh.min}
            max={L.speed_drift_kmh.max}
            step={0.1}
            value={d.high_kmh}
            snap={snap}
            hint={
              fit !== null && fit < d.high_kmh
                ? `Speed drift stays at or below STRIDE_FIT ${fit.toFixed(1)} km/h.`
                : "Speed drift only uses speeds the deck runs."
            }
            onCommit={(kmh) => saveEnd("high", kmh)}
          />
          <NumberField
            label="Change every"
            unit="minutes"
            min={L.speed_drift_minutes.min}
            max={L.speed_drift_minutes.max}
            step={1}
            value={d.every_minutes}
            hint={`${L.speed_drift_minutes.min} to ${L.speed_drift_minutes.max}. Each change is at most 0.3 km/h and takes about half a minute.`}
            onCommit={(every_minutes) => save({ every_minutes: Math.round(every_minutes) })}
          />
        </>
      )}
    </Section>
  );
}
