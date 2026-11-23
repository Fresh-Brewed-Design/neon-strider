import { describe, expect, it } from "vitest";
import type { WristLinkSnapshot } from "../../ipc/bindings/WristLinkSnapshot";
import { wristLinkTone } from "./wristLink";

const wrist = (over: Partial<WristLinkSnapshot>): WristLinkSnapshot => ({
  enabled: true,
  serving: true,
  watches: 0,
  failed: false,
  message: "Waiting for the watch's data field to connect.",
  ...over,
});

describe("wristLinkTone", () => {
  it("colours WRIST_LINK by whether a field is connected", () => {
    expect(wristLinkTone(wrist({ enabled: false, serving: false }))).toBe("idle");
    expect(wristLinkTone(wrist({}))).toBe("working");
    expect(wristLinkTone(wrist({ watches: 1 }))).toBe("ok");
    expect(wristLinkTone(wrist({ serving: false, message: "Starting the link." }))).toBe("working");
    expect(wristLinkTone(wrist({ serving: false, failed: true }))).toBe("problem");
  });
});
