import { describe, expect, it } from "vitest";
import type { HeartRateSnapshot } from "../../ipc/bindings/HeartRateSnapshot";
import { heartRatePill, heartRateTone } from "./heartRate";

const heart = (over: Partial<HeartRateSnapshot>): HeartRateSnapshot => ({
  sensor: { id: "AA:BB", name: "Forerunner 265" },
  status: "waiting",
  label: "WAITING",
  message: "Waiting for heart rate. On a Garmin watch, start an activity or turn on Broadcast Heart Rate.",
  bpm: null,
  history: [],
  zone: null,
  cool_down: null,
  scanning: false,
  found: [],
  max_bpm: 190,
  max_bpm_set: false,
  incline_trim: false,
  ...over,
});

describe("heartRatePill", () => {
  it("shows a sensor that is not broadcasting as waiting, not as a fault", () => {
    const pill = heartRatePill(heart({}));
    expect(pill).toBe("HEART_LINK // WAITING");
    expect(pill).not.toContain("FAULT");
  });

  it("follows the state, so a fault clears once the watch links", () => {
    const fault = heart({ status: "error", label: "FAULT", message: "Bluetooth is off. Turn it on in Windows settings." });
    expect(heartRatePill(fault)).toBe("HEART_LINK // FAULT");
    const linked = { ...fault, status: "connected" as const, label: "LINKED", message: "Reading heart rate from the sensor." };
    expect(heartRatePill(linked)).toBe("HEART_LINK // LINKED");
    expect(heartRateTone(linked)).toBe("ok");
  });
});

describe("heartRateTone", () => {
  it("is busy while waiting or seeking, and a problem only for a fault", () => {
    expect(heartRateTone(heart({ status: "waiting" }))).toBe("working");
    expect(heartRateTone(heart({ status: "searching" }))).toBe("working");
    expect(heartRateTone(heart({ status: "error" }))).toBe("problem");
    expect(heartRateTone(heart({ status: "off" }))).toBe("idle");
  });
});
