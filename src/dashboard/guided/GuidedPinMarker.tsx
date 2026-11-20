import { Pin } from "lucide-react";
import type { GuidedTask } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppState } from "../../state/useAppState";
import { post } from "../util";

/** The codename each guided task shows on the pin marker. */
const TASK_NAME: Record<GuidedTask, string> = {
  ground_truth: "GROUND_TRUTH",
};

/**
 * Shown while a guided task holds the dashboard open and on top
 * (`snapshot.guided_task`), with UNPIN to let it hide on blur again.
 */
export function GuidedPinMarker() {
  const pin = useAppState((s) => s.guided_task);
  const unpin = useAction();
  if (!pin?.pinned) return null;
  const name = pin.tasks.map((t) => TASK_NAME[t]).join(" + ");
  const minutes = pin.unpin_in_s === null ? null : Math.max(1, Math.ceil(pin.unpin_in_s / 60));
  return (
    <div
      role="status"
      className="rounded border border-neon-magenta/40 bg-neon-magenta/10 px-2.5 py-2 shadow-[0_0_10px_rgba(255,0,170,0.25)]"
    >
      <div className="flex items-center gap-1.5 text-neon-magenta">
        <Pin size={12} className="shrink-0" />
        <span className="font-mono text-[11px] font-bold uppercase tracking-[0.14em]">LOCKED ON TOP</span>
      </div>
      <p
        className="mt-1 text-[11px] leading-snug text-white/70"
        title={minutes === null ? undefined : `The pin lets go by itself in ${minutes} min.`}
      >
        Dashboard held open for {name}.
      </p>
      <button
        type="button"
        onClick={() => void unpin.run(post({ type: "guided_task_cancel" }))}
        disabled={unpin.pending}
        className="mt-1.5 w-full rounded border border-neon-magenta/50 px-2 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.18em] text-neon-magenta transition-colors hover:bg-neon-magenta/15 disabled:opacity-50"
      >
        UNPIN
      </button>
      <ActionStatus pending={unpin.pending} error={unpin.error} className="mt-1" />
    </div>
  );
}
