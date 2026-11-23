// PC time walked (the WALKED stat). ?pcwalk pins it on the HUD with 2h 10m
// walked of 5h 40m at the PC (38 %); ?pcwalk=early has 6 minutes of PC time,
// too few for a share (--); ?pcwalk=none has no PC time today; ?pcwalk=long
// is a long day, 10h 05m walked of 11h 50m (85 %), for the widest line.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const pcWalkShare: Scenario = {
  apply() {
    if (!params.has("pcwalk")) return;
    // Pinned on the HUD after DAILY_TOTAL, so the two daily stats sit together.
    const w = snapshot.hud.widgets;
    if (!w.includes("PC_WALK_SHARE")) w.splice(w.indexOf("DAILY") + 1, 0, "PC_WALK_SHARE");
    const flag = params.get("pcwalk");
    if (flag === "early") snapshot.pc_walk_share = { active_s: 360, walked_s: 240, share_pct: null };
    else if (flag === "long") snapshot.pc_walk_share = { active_s: 42_600, walked_s: 36_300, share_pct: 85 };
    else if (flag === "none") snapshot.pc_walk_share = { active_s: 0, walked_s: 0, share_pct: null };
  },
};
