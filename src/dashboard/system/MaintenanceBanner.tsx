import { Wrench } from "lucide-react";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton } from "../ui";
import { confirmServiced } from "./maintenance";

/** One-line "belt service due" notice with a Mark serviced action. Renders nothing otherwise. */
export function MaintenanceBanner() {
  const m = useAppSnapshot()?.maintenance;
  if (!m?.due) return null;

  return (
    <div
      role="status"
      className="flex items-center gap-2 rounded border border-neon-magenta/50 bg-neon-magenta/10 px-3 py-1.5"
    >
      <Wrench size={14} className="shrink-0 text-neon-magenta" aria-hidden />
      <span className="min-w-0 flex-1 font-mono text-[11px] font-bold uppercase tracking-[0.08em] text-neon-magenta">
        Belt service due · {Math.round(m.distance_since_km)} km · {m.days_since} days
      </span>
      <GhostButton className="shrink-0 text-neon-magenta hover:text-white" onClick={confirmServiced}>
        Mark serviced
      </GhostButton>
    </div>
  );
}
