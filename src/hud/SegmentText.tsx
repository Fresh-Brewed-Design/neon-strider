import { Fragment, useLayoutEffect, useRef } from "react";
import { splitSegments } from "./segments";

/**
 * A message in whole parts: a narrow card wraps between parts, never inside
 * one while it fits, and a part that ends a line hides its separator, so no
 * line ends in a stray // or ·. The text keeps every separator for copying
 * and screen readers.
 */
export function SegmentText({ text, className }: { text: string; className?: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const segs = splitSegments(text);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const mark = () => {
      const items = Array.from(el.querySelectorAll<HTMLElement>(":scope > .hud-seg"));
      items.forEach((item, i) => {
        const next = items[i + 1];
        item.toggleAttribute("data-wrapped", !!next && next.offsetTop > item.offsetTop);
      });
    };
    mark();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(mark);
    ro.observe(el);
    return () => ro.disconnect();
  });
  return (
    <span ref={ref} className={className ? `hud-segs ${className}` : "hud-segs"}>
      {segs.map((seg, i) => (
        <Fragment key={i}>
          {/* A flex gap draws the space; the text keeps it. */}
          {i > 0 && " "}
          <span className="hud-seg">
            {seg.text}
            {seg.sep && <span className="hud-seg-sep"> {seg.sep}</span>}
          </span>
        </Fragment>
      ))}
    </span>
  );
}
