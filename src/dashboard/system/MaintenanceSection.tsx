import { useState } from "react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch, type MaintenanceBasis } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { KM_PER_MI } from "../../units";
import { Hint, Label, NeonButton } from "../ui";
import { cx, send } from "../util";
import { INPUT_CLASS, NumberField, Section } from "./fields";
import { confirmServiced, fromDateInput, toDateInput } from "./maintenance";

const BASES: { id: MaintenanceBasis; label: string }[] = [
  { id: "distance", label: "Distance + days" },
  { id: "hours", label: "Belt hours + days" },
];

const toMi = (km: number) => km / KM_PER_MI;

/** One figure, against its service interval when there is one. */
function Reading({ label, value, limit }: { label: string; value: string; limit?: string }) {
  return (
    <div className="min-w-0">
      <Label className="block">{label}</Label>
      <div className="mt-0.5 font-digital text-sm text-white">
        {value}
        {limit && <> <span className="font-mono text-[11px] text-white/45">/ {limit}</span></>}
      </div>
    </div>
  );
}

/** "12 Mar 2026" in the user's locale. */
function formatSince(ms: number | null): string {
  if (ms === null) return "no walks yet";
  return new Date(ms).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/** Belt service reminder: progress against the distance (or belt hours) and time intervals. */
export function MaintenanceSection() {
  const s = useAppSnapshot();
  const [today] = useState(() => toDateInput(Date.now()));
  const lastService = useAction();
  if (!s) return null;
  const m = s.maintenance;
  const odo = s.odometer;
  const hoursBasis = m.basis === "hours";

  const pct = Math.min(100, Math.max(0, m.progress * 100));
  const lastDate = toDateInput(m.last_service_ms);
  const daysLimit = m.interval_days > 0 ? `${m.interval_days} days` : "no limit";

  return (
    <Section
      id="sys-maintenance"
      title="Maintenance"
      description="Jogwell recommends lubricating the deck every 3 months or 100 miles, whichever comes first. Use 100% silicone oil."
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-end justify-between gap-x-6 gap-y-2">
          {hoursBasis ? (
            <Reading
              label="Belt time"
              value={`${m.hours_since.toFixed(1)} h`}
              limit={`${Math.round(m.interval_hours * 10) / 10} h`}
            />
          ) : (
            <Reading
              label="Distance"
              value={`${toMi(m.distance_since_km).toFixed(1)} mi · ${m.distance_since_km.toFixed(1)} km`}
              limit={`${toMi(m.interval_km).toFixed(0)} mi`}
            />
          )}
          <Reading label="Time" value={`${m.days_since} days`} limit={daysLimit} />
          <span
            className={cx(
              "font-mono text-[11px] font-bold uppercase tracking-[0.2em]",
              m.due ? "text-neon-magenta" : "text-white/50",
            )}
          >
            {m.due ? "Service due" : `${Math.round(m.progress * 100)}% of interval`}
          </span>
        </div>
        <div
          role="progressbar"
          aria-label="Time to belt service"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(pct)}
          className="h-2 w-full overflow-hidden rounded-full bg-white/10"
        >
          <div
            className={cx(
              "h-full rounded-full transition-all",
              m.due ? "bg-neon-magenta shadow-[0_0_8px_var(--color-neon-magenta)]" : "bg-neon-cyan",
            )}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>

      <div>
        <Label>Count service by</Label>
        <div role="group" aria-label="Count service by" className="mt-1.5 flex flex-wrap gap-1.5">
          {BASES.map((b) => (
            <button
              key={b.id}
              type="button"
              aria-pressed={m.basis === b.id}
              onClick={() => m.basis !== b.id && send({ type: "maintenance_set_basis", basis: b.id, hours: m.interval_hours })}
              className={cx(
                "rounded border px-2.5 py-1.5 font-mono text-[11px] transition-colors",
                m.basis === b.id
                  ? "border-neon-cyan bg-neon-cyan/15 text-neon-cyan"
                  : "border-white/15 text-white/70 hover:border-neon-cyan/50",
              )}
            >
              {b.label}
            </button>
          ))}
        </div>
        <Hint className="mt-1.5">
          {hoursBasis
            ? "Hours the belt has run, from the treadmill's own timer."
            : "Jogwell's own figure: 100 miles."}
        </Hint>
      </div>

      <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
        <label className="block">
          <Label>Last service</Label>
          <div className="mt-1">
            <input
              type="date"
              aria-label="Last service date"
              value={lastDate}
              min="2020-01-01"
              max={today}
              onChange={(e) => {
                const at = fromDateInput(e.target.value);
                if (at === null || at > Date.now() || at === m.last_service_ms) return;
                void lastService.run(() => dispatch({ type: "maintenance_set_last_service", at_ms: at }));
              }}
              className={cx(INPUT_CLASS, "[color-scheme:dark]")}
            />
          </div>
          <ActionStatus error={lastService.error} className="mt-1" />
        </label>
        {hoursBasis ? (
          <NumberField
            label="Every"
            value={Math.round(m.interval_hours * 10) / 10}
            min={5}
            max={1000}
            step={5}
            unit="belt hours"
            onCommit={(hours) => dispatch({ type: "maintenance_set_basis", basis: m.basis, hours })}
          />
        ) : (
          <NumberField
            label="Every"
            value={Math.round(toMi(m.interval_km) * 10) / 10}
            min={31.1}
            max={3106.8}
            step={10}
            unit="miles"
            onCommit={(mi) => dispatch({ type: "maintenance_set_intervals", km: mi * KM_PER_MI, days: m.interval_days })}
          />
        )}
        <NumberField
          label="Or every"
          value={m.interval_days}
          min={0}
          max={730}
          unit="days (0 = no limit)"
          onCommit={(days) => dispatch({ type: "maintenance_set_intervals", km: m.interval_km, days })}
        />
      </div>

      <div role="group" aria-label="Odometer" className="border-t border-white/10 pt-3">
        <Label>Odometer</Label>
        <div className="mt-1.5 flex flex-wrap gap-x-8 gap-y-2">
          <Reading label="Distance" value={`${odo.distance_km.toFixed(1)} km`} />
          <Reading label="Belt time" value={`${odo.belt_hours.toFixed(1)} h`} />
          <Reading label="Since" value={formatSince(odo.since_ms)} />
        </div>
        <Hint className="mt-1.5">
          Every saved session plus the one running now. Pin it to the HUD from the HUD page (ODO).
        </Hint>
      </div>

      <Hint>
        To check: unplug, lift the belt edge and touch the deck. Slightly oily is fine; dry means it needs oil.
      </Hint>

      <div>
        <NeonButton tone={m.due ? "magenta" : "cyan"} className="px-4 py-2" onClick={confirmServiced}>
          Mark serviced
        </NeonButton>
      </div>
    </Section>
  );
}
