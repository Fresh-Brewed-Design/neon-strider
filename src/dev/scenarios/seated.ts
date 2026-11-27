// ?seated shows the sit-too-long HUD card (ATROPHY_WARNING). Variants, comma-separated: off (treadmill off,
// nothing picked yet), queued (treadmill off, second program remembered), long (long
// program names, to check wrapping), none / one (no or one saved program), many (six
// saved programs with long names; the card shows the first three), folded (shown once
// and left a minute: the seated time on the strip, #422), snoozed (two snoozes taken,
// so the next is 60 MIN).
import { params, snapshot } from "../mockState";
import { foldSeated } from "../cardQueue";
import { refuseWhileStopping } from "./engage";
import { startProgram } from "./programs";
import type { Scenario } from "./types";

/** The programs the card offers, as Rust resolves them. */
let seatedChosen: number[] | null = null;
function resolveSeatedPrograms() {
  const existing = snapshot.programs.map((p) => p.id);
  const picked = (seatedChosen ?? []).filter((id) => existing.includes(id)).slice(0, 3);
  snapshot.seated.program_ids = picked.length > 0 ? picked : existing.slice(0, 3);
  snapshot.seated.custom_programs = seatedChosen !== null;
}

export const seated: Scenario = {
  apply() {
    const seatedFlags = new Set((params.get("seated") ?? "").split(","));
    if (params.has("seated")) {
      Object.assign(snapshot.seated, { show_card: true, seated_minutes: 63 });
      if (seatedFlags.has("off") || seatedFlags.has("queued")) snapshot.connection.connected = false;
      if (seatedFlags.has("queued")) snapshot.seated.pending_program_id = 2;
      if (seatedFlags.has("long")) {
        ["Long afternoon recovery walk", "Standing desk intervals", "Post-lunch"].forEach((name, i) => {
          snapshot.programs[i].name = name;
        });
      }
      if (seatedFlags.has("folded")) foldSeated();
      if (seatedFlags.has("snoozed")) snapshot.seated.snooze_minutes = 60;
      if (seatedFlags.has("none")) snapshot.programs = [];
      if (seatedFlags.has("one")) snapshot.programs = snapshot.programs.slice(0, 1);
      if (seatedFlags.has("many")) {
        const names = ["Long afternoon recovery walk", "Standing desk intervals", "Post-lunch digestion stroll", "Hill", "Endurance base builder", "Wind-down"];
        snapshot.programs = names.map((name, i) => ({ ...snapshot.programs[i % snapshot.programs.length], id: i + 1, name }));
      }
    }
    resolveSeatedPrograms();
  },
  commands: {
    seated_set_config: (c) => {
      seatedChosen = c.program_ids && c.program_ids.length > 0 ? c.program_ids : null;
      Object.assign(snapshot.seated, { enabled: c.enabled, minutes: c.minutes });
      resolveSeatedPrograms();
    },
    seated_start: (c) => {
      const p = snapshot.programs.find((x) => x.id === c.id);
      if (p && snapshot.connection.connected) {
        snapshot.seated.show_card = false;
        snapshot.active_program = startProgram(p);
      } else if (p) {
        snapshot.seated.pending_program_id = p.id;
      }
    },
    seated_engage: () => {
      if (!snapshot.connection.connected) throw "The treadmill is offline";
      if (snapshot.deck.standby) return refuseWhileStopping();
      Object.assign(snapshot.seated, { show_card: false, pending_program_id: null });
      Object.assign(snapshot.deck, { status: "Running", speed_kmh: snapshot.deck.target_speed_kmh });
    },
    seated_snooze: () =>
      Object.assign(snapshot.seated, {
        show_card: false,
        pending_program_id: null,
        // Rust's 15, 30, then 60.
        snooze_minutes: Math.min(snapshot.seated.snooze_minutes * 2, 60),
      }),
  },
};
