/** A rejected command, inside the card that holds the button. */
export function HudNote({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <div className="hud-note" role="alert">
      {error}
    </div>
  );
}
