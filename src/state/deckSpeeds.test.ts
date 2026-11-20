import { describe, expect, it } from "vitest";
import type { DeckProfileSnapshot, DeckSpeedStep } from "../ipc";
import {
  deckLimitOffer,
  inclineSummary,
  limitsText,
  profileSummary,
  roundSpeed,
  snapToDeck,
  snapToLevel,
} from "./deckSpeeds";

const none: DeckProfileSnapshot = {
  learned: 0,
  floor_kmh: null,
  ceiling_kmh: null,
  floor_hw_kmh: null,
  ceiling_hw_kmh: null,
  skipped_kmh: [],
  learned_levels: 0,
  ceiling_level: null,
  skipped_levels: [],
  sweep: null,
  sweep_enabled: false,
};

// Bytes 0.6..2.0 without 1.5, calibrated 10 % slower than the deck says.
const steps: DeckSpeedStep[] = [];
for (let b = 6; b <= 20; b++) if (b !== 15) steps.push({ hw_kmh: b / 10, kmh: (b / 10) * 0.9 });

describe("snapToDeck", () => {
  it("leaves every value alone before anything is learned", () => {
    const deck = { speed_steps: [], profile: none };
    expect(snapToDeck(1.37, deck)).toBe(1.37);
    expect(snapToDeck(9, deck, "hw_kmh")).toBe(9);
    expect(snapToDeck(1.37, null)).toBe(1.37);
  });

  it("moves a speed to the nearest one the deck runs, the slower on a tie", () => {
    const deck = { speed_steps: steps, profile: { ...none, learned: 3 } };
    expect(snapToDeck(1.5 * 0.9, deck)).toBeCloseTo(1.4 * 0.9);
    expect(snapToDeck(1.5, deck, "hw_kmh")).toBeCloseTo(1.4);
    expect(snapToDeck(0.2, deck)).toBeCloseTo(0.54, 5);
    expect(snapToDeck(0, deck)).toBe(0);
  });

  it("lets the top speed rise past the list until the ceiling is known", () => {
    const deck = { speed_steps: steps, profile: { ...none, learned: 3 } };
    expect(snapToDeck(5, deck, "hw_kmh")).toBe(5);
    const capped = { speed_steps: steps, profile: { ...none, learned: 3, ceiling_kmh: 1.8 } };
    expect(snapToDeck(5, capped, "hw_kmh")).toBeCloseTo(2.0);
  });

  it("rounds a snapped speed to 0.01 km/h", () => {
    expect(roundSpeed(1.2345)).toBe(1.23);
  });
});

describe("deckLimitOffer", () => {
  const known = { ...none, learned: 9, floor_hw_kmh: 0.6, ceiling_hw_kmh: 4 };

  it("offers the deck's limits over looser settings", () => {
    expect(deckLimitOffer(known, 0.3, 12)).toEqual({ minSpeed: 0.6, maxSpeed: 4 });
  });

  it("leaves settings that match or that you tightened", () => {
    expect(deckLimitOffer(known, 0.6, 4)).toBeNull();
    expect(deckLimitOffer(known, 1, 3)).toBeNull();
    expect(deckLimitOffer(known, 1, 12)).toEqual({ maxSpeed: 4 });
  });

  it("offers nothing before the limits are seen", () => {
    expect(deckLimitOffer(none, 0.6, 12)).toBeNull();
  });
});

describe("limitsText", () => {
  it("names the limits in the deck's own km/h, the settings' unit", () => {
    expect(limitsText({ ...none, floor_kmh: 0.5, ceiling_kmh: 3.6, floor_hw_kmh: 0.6, ceiling_hw_kmh: 4 })).toBe(
      "0.6 to 4.0 km/h",
    );
    expect(limitsText({ ...none, ceiling_hw_kmh: 4 })).toBe("no faster than 4.0 km/h");
  });
});

describe("profileSummary", () => {
  it("names the limits and skipped speeds", () => {
    expect(profileSummary(none)).toBe("No speeds learned yet.");
    expect(profileSummary({ ...none, learned: 9, floor_kmh: 0.6, ceiling_kmh: 4, skipped_kmh: [0.3, 1.5] })).toBe(
      "9 speeds learned, slowest 0.6 km/h, fastest 4.0 km/h, skips 0.3, 1.5.",
    );
  });
});

describe("snapToLevel", () => {
  const deck = { incline_steps: [0, 1, 2, 3, 5, 6] };

  it("leaves every level alone before anything is learned", () => {
    expect(snapToLevel(4, { incline_steps: [] })).toBe(4);
    expect(snapToLevel(4, null)).toBe(4);
  });

  it("moves a level to the nearest one the deck runs, the lower on a tie", () => {
    expect(snapToLevel(4, deck)).toBe(3);
    expect(snapToLevel(5, deck)).toBe(5);
    expect(snapToLevel(0, deck)).toBe(0);
  });

  it("leaves a level above the list as typed", () => {
    expect(snapToLevel(12, deck)).toBe(12);
  });
});

describe("inclineSummary", () => {
  it("says what the deck taught", () => {
    expect(inclineSummary(none)).toBe("No incline levels learned yet.");
    expect(inclineSummary({ ...none, learned_levels: 4, ceiling_level: 12, skipped_levels: [7, 15] })).toBe(
      "4 incline levels learned, top level 12, skips 7, 15.",
    );
  });
});
