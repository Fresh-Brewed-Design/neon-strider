import { useEffect, useRef } from "react";

/** A mouse wheel notch reports about this many pixels of deltaY. */
const NOTCH_PX = 100;
const LINE_PX = 16;
const PAGE_PX = 800;

function toPixels(e: WheelEvent): number {
  if (e.deltaMode === 1) return e.deltaY * LINE_PX;
  if (e.deltaMode === 2) return e.deltaY * PAGE_PX;
  return e.deltaY;
}

/**
 * Turns the mouse wheel over an element into step calls: wheel up is `up: true`.
 * It sends one step per 100 px of accumulated delta, because a mouse notch is
 * about 100 px, so a trackpad swipe of many small deltas can't fire a burst.
 * A change of direction drops what had built up, and one wheel event sends at
 * most one step. Does nothing while `disabled`. The listener is native and
 * non-passive (React's onWheel is passive), so the page doesn't scroll.
 */
export function useWheelSteps<T extends HTMLElement>(onStep: (up: boolean) => void, disabled: boolean) {
  const ref = useRef<T>(null);
  const latest = useRef({ onStep, disabled });
  useEffect(() => {
    latest.current = { onStep, disabled };
  });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let acc = 0;
    const onWheel = (e: WheelEvent) => {
      if (latest.current.disabled) return;
      const dy = toPixels(e);
      if (dy === 0) return;
      e.preventDefault();
      if (Math.sign(dy) !== Math.sign(acc)) acc = 0;
      acc += dy;
      if (Math.abs(acc) < NOTCH_PX) return;
      const up = acc < 0;
      acc %= NOTCH_PX;
      latest.current.onStep(up);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  return ref;
}
