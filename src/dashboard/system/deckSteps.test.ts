import { describe, expect, it } from "vitest";
import { deckStepsText } from "./deckSteps";

describe("deckStepsText", () => {
  it("says when there is no count", () => {
    expect(deckStepsText({ count: null, moves: 0, live: false, still_s: null })).toBe(
      "No step count from the treadmill yet.",
    );
  });

  it("reads a count that moves while walking as moving", () => {
    expect(deckStepsText({ count: 120, moves: 30, live: true, still_s: 2 })).toBe("Deck step count 120, moving.");
  });

  it("counts the seconds a count has been still", () => {
    expect(deckStepsText({ count: 120, moves: 30, live: true, still_s: 38 })).toBe("Deck step count 120, still for 38 s.");
    // A deck that sends 0 the whole walk never reads as moving.
    expect(deckStepsText({ count: 0, moves: 0, live: false, still_s: 1 })).toBe("Deck step count 0, still for 1 s.");
  });

  it("says how often it moved once the belt is still", () => {
    expect(deckStepsText({ count: 900, moves: 600, live: true, still_s: null })).toBe(
      "Deck step count 900, moved 600 times while walking.",
    );
    expect(deckStepsText({ count: 0, moves: 0, live: false, still_s: null })).toBe("Deck step count 0, not moved while walking.");
  });
});
