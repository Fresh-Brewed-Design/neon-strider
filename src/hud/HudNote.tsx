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
 * clicks pass through it.
 */
export function NotePop({ note }: { note: PanelNote | null }) {
  if (!note) return null;
  return (
    <div className="hud-note hud-note-pop" role="alert" data-testid="hud-note">
      <span className="hud-note-text">{note.text}</span>
      {note.detail && <span className="hud-note-detail">{note.detail}</span>}
    </div>
  );
}
