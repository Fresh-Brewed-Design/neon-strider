// WALL_CLOCK (History's card). ?wallclock=new: no desk days yet.
// ?wallclock=late: a walk at 21:00 on one day, so the row widens past 20:00.
// ?wallclock=none: desk days but no walks.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const wallClock: Scenario = {
  apply() {
    const flag = params.get("wallclock");
    if (flag === "new") {
      Object.assign(snapshot.wall_clock, { hours: Array<number>(24).fill(0), desk_days: 0 });
    } else if (flag === "late") {
      snapshot.wall_clock.hours[21] = 1;
      snapshot.wall_clock.last_hour = 21;
    } else if (flag === "none") {
      Object.assign(snapshot.wall_clock, { hours: Array<number>(24).fill(0), desk_days: 6 });
    }
  },
};
