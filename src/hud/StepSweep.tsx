import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { sweepSpot, type StepSweep as Sweep } from "./hudMotion";

/** How long one sweep shows; hud.css runs its animation for the same time. */
export const STEP_SWEEP_MS = 900;

/**
 * The step change sweep (#376): one yellow scanline down the panel, with the
 * new step's name over the readings (`sweepSpot`), never over a control. Drawn
 * over the panel and taking no clicks, so nothing on the panel moves and every
 * control still works under it. Decorative: the program line already says the
 * step in words, so it is hidden from screen readers.
 */
export function StepSweep({ sweep }: { sweep: Sweep | null }) {
  const key = sweep?.key ?? 0;
  // A sweep from before the panel showed (hidden HUD, still belt) is old news: never replay it.
  const [done, setDone] = useState(key);
  const spotRef = useRef<HTMLSpanElement>(null);
  const showing = sweep !== null && done !== key;
  useEffect(() => {
    if (key === 0) return;
    const timer = window.setTimeout(() => setDone(key), STEP_SWEEP_MS);
    return () => window.clearTimeout(timer);
  }, [key]);
  // Placed before the first paint, from where the panel's readings are now.
  useLayoutEffect(() => {
    const spot = spotRef.current;
    const overlay = spot?.parentElement;
    const panel = overlay?.parentElement;
    if (!showing || !spot || !overlay || !panel) return;
    const s = sweepSpot(panel, overlay);
    Object.assign(spot.style, { top: `${s.top}px`, left: `${s.left}px`, width: `${s.width}px`, height: `${s.height}px` });
  }, [showing, key]);
  if (!showing) return null;
  return (
    <div key={key} className="hud-step-sweep" aria-hidden="true" data-testid="hud-step-sweep">
      <span className="hud-step-sweep-line" />
      <span ref={spotRef} className="hud-step-sweep-spot">
        <span className="hud-step-sweep-name">
          <span className="hud-step-sweep-tag">STEP //</span> {sweep.label}
        </span>
      </span>
    </div>
  );
}
