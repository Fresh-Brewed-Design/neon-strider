// ?easystart=off: Easy start switched off (System -> Treadmill).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const easyStart: Scenario = {
  apply() {
    if (params.get("easystart") === "off") snapshot.easy_start.enabled = false;
  },
  commands: {
    easy_start_set: (c) => (snapshot.easy_start.enabled = c.on),
  },
};
