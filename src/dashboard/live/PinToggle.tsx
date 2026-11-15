import { Pin } from "lucide-react";
import type { HudWidget } from "../../ipc";
import { cx, send } from "../util";

/** Show/hide a stat on the HUD, right where the stat is. */
export function PinToggle({ widget, pinned }: { widget: HudWidget; pinned: boolean }) {
  return (
    <button
      type="button"
      aria-pressed={pinned}
      title={pinned ? "Remove from HUD" : "Show on HUD"}
      onClick={() => send({ type: "hud_toggle_widget", widget })}
      className={cx(
        "shrink-0 rounded p-0.5 transition-colors hover:bg-white/10",
        pinned ? "text-neon-cyan" : "text-white/25 hover:text-white/70",
      )}
    >
      <Pin size={12} fill={pinned ? "currentColor" : "none"} />
    </button>
  );
}
