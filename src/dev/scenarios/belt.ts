// The belt itself. ?idle: the belt stopped. ?daily=12.34: today's total
// (DAILY_TOTAL) in km, to check long values fit; ?dailytime=659 today's
// walking time beside it in minutes (659 is 10h 59m); ?distance=12.34 the same for
// this walk's distance (DIST). ?decknote=refused|adjusted:
// the HUD note after the deck refuses or changes a target, with Rust's plain words.
import { params, snapshot } from "../mockState";
import { setProgramPaused } from "./programs";
import type { Scenario } from "./types";

export const belt: Scenario = {
  apply() {
    if (params.has("idle")) snapshot.deck.status = "Stopped";
    const dailyFlag = Number(params.get("daily"));
    if (params.has("daily") && Number.isFinite(dailyFlag)) snapshot.session.daily_distance_m = Math.round(dailyFlag * 1000);
    const dailyTimeFlag = Number(params.get("dailytime"));
    if (params.has("dailytime") && Number.isFinite(dailyTimeFlag)) snapshot.session.daily_time_s = Math.round(dailyTimeFlag * 60);
    const distanceFlag = Number(params.get("distance"));
    if (params.has("distance") && Number.isFinite(distanceFlag)) snapshot.session.distance_m = Math.round(distanceFlag * 1000);
    const deckNoteFlag = params.get("decknote");
    if (deckNoteFlag === "refused") {
      snapshot.deck.target_note = "DECK_REFUSED // 3.5 KM/H, TILT 6.3 %";
      snapshot.deck.target_note_hint = "The treadmill turned down 3.5 km/h. Pick another speed.";
    }
    if (deckNoteFlag === "adjusted") {
      snapshot.deck.target_note = "DECK_ADJUSTED // 0.6 KM/H -> 0.8 KM/H";
      snapshot.deck.target_note_hint = "The treadmill took 0.8 km/h in place of 0.6 km/h. 0.8 km/h is the treadmill's slowest speed.";
    }
  },
  commands: {
    // Engage / Stasis: a still belt starts (or a paused program goes), a moving one pauses.
    toggle_start: () => {
      const a = snapshot.active_program;
      if (a?.paused) return setProgramPaused(false);
      const moving = snapshot.deck.status === "Running" || snapshot.deck.status === "Starting";
      Object.assign(snapshot.deck, moving ? { status: "Paused", speed_kmh: 0 } : { status: "Running", speed_kmh: snapshot.deck.target_speed_kmh });
    },
  },
};
