import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { profileSummary } from "../../state/deckSpeeds";
import { isMoving } from "../../state/linkStatus";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { GhostButton, Hint, Label } from "../ui";
import { Toggle } from "./fields";

/** System -> Diagnostics: what DECK_PROFILE learned from the deck's replies, and FORGET. */
export function DeckProfile() {
  const profile = useAppState((s) => s.deck.profile);
  const forget = useAction();
  const sweep = useAction();
  const toggle = useAction();
  const running = useAppState((st) => isMoving(st.deck));
  if (!profile) return null;

  const ask = () =>
    useTreadmillStore.getState().confirm({
      title: "Forget DECK_PROFILE",
      message: "The controls offer every speed again until the treadmill answers new targets.",
      onConfirm: () => void forget.run(() => dispatch({ type: "deck_profile_forget" })),
    });

  return (
    <div>
      <Label className="mb-1 block">DECK_PROFILE</Label>
      <Hint>The speeds the treadmill runs, learned from its replies while you walk.</Hint>
      <div className="mt-2 flex flex-wrap items-center gap-3">
        <span className="font-mono text-[11px] text-white/70">{profileSummary(profile)}</span>
        {profile.learned > 0 && (
          <GhostButton onClick={ask} disabled={forget.pending}>
            Forget
          </GhostButton>
        )}
      </div>
      <ActionStatus pending={forget.pending} error={forget.error && `Could not forget: ${forget.error}`} className="mt-1" />
      <div className="mt-3">
        <Toggle
          label="LABS // DECK_SWEEP: Sweep the deck's speeds"
          description="DECK_SWEEP steps a moving belt through each unlearned speed up to Top speed, then back to your speed."
          checked={profile.sweep_enabled}
          onChange={(on) => void toggle.run(() => dispatch({ type: "deck_profile_sweep_set", on }))}
        />
        <ActionStatus
          pending={toggle.pending}
          error={toggle.error && `Could not switch DECK_SWEEP ${profile.sweep_enabled ? "off" : "on"}: ${toggle.error}`}
          className="mt-1"
        />
        {profile.sweep_enabled && (
          <>
            <Hint className="mt-1">Stopping the belt, a new speed or a program ends DECK_SWEEP at once.</Hint>
            <div className="mt-2 flex flex-wrap items-center gap-3">
              {profile.sweep ? (
                <>
                  <span className="font-mono text-[11px] text-white/70">
                    {profile.sweep.done} / {profile.sweep.total} speeds, now {profile.sweep.kmh.toFixed(1)} km/h
                  </span>
                  <GhostButton
                    onClick={() => void sweep.run(() => dispatch({ type: "deck_profile_sweep_stop" }))}
                    disabled={sweep.pending}
                  >
                    Stop sweep
                  </GhostButton>
                </>
              ) : (
                <GhostButton
                  onClick={() => void sweep.run(() => dispatch({ type: "deck_profile_sweep_start" }))}
                  disabled={sweep.pending || !running}
                >
                  Sweep speeds
                </GhostButton>
              )}
              {!profile.sweep && !running && (
                <span className="text-[11px] text-white/50">Start the belt to sweep its speeds.</span>
              )}
            </div>
            <ActionStatus pending={sweep.pending} error={sweep.error && `Could not sweep: ${sweep.error}`} className="mt-1" />
          </>
        )}
      </div>
    </div>
  );
}
