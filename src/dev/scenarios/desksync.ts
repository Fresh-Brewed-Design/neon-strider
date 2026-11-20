// DESK_SYNC (History's card). ?desksync=open: today is a desk day not walked
// yet (left out of the count). ?desksync=new: nothing recorded yet.
// ?desksync=few: only three desk days so far.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const deskSync: Scenario = {
  apply() {
    const deskSyncFlag = params.get("desksync");
    if (deskSyncFlag === "open") {
      const days = snapshot.desk_sync.days.slice(0, -1);
      Object.assign(snapshot.desk_sync, { days, walked: days.filter((x) => x.walked).length, desk_days: days.length, today_open: true });
    } else if (deskSyncFlag === "new") {
      Object.assign(snapshot.desk_sync, { days: [], walked: 0, desk_days: 0, today_open: false });
    } else if (deskSyncFlag === "few") {
      const days = snapshot.desk_sync.days.slice(-3);
      Object.assign(snapshot.desk_sync, { days, walked: days.filter((x) => x.walked).length, desk_days: days.length });
    }
  },
};
