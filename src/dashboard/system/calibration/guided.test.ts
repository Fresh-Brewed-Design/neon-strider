import { describe, expect, it } from "vitest";
import { calibrationRows, chrono, deckVerdict, droppedLine, listedTarget, runRowFor } from "./guided";

const point = (speed: number) => ({ speed, stride_length_cm: 70, distance_scale: 0.9 });

describe("calibrationRows", () => {
  it("offers every target and marks the ones already measured", () => {
    const rows = calibrationRows([point(3.0), point(4.5)], [2, 3, 4]);
    expect(rows.map((r) => [r.targetKmh, r.index])).toEqual([
      [2, null],
      [3, 0],
      [4, null],
      [4.5, 1],
    ]);
  });

  it("treats a point within 0.05 km/h as that target's", () => {
    const rows = calibrationRows([point(2.96)], [3]);
    expect(rows).toHaveLength(1);
    expect(rows[0].index).toBe(0);
  });
});

describe("chrono", () => {
  it("shows minutes, seconds and tenths", () => {
    expect(chrono(0)).toBe("0:00.0");
    expect(chrono(7.25)).toBe("0:07.3");
    expect(chrono(83.4)).toBe("1:23.4");
    expect(chrono(59.96)).toBe("1:00.0");
  });
});

describe("runRowFor", () => {
  it("finds the row of a run's target, saved point or not", () => {
    const rows = calibrationRows([point(2.96)], [3, 4]);
    expect(runRowFor(rows, 2.96)?.index).toBe(0);
    expect(runRowFor(rows, 4)?.targetKmh).toBe(4);
    expect(runRowFor(rows, 5)).toBeNull();
  });
});

describe("listedTarget", () => {
  it("finds the list entry for a row's speed", () => {
    const targets = [{ kmh: 2, runnable: true }, { kmh: 14, runnable: false }];
    expect(listedTarget(targets, 14.02)).toEqual({ kmh: 14, runnable: false });
    expect(listedTarget(targets, 3)).toBeNull();
  });
});

describe("deckVerdict", () => {
  it("says how far the deck is from the belt, in plain words", () => {
    expect(deckVerdict(3, 2.82, 3)).toBe("At 3.0 km/h the deck runs 6.0 % slow.");
    expect(deckVerdict(4, 4.6, 4)).toBe("At 4.0 km/h the deck runs 15 % fast.");
    expect(deckVerdict(3, 3.01, 3)).toBe("At 3.0 km/h the deck matches the belt.");
    expect(deckVerdict(3, 2.82, 3, "saved")).toBe("At 3.0 km/h the deck runs 6.0 % slow; saved.");
    expect(deckVerdict(3, 3.01, 3, "undone")).toBe("At 3.0 km/h the deck matches the belt; undone.");
  });
});

describe("droppedLine", () => {
  it("names dropped taps only when there are some", () => {
    expect(droppedLine(0)).toBeNull();
    expect(droppedLine(1)).toBe("1 tap dropped as a miss or a double.");
    expect(droppedLine(3)).toBe("3 taps dropped as misses or doubles.");
  });
});
