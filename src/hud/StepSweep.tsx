import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { sweepSpot, sweepTop, type StepSweep as Sweep } from "./hudMotion";

/** How long one sweep shows; hud.css runs its animation for the same time. */
export const STEP_SWEEP_MS = 900;

/**
 * The step change sweep (#376): one yellow scanline down the whole panel's
 * body, with the new step's name over the stats (`sweepSpot`), never over the
 * header, the status row, a control or a card. Drawn over the panel and taking
 * no clicks, so nothing on the panel moves and every control still works under
 * it; the panel's note draws over it. HudApp mounts it on the whole panel only,
 * so a step change while the HUD is a strip never sweeps, then or on expand.
 * Decorative: the program line already says the step in words, so it is
 * hidden from screen readers.
 */
export function StepSweep({ sweep }: { sweep: Sweep | null }) {
  const key = sweep?.key ?? 0;
  // A sweep from before the panel showed (a strip, a hidden HUD) is old news: never replay it.
  const [done, setDone] = useState(key);
  const overlayRef = useRef<HTMLDivElement>(null);
  const spotRef = useRef<HTMLSpanElement>(null);
  const showing = sweep !== null && done !== key;
  useEffect(() => {
    if (key === 0) return;
    const timer = window.setTimeout(() => setDone(key), STEP_SWEEP_MS);
    return () => window.clearTimeout(timer);
  }, [key]);
  // Placed before the first paint, from where the panel's body and stats are now.
  useLayoutEffect(() => {
    const overlay = overlayRef.current;
    const spot = spotRef.current;
    const panel = overlay?.parentElement;
    if (!showing || !overlay || !spot || !panel) return;
    overlay.style.top = `${sweepTop(panel)}px`;
    const s = sweepSpot(panel, overlay);
    if (s) Object.assign(spot.style, { display: "", top: `${s.top}px`, left: `${s.left}px`, width: `${s.width}px`, height: `${s.height}px` });
    else spot.style.display = "none";
  }, [showing, key]);
  if (!showing) return null;
  return (
    <div key={key} ref={overlayRef} className="hud-step-sweep" aria-hidden="true" data-testid="hud-step-sweep">
      <span className="hud-step-sweep-line" />
      <span ref={spotRef} className="hud-step-sweep-spot">
        <span className="hud-step-sweep-name">
          <span className="hud-step-sweep-tag">STEP //</span> {sweep.label}
        </span>
      </span>
    </div>
  );
}
