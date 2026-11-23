import { describe, expect, it } from "vitest";
import type { TypingSpeedSnapshot } from "../../ipc/bindings/TypingSpeedSnapshot";
import { bandLabel, barShares, typingSpeedLines } from "./typingSpeedText";

const band = (from_kmh: number, median_kpm: number) => ({ from_kmh, to_kmh: from_kmh + 0.5, minutes: 6, median_kpm });
const snap = (over: Partial<TypingSpeedSnapshot>): TypingSpeedSnapshot => ({
  enabled: true,
  bands: [],
  walks: 0,
  typing_minutes: 0,
  min_minutes: 5,
  recent_walks: 20,
  ...over,
});

describe("typing by speed text", () => {
  it("labels a band by both ends", () => {
    expect(bandLabel(band(3, 80))).toBe("3.0 to 3.5 km/h");
  });

  it("scales bars to the highest median", () => {
    expect(barShares([band(2, 40), band(3, 80)])).toEqual([0.5, 1]);
    expect(barShares([band(2, 0)])).toEqual([0]);
  });

  it("says why the table is empty", () => {
    expect(typingSpeedLines(snap({}))).toEqual([
      "No walks with key counts saved yet.",
      "A speed shows once it has 5 minutes with typing.",
    ]);
    expect(typingSpeedLines(snap({ walks: 1, typing_minutes: 3 }))[1]).toBe("Key counts so far come from 1 walk.");
  });

  it("names the walks the medians come from", () => {
    expect(typingSpeedLines(snap({ walks: 12, bands: [band(3, 80)] }))[0]).toBe(
      "Medians over your last 12 walks with key counts.",
    );
  });
});
