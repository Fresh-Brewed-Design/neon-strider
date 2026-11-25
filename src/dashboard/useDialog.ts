import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The dialogs open right now, newest last: only the top one answers Escape and Tab. */
const stack: symbol[] = [];

function focusables(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE));
}

/**
 * What makes a dialog behave like one, for every dashboard dialog: while
 * `open`, focus moves into it (`initialFocus`, else its first control, else
 * the dialog itself, which needs `tabIndex={-1}`), Tab and Shift+Tab stay
 * inside, Escape calls `onClose`, and on close focus goes back to the control
 * that opened it. Put the returned ref on the dialog's box.
 */
export function useDialog<T extends HTMLElement = HTMLDivElement>(
  open: boolean,
  onClose: () => void,
  initialFocus?: RefObject<HTMLElement | null>,
): RefObject<T | null> {
  const ref = useRef<T>(null);
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  });

  useEffect(() => {
    if (!open) return;
    const id = Symbol("dialog");
    stack.push(id);
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const box = ref.current;
    (initialFocus?.current ?? (box && focusables(box)[0]) ?? box)?.focus();

    // Capture phase, so the dialog has Escape before the page's own key handlers.
    const onKey = (e: KeyboardEvent) => {
      const root = ref.current;
      if (stack[stack.length - 1] !== id || !root) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close.current();
        return;
      }
      if (e.key !== "Tab") return;
      const items = focusables(root);
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      const inside = active instanceof Node && root.contains(active);
      if (!first || !last) {
        e.preventDefault();
        root.focus();
      } else if (e.shiftKey && (active === first || !inside)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (active === last || !inside)) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      stack.splice(stack.indexOf(id), 1);
      if (opener?.isConnected) opener.focus();
    };
  }, [open, initialFocus]);

  return ref;
}
