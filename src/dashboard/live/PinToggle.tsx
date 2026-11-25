import { useEffect } from "react";
import { Pin } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import type { HudWidget } from "../../ipc";
import { cx, post } from "../util";

/** How long a refused pin's note stays over the tile before it clears. */
const NOTE_MS = 8000;

/**
 * Show/hide a stat on the HUD, right where the stat is. A refusal shows in a
 * small note just under the pin, over the tile, so the page keeps its height.
 */
export function PinToggle({ widget, pinned }: { widget: HudWidget; pinned: boolean }) {
  const toggle = useAction();
  const { error, reset } = toggle;
  useEffect(() => {
    if (!error) return;
    const t = setTimeout(reset, NOTE_MS);
    return () => clearTimeout(t);
  }, [error, reset]);

  return (
    <span className="relative inline-flex shrink-0">
      <button
        type="button"
        aria-pressed={pinned}
        title={pinned ? "Remove from HUD" : "Show on HUD"}
        disabled={toggle.pending}
        onClick={() => void toggle.run(post({ type: "hud_toggle_widget", widget }))}
        className={cx(
          "flex h-6 w-6 shrink-0 items-center justify-center rounded transition-colors hover:bg-white/10",
          pinned ? "text-neon-cyan" : "text-white/60 hover:text-white",
        )}
      >
        <Pin size={14} fill={pinned ? "currentColor" : "none"} />
      </button>
      {error && (
        <span className="absolute right-0 top-[calc(100%-2px)] z-10 block w-max max-w-40" onClick={reset}>
          {/* Ambient: the note sits right by the pin just pressed, and scrolling it into view
              would scroll the tile's clipped content instead of the page. */}
          <ActionStatus
            ambient
            error={error}
            className="rounded border border-red-400/40 bg-black px-1.5 !leading-snug text-left normal-case tracking-normal"
          />
        </span>
      )}
    </span>
  );
}
