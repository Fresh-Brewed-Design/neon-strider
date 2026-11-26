import { useLayoutEffect, useRef, useState } from "react";

/** A rejected command, inside the card that holds the button. */
export function HudNote({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <div className="hud-note" role="alert">
      {error}
    </div>
  );
}

/** A note on the panel: the themed line, and Rust's plain words under it when it has some. */
export interface PanelNote {
  text: string;
  detail?: string | null;
}

/**
 * The panel's one note (a refused press, a deck reply, a start that failed),
 * drawn over the readings next to the controls instead of between them, so
 * no control moves when it comes or goes. hud.css places it in each view;
 * clicks pass through it. A note with plain words under it (a deck reply)
 * shows as its one themed line while that line fits, so it covers no
 * reading; the plain words are then the panel row's tooltip and still read
 * out. A line too long for one row shows whole, with the words under it.
 */
export function NotePop({ note }: { note: PanelNote | null }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [fits, setFits] = useState(true);
  const detail = note?.detail ?? null;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !detail) return;
    // Wider than the row on one line: the whole note shows.
    const check = () => setFits(el.scrollWidth <= el.clientWidth + 1);
    check();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  }, [detail, note?.text]);
  if (!note) return null;
  const oneLine = detail !== null && fits;
  return (
    <div className={`hud-note hud-note-pop${oneLine ? " is-one-line" : ""}`} role="alert" data-testid="hud-note">
      <span className="hud-note-text">{note.text}</span>
      {detail && <span className="hud-note-detail">{detail}</span>}
      {/* The themed line laid out on one line, unseen, to see if it fits. */}
      {detail && (
        <span ref={ref} className="hud-note-measure" aria-hidden="true">
          {note.text}
        </span>
      )}
    </div>
  );
}
