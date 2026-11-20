// The Diagnostics traffic recorder. ?traffic=recording: a recording 42 s in
// (the packet count ticks). ?traffic=saved: one saved after reaching the
// 4-hour limit. Switching it on in the preview starts a fresh one.
import { params, publish, snapshot } from "../mockState";
import type { Scenario } from "./types";

const TRAFFIC_DIR = "C:/Users/you/AppData/Local/com.freshbreweddesign.neonstrider/logs/traffic";
let trafficTimer: ReturnType<typeof setInterval> | null = null;
function startTraffic(elapsed_s: number) {
  const file_name = "traffic-2026-11-15_09-30-12.jsonl";
  const packets = Math.round(elapsed_s * 3.3);
  Object.assign(snapshot.traffic, {
    recording: true,
    file_name,
    file_path: `${TRAFFIC_DIR}/${file_name}`,
    packets,
    bytes: packets * 105,
    elapsed_s,
    stop_reason: null,
  });
  if (trafficTimer) clearInterval(trafficTimer);
  trafficTimer = setInterval(() => {
    const t = snapshot.traffic;
    Object.assign(t, { elapsed_s: t.elapsed_s + 1, packets: t.packets + 3, bytes: t.bytes + 315 });
    publish();
  }, 1000);
}
function stopTraffic(stop_reason: string | null = null) {
  if (trafficTimer) clearInterval(trafficTimer);
  trafficTimer = null;
  Object.assign(snapshot.traffic, { recording: false, stop_reason });
}

export const traffic: Scenario = {
  apply() {
    const trafficFlag = params.get("traffic");
    if (trafficFlag === "recording") startTraffic(42);
    if (trafficFlag === "saved") {
      startTraffic(4 * 3600);
      stopTraffic("Stopped by itself at the 4-hour limit.");
    }
  },
  commands: {
    traffic_record: (c) => (c.on ? startTraffic(0) : stopTraffic()),
    traffic_open_folder: () => {},
  },
};
