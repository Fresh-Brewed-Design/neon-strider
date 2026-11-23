import { describe, expect, it } from "vitest";
import { bandWith } from "./speedDrift";

describe("speed drift band fields", () => {
  const band = { low: 3.2, high: 3.8 };

  it("keeps a typed end and the other end when the band stays valid", () => {
    expect(bandWith(band, "low", 3.4)).toEqual({ low: 3.4, high: 3.8 });
    expect(bandWith(band, "high", 4.0)).toEqual({ low: 3.2, high: 4.0 });
  });

  it("moves the other end to keep the band at least 0.2 km/h wide", () => {
    expect(bandWith(band, "low", 3.9)).toEqual({ low: 3.9, high: 4.1 });
    expect(bandWith(band, "high", 3.1)).toEqual({ low: 2.9, high: 3.1 });
  });

  it("moves the other end to keep the band at most 1.5 km/h wide", () => {
    expect(bandWith(band, "low", 2.0)).toEqual({ low: 2.0, high: 3.5 });
    expect(bandWith(band, "high", 6.0)).toEqual({ low: 4.5, high: 6.0 });
  });

  it("holds both ends inside Rust's range", () => {
    expect(bandWith(band, "high", 0.5)).toEqual({ low: 0.5, high: 0.7 });
    expect(bandWith(band, "low", 12)).toEqual({ low: 11.8, high: 12 });
  });
});
