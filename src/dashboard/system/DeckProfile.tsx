import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch, type DeckProfileSnapshot } from "../../ipc";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { GhostButton, Hint, Label } from "../ui";

const kmh = (v: number) => `${v.toFixed(1)} km/h`;

/** One line on what DECK_PROFILE knows. */
export function profileSummary(p: DeckProfileSnapshot): string {
  if (p.learned === 0) return "No speeds learned yet.";
  const parts = [`${p.learned} ${p.learned === 1 ? "speed" : "speeds"} learned`];
  if (p.floor_kmh != null) parts.push(`slowest ${kmh(p.floor_kmh)}`);
  if (p.ceiling_kmh != null) parts.push(`fastest ${kmh(p.ceiling_kmh)}`);
  if (p.skipped_kmh.length > 0) parts.push(`skips ${p.skipped_kmh.map((v) => v.toFixed(1)).join(", ")}`);
  return `${parts.join(", ")}.`;
}

/** System -> Diagnostics: what DECK_PROFILE learned from the deck's replies, and FORGET. */
export function DeckProfile() {
  const profile = useAppState((s) => s.deck.profile);
  const forget = useAction();
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
    </div>
  );
}
