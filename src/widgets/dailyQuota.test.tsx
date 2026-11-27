// The testing library reads a no-break space as a plain space, so "/ 5.0" finds the target text.
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot, DailyQuotaSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { statWidget } = await import("./registry");
const { QuotaBar } = await import("./QuotaBar");
const { StatCell } = await import("../hud/HudPanel");
const { StripRow } = await import("../hud/HudStrip");
const { WalkReadout } = await import("../dashboard/live/WalkReadout");

const QUOTA: DailyQuotaSnapshot = {
  target_km: 5,
  fraction: 0.862,
  left_km: 0.69,
  message: "0.69 km left of today's 5.0 km target.",
};
const OFF: DailyQuotaSnapshot = { target_km: null, fraction: 0, left_km: 0, message: "" };

function snap(quota: DailyQuotaSnapshot, still = false): AppSnapshot {
  const s = structuredClone(mock);
  s.session.daily_distance_m = 4310;
  s.daily_quota = quota;
  if (still) Object.assign(s.deck, { status: "Stopped", speed_kmh: 0 });
  s.active_program = null;
  return s;
}

describe("DAILY_QUOTA on DAILY_TOTAL", () => {
  it("reads the target after the km only while one is set", () => {
    const w = statWidget("DAILY")!;
    expect(w.read(snap(OFF))).toEqual({ value: "4.31", unit: "KM" });
    expect(w.read(snap(QUOTA))).toEqual({ value: "4.31", of: "/ 5.0", unit: "KM" });
    expect(w.quota?.(snap(OFF))).toBeNull();
    expect(w.quota?.(snap(QUOTA))).toEqual(QUOTA);
  });

  it("draws the bar at Rust's share with Rust's sentence", () => {
    render(<QuotaBar quota={QUOTA} />);
    const bar = screen.getByRole("progressbar", { name: "DAILY_QUOTA: daily km target" });
    expect(bar).toHaveAttribute("aria-valuenow", "86.2");
    expect(bar).toHaveAttribute("title", "0.69 km left of today's 5.0 km target.");
    expect(screen.getByTestId("quota-fill").style.width).toBe("86.2%");
  });

  it("stays full past the target, with nothing more", () => {
    render(<QuotaBar quota={{ target_km: 5, fraction: 1, left_km: 0, message: "Today's 5.0 km target is walked." }} />);
    expect(screen.getByTestId("quota-fill").style.width).toBe("100%");
    expect(screen.getByRole("progressbar")).toHaveAttribute("title", "Today's 5.0 km target is walked.");
  });

  it("draws nothing with no target", () => {
    const { container } = render(<QuotaBar quota={OFF} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows on the HUD panel's TODAY with the target, and not without one", () => {
    const { unmount } = render(<StatCell id="DAILY" snapshot={snap(QUOTA)} />);
    expect(screen.getByTestId("quota-bar")).toBeInTheDocument();
    expect(screen.getByText("/ 5.0")).toHaveClass("hud-quota-of");
    unmount();
    render(<StatCell id="DAILY" snapshot={snap(OFF)} />);
    expect(screen.queryByTestId("quota-bar")).toBeNull();
    expect(screen.queryByText("/ 5.0")).toBeNull();
  });

  it("shows on the strip's TODAY beside Engage", () => {
    render(<StripRow snapshot={snap(QUOTA, true)} send={async () => true} note={null} />);
    expect(screen.getByTestId("quota-bar")).toBeInTheDocument();
    expect(screen.getByText("/ 5.0")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ENGAGE/ })).toBeInTheDocument();
  });

  it("shows under STEP_MAP's TODAY", () => {
    render(<WalkReadout s={snap(QUOTA)} />);
    expect(screen.getByTestId("quota-bar")).toBeInTheDocument();
    expect(screen.getByText("/ 5.0")).toBeInTheDocument();
  });
});
