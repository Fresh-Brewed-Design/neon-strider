import { Wrench } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { GhostButton } from "../ui";
import { post } from "../util";
import { confirmServiced } from "./maintenance";

/**
 * One-line "belt service due" notice with a Mark serviced action. It sits in the Live page
 * header, where it costs no height; a refused Mark serviced shows on a line under it.
 * Renders nothing otherwise.
 */
export function MaintenanceBanner() {
  const m = useAppSnapshot()?.maintenance;
  const serviced = useAction();
  if (!m?.due) return null;

  return (
    <div className="flex flex-col items-end gap-0.5">
      <div
        role="status"
        className="flex items-center gap-2 rounded border border-neon-yellow/50 bg-neon-yellow/10 px-3 py-0.5"
      >
        <Wrench size={14} className="shrink-0 text-neon-yellow" aria-hidden />
        <span className="min-w-0 flex-1 whitespace-nowrap font-mono text-[11px] font-bold uppercase tracking-[0.04em] text-neon-yellow">
          Belt service due · {Math.round(m.distance_since_km)} km · {m.days_since} days
        </span>
        <GhostButton
          className="shrink-0 !py-0 !tracking-[0.06em] text-neon-yellow hover:text-white"
          disabled={serviced.pending}
          onClick={() => confirmServiced(() => void serviced.run(post({ type: "maintenance_mark_serviced" })))}
        >
          Mark serviced
        </GhostButton>
      </div>
      <ActionStatus error={serviced.error} className="justify-end text-right" />
    </div>
  );
}
