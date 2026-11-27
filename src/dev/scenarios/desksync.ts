// DESK_SYNC (History's card). ?desksync=open: today is a desk day not walked
// yet (left out of the count). ?desksync=new: nothing recorded yet.
// ?desksync=few: only three desk days so far. ?desksync=counted: today open,
// then three seconds after load the walk counts and today's light pulses on
// (`today_lit`, cleared by the card's `desk_sync_light_shown`).
import type { DeskSync } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Seconds after load before today counts: open History first to see it. */
const COUNT_AFTER_S = 3;

let elapsed = 0;
let counted: DeskSync | null = null;

function openToday() {
  const days = snapshot.desk_sync.days.slice(0, -1);
  Object.assign(snapshot.desk_sync, { days, walked: days.filter((x) => x.walked).length, desk_days: days.length, today_open: true });
}

export const deskSync: Scenario = {
  apply() {
    const deskSyncFlag = params.get("desksync");
    if (deskSyncFlag === "open") {
      openToday();
    } else if (deskSyncFlag === "counted") {
      counted = { ...snapshot.desk_sync, today_lit: true };
      openToday();
    } else if (deskSyncFlag === "new") {
      Object.assign(snapshot.desk_sync, { days: [], walked: 0, desk_days: 0, today_open: false });
    } else if (deskSyncFlag === "few") {
      const days = snapshot.desk_sync.days.slice(-3);
      Object.assign(snapshot.desk_sync, { days, walked: days.filter((x) => x.walked).length, desk_days: days.length });
    }
  },
  commands: {
    desk_sync_light_shown: () => (snapshot.desk_sync.today_lit = false),
  },
  tick() {
    if (!counted) return false;
    elapsed += 1;
    if (elapsed < COUNT_AFTER_S) return false;
    Object.assign(snapshot.desk_sync, counted);
    counted = null;
    return true;
  },
};
