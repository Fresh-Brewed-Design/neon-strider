// Labs: GHOST_POD. ?ghostpod=advertising|connected|error picks its status;
// switching it on in the preview advertises, then a watch connects 3 s later.
// ?ghostpower=waiting|linked turns its power switch on, with no watch reading
// power yet or a watch getting 135 W.
import { params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

const GHOST_POD: Record<"off" | "starting" | "advertising" | "connected" | "error", [string, string]> = {
  off: ["OFFLINE", "Switch GHOST_POD on to let a watch read the belt as a foot pod."],
  starting: ["BOOTING", "Starting the pod."],
  advertising: ["BROADCASTING", "Waiting for a watch. Add a foot pod sensor on it; the pod shows under this PC's Bluetooth name."],
  connected: ["LINKED", "A watch is connected and gets the belt speed every second."],
  error: ["FAULT", "This PC's Bluetooth adapter can't act as a sensor (no peripheral role)."],
};
let ghostPodTimer: ReturnType<typeof setTimeout> | null = null;
function setGhostPod(status: keyof typeof GHOST_POD) {
  const [label, message] = GHOST_POD[status];
  Object.assign(snapshot.ghost_pod, {
    enabled: status !== "off",
    status,
    label,
    message,
    watches: status === "connected" ? 1 : 0,
    speed_kmh: status === "connected" ? snapshot.deck.speed_kmh : 0,
  });
  setGhostPower(snapshot.ghost_pod.power_enabled);
}
function setGhostPower(on: boolean, linked = snapshot.ghost_pod.status === "connected") {
  const reading = on && snapshot.ghost_pod.enabled && linked;
  Object.assign(snapshot.ghost_pod, {
    power_enabled: on,
    power_watches: reading ? 1 : 0,
    power_w: reading ? 135 : 0,
  });
}
function switchGhostPod(on: boolean) {
  if (ghostPodTimer) clearTimeout(ghostPodTimer);
  ghostPodTimer = null;
  setGhostPod(on ? "advertising" : "off");
  if (on) {
    ghostPodTimer = setTimeout(() => {
      setGhostPod("connected");
      publish();
    }, 3000);
  }
}
// Labs: WRIST_LINK. ?wristlink=waiting|connected shows it on (with GHOST_POD on);
// switching it on in the preview waits for the field, which connects 3 s later.
const WRIST_LINK_MESSAGES = {
  off: "Switch WRIST_LINK on to let the watch's data field read the belt.",
  needpod: "Waiting for GHOST_POD: the watch keeps its foot pod link while the field reads the belt.",
  waiting: "Waiting for the watch's data field to connect.",
  connected: "The watch's data field is connected and reads the belt.",
};
let wristLinkTimer: ReturnType<typeof setTimeout> | null = null;
function setWristLink(state: keyof typeof WRIST_LINK_MESSAGES) {
  Object.assign(snapshot.ghost_pod.wrist_link, {
    enabled: state !== "off",
    serving: state === "waiting" || state === "connected",
    watches: state === "connected" ? 1 : 0,
    message: WRIST_LINK_MESSAGES[state],
  });
}
function switchWristLink(on: boolean) {
  if (wristLinkTimer) clearTimeout(wristLinkTimer);
  wristLinkTimer = null;
  if (!on) return setWristLink("off");
  if (!snapshot.ghost_pod.enabled) return setWristLink("needpod");
  setWristLink("waiting");
  wristLinkTimer = setTimeout(() => {
    setWristLink("connected");
    publish();
  }, 3000);
}

export const ghostPod: Scenario = {
  apply() {
    const wristLinkFlag = params.get("wristlink");
    if (wristLinkFlag === "waiting" || wristLinkFlag === "connected") setWristLink(wristLinkFlag);

    const ghostPodFlag = params.get("ghostpod");
    if (ghostPodFlag && ghostPodFlag in GHOST_POD) setGhostPod(ghostPodFlag as keyof typeof GHOST_POD);
    const ghostPowerFlag = params.get("ghostpower");
    if (ghostPowerFlag === "waiting" || ghostPowerFlag === "linked") {
      if (!snapshot.ghost_pod.enabled) setGhostPod("connected");
      setGhostPower(true, ghostPowerFlag === "linked");
    }
  },
  commands: {
    ghost_pod_set: (c) => switchGhostPod(c.on),
    ghost_pod_power_set: (c) => setGhostPower(c.on),
    wrist_link_set: (c) => switchWristLink(c.on),
  },
};
