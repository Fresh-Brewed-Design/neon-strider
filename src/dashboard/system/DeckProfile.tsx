import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { profileSummary } from "../../state/deckSpeeds";
import { isMoving } from "../../state/linkStatus";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { GhostButton, Hint, Label } from "../ui";

/** System -> Diagnostics: what DECK_PROFILE learned from the deck's replies, and FORGET. */
export function DeckProfile() {
  const profile = useAppState((s) => s.deck.profile);
  const forget = useAction();
  const sweep = useAction();
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
        <Label className="mb-1 block">LABS // DECK_SWEEP</Label>
        <Hint>
          Steps the moving belt through every speed it has not answered yet, one by one, then back to your speed. It runs only on your press and
          ends if you stop the belt, change the speed or start a program.
        </Hint>
        <div className="mt-2 flex flex-wrap items-center gap-3">
          {profile.sweep ? (
            <>
              <span className="font-mono text-[11px] text-white/70">
                {profile.sweep.done} / {profile.sweep.total} speeds, now {profile.sweep.kmh.toFixed(1)} km/h
              </span>
              <GhostButton onClick={() => void sweep.run(() => dispatch({ type: "deck_profile_sweep_stop" }))} disabled={sweep.pending}>
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
          {!profile.sweep && !running && <span className="text-[11px] text-white/50">Start the belt first.</span>}
        </div>
        <ActionStatus pending={sweep.pending} error={sweep.error && `Could not sweep: ${sweep.error}`} className="mt-1" />
      </div>
    </div>
  );
}
