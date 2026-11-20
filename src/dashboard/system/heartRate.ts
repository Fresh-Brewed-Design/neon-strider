import type { HeartRateSnapshot } from "../../ipc/bindings/HeartRateSnapshot";
import type { StatusTone } from "../../state/linkStatus";

/** The pill tone for HEART_LINK's status: cyan linked, yellow busy, red fault. */
export function heartRateTone(h: HeartRateSnapshot): StatusTone {
  switch (h.status) {
    case "connected":
      return "ok";
    case "searching":
      return "working";
    case "error":
      return "problem";
    default:
      return "idle";
  }
}

/** `HEART_LINK // LINKED`, from Rust's deck-voice label. */
export function heartRatePill(h: HeartRateSnapshot): string {
  return `HEART_LINK // ${h.label}`;
}
