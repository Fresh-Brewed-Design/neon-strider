import type { LinkTone, RelinkView } from "./status";
import type { useLogoDrag } from "./useHudDrag";

/**
 * The panel's top row, also the strip's: the link LED, the draggable logo and
 * the link status, which is a relink button while the link is down.
 */
export function HudHeader({
  link,
  relink,
  onRelink,
  logo,
  dragging,
}: {
  link: { text: string; tone: LinkTone };
  relink: RelinkView | null;
  onRelink: () => void;
  logo: ReturnType<typeof useLogoDrag>;
  dragging: boolean;
}) {
  return (
    <header className="hud-header">
      <span className={`hud-led tone-${link.tone}`} />
      <span
        className={`hud-title is-draggable${dragging ? " is-dragging" : ""}`}
        title="Drag to move the HUD. Double-click to put it back in its corner."
        {...logo}
      >
        NEON STRIDER
      </span>
      {relink?.clickable ? (
        <button
          type="button"
          className={`hud-status hud-relink tone-${link.tone}`}
          title={relink.title ?? undefined}
          onClick={onRelink}
        >
          {link.text}
        </button>
      ) : (
        <span className={`hud-status tone-${link.tone}`} title={relink?.title ?? undefined}>
          {link.text}
        </span>
      )}
    </header>
  );
}
