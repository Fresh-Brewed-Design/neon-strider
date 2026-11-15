import type { AppSnapshot } from "../ipc";
import { startFailure } from "../state/linkStatus";
import { statWidget } from "../widgets/registry";
import { HudNote } from "./HudNote";
import { EngageButton } from "./HudPanel";
import { useHudSend } from "./useHudSend";

/**
 * The panel collapsed during stasis (`hud_view` "strip"): under the header,
 * today's total (DAILY_TOTAL), expand (the whole panel until the belt next
 * moves) and Engage.
 */
export function StripRow({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  const daily = statWidget("DAILY");
  const total = daily?.read(snapshot);
  return (
    <div className="hud-strip">
      <div className="hud-strip-row">
        {daily && total && (
          <div className={`hud-strip-total accent-${daily.accent}`} title={daily.codename}>
            <span className="hud-label">{daily.label}</span>
            <span className="hud-value hud-value-sm">
              {total.value}
              {total.unit && <span className="hud-unit">{total.unit}</span>}
            </span>
          </div>
        )}
        <button
          type="button"
          className="hud-btn hud-btn-expand"
          aria-label="Show the whole HUD"
          title="Show the whole HUD until the belt moves again"
          onClick={() => void send({ type: "hud_expand" })}
        >
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path
              d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="square"
            />
          </svg>
        </button>
        <EngageButton snapshot={snapshot} send={send} />
      </div>
      <HudNote error={error ?? startFailure(snapshot)} />
    </div>
  );
}
