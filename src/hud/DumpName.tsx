/**
 * DUMP's name with its stop square, the same on the panel, the strip and the
 * walk-away card. The square takes the label's colour, so it turns dark with
 * the label while DUMP is pressed.
 */
export function DumpName() {
  return (
    <span className="hud-dump-name">
      <span className="hud-dump-glyph" aria-hidden />
      DUMP
    </span>
  );
}
