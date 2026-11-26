import { useEffect, useState } from "react";
import type { LinkTone, RelinkView } from "./status";
import type { useLogoDrag } from "./useHudDrag";
import { SCRAMBLE_LEAD, TITLE, scrambled } from "./titleScramble";

/** The header's size button: minify on the whole panel, expand on either strip. */
export interface HeaderToggle {
  kind: "minify" | "expand";
  onClick: () => void;
  title: string;
}

/** Four corners pointing in (minify) or out (expand). */
function ToggleIcon({ kind }: { kind: HeaderToggle["kind"] }) {
  const d = kind === "minify" ? "M6 2v4H2M14 6h-4V2M10 14v-4h4M2 10h4v4" : "M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4";
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <path d={d} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="square" />
    </svg>
  );
}

/** One frame of the LINK UP scramble. */
export const SCRAMBLE_FRAME_MS = 45;
/** Frames in the whole scramble: the last letter settles, then it ends (about 0.7 s). */
export const SCRAMBLE_FRAMES = SCRAMBLE_LEAD + TITLE.length;

/**
 * NEON STRIDER, which decodes once each time `linkUp` moves (LINK UP, #374).
 * The real words stay in place, only see-through, so the header never moves
 * and a screen reader always reads NEON STRIDER; the noise is drawn over them
 * and hidden from it.
 */
function HudTitle({ linkUp }: { linkUp: number }) {
  const [glyphs, setGlyphs] = useState<string | null>(null);
  // A connect from before the panel showed (a hidden HUD) is old news: never replay it.
  const [shownAt] = useState(linkUp);
  useEffect(() => {
    if (linkUp === shownAt) return;
    let frame = 0;
    const timer = window.setInterval(() => {
      frame += 1;
      if (frame >= SCRAMBLE_FRAMES) {
        window.clearInterval(timer);
        setGlyphs(null);
      } else {
        setGlyphs(scrambled(frame));
      }
    }, SCRAMBLE_FRAME_MS);
    return () => window.clearInterval(timer);
  }, [linkUp, shownAt]);
  return (
    <>
      <span className={`hud-title-text${glyphs ? " is-decoding" : ""}`}>{TITLE}</span>
      {glyphs && (
        <span className="hud-title-scramble" aria-hidden="true" data-testid="hud-title-scramble">
          {glyphs}
        </span>
      )}
    </>
  );
}

/**
 * The panel's top row, also the strips': the link LED, the draggable logo and
 * the link status, which is a relink button while the link is down. It ends
 * with the size button (`toggle`), in the same corner in every view: minify on
 * the whole panel, expand on the strips. A SIM tag marks the dev builds'
 * simulated treadmill. The logo decodes once per connect (`linkUp`, from
 * `useLinkUp`).
 */
export function HudHeader({
  link,
  relink,
  onRelink,
  logo,
  dragging,
  toggle,
  simulated = false,
  linkUp = 0,
}: {
  link: { text: string; tone: LinkTone };
  relink: RelinkView | null;
  onRelink: () => void;
  logo: ReturnType<typeof useLogoDrag>;
  dragging: boolean;
  /** Minify or expand; omitted while the panel can do neither (a program waiting for GO). */
  toggle?: HeaderToggle;
  /** The linked deck is the dev builds' simulated treadmill. */
  simulated?: boolean;
  /** Moves by one on each connect; 0 before the first. */
  linkUp?: number;
}) {
  return (
    <header className="hud-header">
      <span className={`hud-led tone-${link.tone}`} />
      <span
        className={`hud-title is-draggable${dragging ? " is-dragging" : ""}`}
        title="Drag to move the HUD. Double-click to put it back in its corner."
        {...logo}
      >
        <HudTitle linkUp={linkUp} />
      </span>
      {/* The status and minify stay together; the SIM tag wraps above them. */}
      <span className="hud-header-end">
        {simulated && (
          <span className="hud-sim-tag" title="Simulated treadmill (dev build)">
            SIM
          </span>
        )}
        <span className="hud-header-live">
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
          {toggle && (
            <button
              type="button"
              className="hud-btn hud-btn-minify"
              aria-label={toggle.kind === "minify" ? "Minify the HUD" : "Show the whole HUD"}
              title={toggle.title}
              onClick={toggle.onClick}
            >
              <ToggleIcon kind={toggle.kind} />
            </button>
          )}
        </span>
      </span>
    </header>
  );
}
