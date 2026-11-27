import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch } from "../../ipc";
import { useAppState } from "../../state/useAppState";
import { Hint, Label } from "../ui";
import { deckStepsText } from "./deckSteps";
import { Toggle } from "./fields";

/**
 * System -> Diagnostics: the deck's own step counter (`walkaway.deck_steps`)
 * and FOOTFALL, the Labs switch that lets walk-away count those steps as
 * presence (Rust's `walkaway.rs`). The readout shows with the switch off too,
 * so the counter can be checked on the treadmill first.
 */
export function FootfallSetting() {
  const w = useAppState((s) => s.walkaway);
  const toggle = useAction();
  if (!w) return null;

  return (
    <div>
      <Label className="mb-1 block">FOOTFALL</Label>
      <span className="font-mono text-[11px] text-white/70">{deckStepsText(w.deck_steps)}</span>
      <Hint className="mt-0.5">Walk, then step off the belt: a step sensor stops counting.</Hint>
      <div className="mt-3">
        <Toggle
          label="FOOTFALL: Count the deck's steps as presence"
          description="Walk-away treats a moving step count as you on the belt."
          hint={[
            `No steps and no input for ${w.steps_away_s} s starts the walk-away warning.`,
            "Steps alone hold off the warning for up to an hour.",
            "A deck with no moving step count changes nothing.",
            "Walk-away's idle pause must be on.",
          ]}
          checked={w.steps_presence}
          onChange={(on) => void toggle.run(() => dispatch({ type: "walkaway_steps_set", on }))}
        />
        <ActionStatus
          pending={toggle.pending}
          error={toggle.error && `Could not switch FOOTFALL ${w.steps_presence ? "off" : "on"}: ${toggle.error}`}
          className="mt-1"
        />
      </div>
    </div>
  );
}
