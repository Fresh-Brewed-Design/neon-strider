// The belt reminder. ?hours: it counts belt hours instead of distance.
// ?due: well past the distance interval.
import type { MaintenanceSnapshot } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Apply a maintenance change and recompute the derived fields (Rust does this for real). */
function setMaintenance(patch: Partial<MaintenanceSnapshot>) {
  const m = Object.assign(snapshot.maintenance, patch);
  m.days_since = Math.max(0, Math.floor((Date.now() - m.last_service_ms) / 86_400_000));
  const usage = m.basis === "hours" ? m.hours_since / m.interval_hours : m.distance_since_km / m.interval_km;
  const frac = Math.max(
    usage,
    m.interval_days > 0 ? (Date.now() - m.last_service_ms) / (m.interval_days * 86_400_000) : 0,
  );
  m.progress = frac;
  m.due = frac >= 1;
}

export const maintenance: Scenario = {
  apply() {
    if (params.has("hours")) Object.assign(snapshot.maintenance, { basis: "hours", hours_since: 38.2 });
    if (params.has("due")) setMaintenance({ last_service_ms: Date.now() - 95 * 86_400_000, distance_since_km: 212.4, hours_since: 52.6 });
  },
  commands: {
    maintenance_mark_serviced: () => setMaintenance({ last_service_ms: Date.now(), distance_since_km: 0, hours_since: 0 }),
    maintenance_set_last_service: (c) => {
      if (c.at_ms > Date.now() || c.at_ms < Date.UTC(2020, 0, 1)) return;
      // Fake distance: about 5.6 km a day since the date.
      const days = (Date.now() - c.at_ms) / 86_400_000;
      setMaintenance({ last_service_ms: c.at_ms, distance_since_km: Math.round(days * 56) / 10, hours_since: Math.round(days * 17) / 10 });
    },
    maintenance_set_intervals: (c) => setMaintenance({ interval_km: c.km, interval_days: c.days }),
    maintenance_set_basis: (c) => setMaintenance({ basis: c.basis, interval_hours: c.hours }),
  },
};
