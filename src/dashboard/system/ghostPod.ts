import type { GhostPodSnapshot } from "../../ipc/bindings/GhostPodSnapshot";
import type { StatusTone } from "../../state/linkStatus";

/** The pill tone for GHOST_POD's status: cyan linked, yellow busy, red fault. */
export function ghostPodTone(p: GhostPodSnapshot): StatusTone {
  switch (p.status) {
    case "connected":
      return "ok";
    case "starting":
    case "advertising":
      return "working";
    case "error":
      return "problem";
    default:
      return "idle";
  }
}

/** `GHOST_POD // LINKED`, from Rust's deck-voice label. */
export function ghostPodPill(p: GhostPodSnapshot): string {
  return `GHOST_POD // ${p.label}`;
}

/** What the pod sends now, while a watch reads it. */
export function ghostPodReading(p: GhostPodSnapshot): string | null {
  if (p.status !== "connected") return null;
  return `Sending ${p.speed_kmh.toFixed(1)} km/h`;
}

/** What the power switch reports: a watch reading the watts, or waiting for one. */
export function ghostPodPowerReading(p: GhostPodSnapshot): string | null {
  if (!p.enabled || !p.power_enabled || p.status === "error") return null;
  if (p.power_watches > 0) return `A watch reads power: sending ${p.power_w} W.`;
  return "No watch reads power yet: add a power sensor on the watch.";
}
