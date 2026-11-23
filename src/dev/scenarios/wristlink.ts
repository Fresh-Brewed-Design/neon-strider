// Labs: WRIST_LINK. ?wristlink=waiting|connected shows it on; switching it on
// in the preview waits for the field, which connects 3 s later.
import { params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

const WRIST_LINK_MESSAGES = {
  off: "Switch WRIST_LINK on to let the watch's data field read the belt.",
  waiting: "Waiting for the watch's data field to connect.",
  connected: "The watch's data field is connected and reads the belt.",
};
let wristLinkTimer: ReturnType<typeof setTimeout> | null = null;
function setWristLink(state: keyof typeof WRIST_LINK_MESSAGES) {
  Object.assign(snapshot.wrist_link, {
    enabled: state !== "off",
    serving: state !== "off",
    watches: state === "connected" ? 1 : 0,
    failed: false,
    message: WRIST_LINK_MESSAGES[state],
  });
}
function switchWristLink(on: boolean) {
  if (wristLinkTimer) clearTimeout(wristLinkTimer);
  wristLinkTimer = null;
  if (!on) return setWristLink("off");
  setWristLink("waiting");
  wristLinkTimer = setTimeout(() => {
    setWristLink("connected");
    publish();
  }, 3000);
}

export const wristLink: Scenario = {
  apply() {
    const wristLinkFlag = params.get("wristlink");
    if (wristLinkFlag === "waiting" || wristLinkFlag === "connected") setWristLink(wristLinkFlag);
  },
  commands: {
    wrist_link_set: (c) => switchWristLink(c.on),
  },
};
