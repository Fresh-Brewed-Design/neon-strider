import { describe, expect, it } from "vitest";
import { textScaleZoom } from "./textScale";

describe("?textscale=", () => {
  it("takes any Windows text size from 100 to 225 %", () => {
    expect(textScaleZoom("100")).toBe(1);
    expect(textScaleZoom("121")).toBe(1.21);
    expect(textScaleZoom("125")).toBe(1.25);
    expect(textScaleZoom("225")).toBe(2.25);
  });

  it("ignores values Windows doesn't offer", () => {
    expect(textScaleZoom(null)).toBeNull();
    expect(textScaleZoom("")).toBeNull();
    expect(textScaleZoom("99")).toBeNull();
    expect(textScaleZoom("226")).toBeNull();
    expect(textScaleZoom("121.5")).toBeNull();
    expect(textScaleZoom("big")).toBeNull();
  });
});
