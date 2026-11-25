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
    <span className="relative shrink-0">
      <button
        type="button"
        aria-pressed={pinned}
        title={pinned ? "Remove from HUD" : "Show on HUD"}
        disabled={toggle.pending}
        onClick={() => void toggle.run(post({ type: "hud_toggle_widget", widget }))}
        className={cx(
          "shrink-0 rounded p-0.5 transition-colors hover:bg-white/10",
          pinned ? "text-neon-cyan" : "text-white/25 hover:text-white/70",
        )}
      >
        <Pin size={12} fill={pinned ? "currentColor" : "none"} />
      </button>
      {error && (
        <span className="absolute right-0 top-full z-10 mt-0.5 block w-max max-w-40" onClick={reset}>
          <ActionStatus
            error={error}
            className="rounded border border-red-400/40 bg-black px-1.5 py-0.5 text-left normal-case tracking-normal"
          />
        </span>
      )}
    </span>
  );
}
