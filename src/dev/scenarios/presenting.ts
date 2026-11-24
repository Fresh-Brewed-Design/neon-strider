// ?presenting: Windows says a presentation or full-screen app is in front, so
// the HUD holds its non-urgent cards (with ?seated, ?eyebreak=upcoming,
// ?weekly, ?due or ?update=available) and shows PRESENTING // CARDS HELD.
// `=off`: the switch off in System -> Desk, so the cards show as usual.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** What the cards were before the hold, so switching it off brings them back. */
let held: (() => void) | null = null;

/** Rust's `presenting::hold_cards`, on the mock's snapshot. */
function holdCards() {
  const s = snapshot;
  s.presenting.holding = s.presenting.enabled && s.presenting.presenting;
  if (!s.presenting.holding) return;
  const before = {
    seated: s.seated.show_card,
    weekly: s.weekly.show_card,
    update: s.update.hud_card,
    maintenance: s.maintenance.hud_card,
    upcoming: s.eye_break.phase === "upcoming",
  };
  held = () => {
    s.seated.show_card ||= before.seated;
    s.weekly.show_card ||= before.weekly;
    s.update.hud_card ||= before.update;
    s.maintenance.hud_card ||= before.maintenance;
    if (before.upcoming && s.eye_break.phase === "idle") s.eye_break.phase = "upcoming";
  };
  s.seated.show_card = false;
  s.weekly.show_card = false;
  s.update.hud_card = false;
  s.maintenance.hud_card = false;
  if (s.eye_break.phase === "upcoming") s.eye_break.phase = "idle";
}

export const presenting: Scenario = {
  apply() {
    const flag = params.get("presenting");
    if (flag === null) return;
    snapshot.presenting.presenting = true;
    snapshot.presenting.enabled = flag !== "off";
    holdCards();
  },
  commands: {
    presenting_set_config: (c) => {
      snapshot.presenting.enabled = c.enabled;
      if (c.enabled) holdCards();
      else {
        snapshot.presenting.holding = false;
        held?.();
        held = null;
      }
    },
  },
};
