import type { LinkTone, RelinkView } from "./status";
import type { useLogoDrag } from "./useHudDrag";

/**
 * The panel's top row, also the strip's: the link LED, the draggable logo and
 * the link status, which is a relink button while the link is down. On the
 * whole panel it ends with minify (`onMinify`). A SIM tag marks the dev
 * builds' simulated treadmill.
 */
export function HudHeader({
  link,
  relink,
  onRelink,
  logo,
  dragging,
  onMinify,
  minifyKey = null,
  simulated = false,
}: {
  link: { text: string; tone: LinkTone };
  relink: RelinkView | null;
  onRelink: () => void;
  logo: ReturnType<typeof useLogoDrag>;
  dragging: boolean;
  /** Minify to the running strip; omitted on the strips. */
  onMinify?: () => void;
  /** The minify hotkey for the tooltip ("Alt+Insert"); null while it is off. */
  minifyKey?: string | null;
  /** The linked deck is the dev builds' simulated treadmill. */
  simulated?: boolean;
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
      {/* Status and minify wrap to a line of their own together. */}
      <span className="hud-header-end">
        {simulated && (
          <span className="hud-sim-tag" title="Simulated treadmill (dev build)">
            SIM
          </span>
        )}
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
        {onMinify && (
          <button
            type="button"
            className="hud-btn hud-btn-minify"
            aria-label="Minify the HUD"
            title={minifyKey ? `Shrink the HUD to a strip (${minifyKey})` : "Shrink the HUD to a strip"}
            onClick={onMinify}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
              <path
                d="M6 2v4H2M14 6h-4V2M10 14v-4h4M2 10h4v4"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="square"
              />
            </svg>
          </button>
        )}
      </span>
    </header>
  );
}
