import { describe, expect, it } from "vitest";
import type { GhostPodSnapshot } from "../../ipc/bindings/GhostPodSnapshot";
import { ghostPodPill, ghostPodReading, ghostPodTone, wristLinkTone } from "./ghostPod";

const pod = (over: Partial<GhostPodSnapshot>): GhostPodSnapshot => ({
  enabled: true,
  status: "advertising",
  label: "BROADCASTING",
  message: "Waiting for a watch.",
  watches: 0,
  speed_kmh: 0,
  wrist_link: { enabled: false, serving: false, watches: 0, message: "" },
  ...over,
});

describe("ghostPod", () => {
  it("colours the pill by status", () => {
    expect(ghostPodTone(pod({ status: "off" }))).toBe("idle");
    expect(ghostPodTone(pod({ status: "starting" }))).toBe("working");
    expect(ghostPodTone(pod({ status: "advertising" }))).toBe("working");
    expect(ghostPodTone(pod({ status: "connected" }))).toBe("ok");
    expect(ghostPodTone(pod({ status: "error" }))).toBe("problem");
  });

  it("colours WRIST_LINK by whether a field is connected", () => {
    const wrist = (enabled: boolean, watches: number) => ({ enabled, serving: enabled, watches, message: "" });
    expect(wristLinkTone(pod({ wrist_link: wrist(false, 0) }))).toBe("idle");
    expect(wristLinkTone(pod({ wrist_link: wrist(true, 0) }))).toBe("working");
    expect(wristLinkTone(pod({ wrist_link: wrist(true, 1) }))).toBe("ok");
    expect(wristLinkTone(pod({ status: "error", wrist_link: wrist(true, 0) }))).toBe("problem");
  });

  it("labels the pill in the deck's voice", () => {
    expect(ghostPodPill(pod({ label: "LINKED" }))).toBe("GHOST_POD // LINKED");
  });

  it("shows the speed sent only while a watch reads it", () => {
    expect(ghostPodReading(pod({ status: "connected", speed_kmh: 4.5 }))).toBe("Sending 4.5 km/h");
    expect(ghostPodReading(pod({ status: "advertising", speed_kmh: 4.5 }))).toBeNull();
  });
});
