import { useEffect, useState } from "react";
import type { StepSweep as Sweep } from "./hudMotion";

/** How long one sweep shows; hud.css runs its animation for the same time. */
export const STEP_SWEEP_MS = 900;

/**
 * The step change sweep (#376): one yellow scanline down the panel with the
 * new step's name, drawn over the panel and taking no clicks, so nothing on
 * the panel moves and every control still works under it. Decorative: the
 * program line already says the step in words, so it is hidden from screen
 * readers.
 */
export function StepSweep({ sweep }: { sweep: Sweep | null }) {
  const key = sweep?.key ?? 0;
  // A sweep from before the panel showed (hidden HUD, still belt) is old news: never replay it.
  const [done, setDone] = useState(key);
  useEffect(() => {
    if (key === 0) return;
    const timer = window.setTimeout(() => setDone(key), STEP_SWEEP_MS);
    return () => window.clearTimeout(timer);
  }, [key]);
  if (!sweep || done === key) return null;
  return (
    <div key={key} className="hud-step-sweep" aria-hidden="true" data-testid="hud-step-sweep">
      <span className="hud-step-sweep-line" />
      <span className="hud-step-sweep-name">
        <span className="hud-step-sweep-tag">STEP //</span> {sweep.label}
      </span>
    </div>
  );
}
