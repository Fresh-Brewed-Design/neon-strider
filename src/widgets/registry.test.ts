import { describe, expect, it } from "vitest";
import { snapshot } from "../dev/mockBackend";
import { lifetimeKm } from "../format";
import { statWidget, widgetName } from "./registry";

describe("ODOMETER widget", () => {
  it("is a stat widget with the ODO label and LIFETIME_ODOMETER codename", () => {
    const w = statWidget("ODOMETER");
    expect(w?.label).toBe("ODO");
    expect(w?.codename).toBe("LIFETIME_ODOMETER");
    expect(widgetName("ODOMETER")).toBe("LIFETIME_ODOMETER");
  });

  it("reads the lifetime distance in km from the snapshot", () => {
    const reading = statWidget("ODOMETER")?.read({
      ...snapshot,
      odometer: { distance_km: 412.64, belt_hours: 138.2, since_ms: null },
    });
    expect(reading).toEqual({ value: "412.6", unit: "KM" });
  });

  it("drops the decimal from 1000 km so the HUD cell stays narrow", () => {
    expect(lifetimeKm(0)).toBe("0.0");
    expect(lifetimeKm(999.94)).toBe("999.9");
    expect(lifetimeKm(1000)).toBe("1000");
    expect(lifetimeKm(12345.6)).toBe("12346");
  });
});
