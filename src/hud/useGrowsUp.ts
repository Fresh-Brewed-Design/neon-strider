import { useLayoutEffect, useState, type RefObject } from "react";

/** The space `.hud-root.anchor-custom` keeps under the content (its bottom padding). */
const PAD_BOTTOM = 8;

/**
 * Whether the HUD grows upward, so a taller panel would move its controls:
 * in a bottom corner always, at a dragged spot when the content (with the
 * programs module at its reserved height) reaches the screen's foot and the
 * spacer above it gives way. In a top corner it grows downward and only its
 * foot moves.
 *
 * The dragged case is measured from the layout: the reserved shapes keep
 * their content while they take no height, so `scrollHeight` gives what they
 * would add. Reserved or not, the answer is the same, so it never flips back
 * and forth.
 */
export function useGrowsUp(
  contentRef: RefObject<HTMLElement | null>,
  anchor: string | undefined,
  custom: boolean,
): boolean {
  const [pinned, setPinned] = useState(false);
  useLayoutEffect(() => {
    const el = contentRef.current;
    if (!custom || !el) return;
    const check = () => {
      const stack = el.querySelector<HTMLElement>(".hud-program-stack");
      let extra = 0;
      if (stack) {
        const tallest = Math.max(0, ...Array.from(stack.children, (c) => (c as HTMLElement).scrollHeight));
        extra = Math.max(0, tallest - stack.offsetHeight);
      }
      setPinned(el.getBoundingClientRect().bottom + extra >= window.innerHeight - PAD_BOTTOM - 0.5);
    };
    check();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(check);
    ro.observe(el);
    window.addEventListener("resize", check);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", check);
    };
  });
  if (custom) return pinned;
  return anchor === "bottom_right" || anchor === "bottom_left";
}
