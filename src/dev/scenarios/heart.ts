// HEART_LINK. ?heart=on picks a watch that reads about 112 bpm; scanning in
// the preview finds one after a moment. ?heart=waiting picks one that is not
// broadcasting yet. ?maxhr=178: a max heart rate of 178 bpm set by the user.
import { params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

const PREVIEW_WATCH = { id: "AA:BB:CC:DD:EE:01", name: "Forerunner 265" };
let heartTimer: ReturnType<typeof setInterval> | null = null;
function pickHeartSensor(sensor: { id: string; name: string } | null) {
  if (heartTimer) clearInterval(heartTimer);
  heartTimer = null;
  const h = snapshot.heart_rate;
  if (!sensor) {
    Object.assign(h, {
      sensor: null,
      status: "off",
      label: "OFFLINE",
      message: "Scan for a heart rate sensor and pick your watch to show heart rate.",
      bpm: null,
    });
    return;
  }
  if (params.get("heart") === "waiting") {
    Object.assign(h, {
      sensor,
      status: "waiting",
      label: "WAITING",
      message: "Waiting for heart rate. On a Garmin watch, start an activity or turn on Broadcast Heart Rate.",
      bpm: null,
    });
    return;
  }
  Object.assign(h, {
    sensor,
    status: "connected",
    label: "LINKED",
    message: "Reading heart rate from the sensor.",
    bpm: 112,
  });
  heartTimer = setInterval(() => {
    h.bpm = 108 + Math.round(Math.random() * 8);
    publish();
  }, 1000);
}

export const heartRate: Scenario = {
  apply() {
    const max = Number(params.get("maxhr"));
    if (params.has("maxhr") && Number.isFinite(max)) Object.assign(snapshot.heart_rate, { max_bpm: max, max_bpm_set: true });
    if (params.get("heart") === "on" || params.get("heart") === "waiting") pickHeartSensor(PREVIEW_WATCH);
    // ?zonetilt: System -> Labs ZONE_TILT on, so the editor offers incline trims.
    if (params.has("zonetilt")) snapshot.heart_rate.incline_trim = true;
  },
  commands: {
    heart_rate_scan: async () => {
      snapshot.heart_rate.scanning = true;
      publish();
      await new Promise((r) => setTimeout(r, 1500));
      snapshot.heart_rate.scanning = false;
      snapshot.heart_rate.found = [PREVIEW_WATCH];
    },
    heart_rate_pick: (c) => pickHeartSensor({ id: c.id, name: c.name }),
    heart_rate_forget: () => pickHeartSensor(null),
    heart_rate_max_set: (c) => {
      if (c.bpm < 120 || c.bpm > 230) throw new Error("Maximum heart rate must be between 120 and 230 bpm.");
      Object.assign(snapshot.heart_rate, { max_bpm: c.bpm, max_bpm_set: true });
    },
    heart_rate_incline_trim_set: (c) => {
      snapshot.heart_rate.incline_trim = c.on;
    },
  },
};
