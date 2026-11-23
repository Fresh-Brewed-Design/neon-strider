import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot, SpeedDriftState } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { SpeedDriftLine } = await import("./HudPanel");

/** The mock's snapshot with speed drift on in Rust's `state`. */
function snap(state: SpeedDriftState): AppSnapshot {
  const s = structuredClone(mock);
  s.speed_drift = {
    ...s.speed_drift,
    enabled: true,
    state,
    speed_kmh: 3.6,
    message: "Speed drift has the belt at 3.6 km/h, within 3.2 to 3.8 km/h.",
  };
  return s;
}

describe("speed drift HUD line", () => {
  it("shows the band while speed drift sets the speed, with Rust's sentence as the tooltip", () => {
    render(<SpeedDriftLine snapshot={snap("drifting")} />);
    const line = screen.getByTestId("hud-drift");
    expect(line).toHaveTextContent("SPEED DRIFT");
    expect(line).toHaveTextContent("3.2 to 3.8km/h");
    expect(line).toHaveAttribute("title", "Speed drift has the belt at 3.6 km/h, within 3.2 to 3.8 km/h.");
  });

  it("stays while held for Neural flow", () => {
    render(<SpeedDriftLine snapshot={snap("held")} />);
    expect(screen.getByTestId("hud-drift")).toBeInTheDocument();
  });

  it("stays hidden in every other state", () => {
    for (const state of ["off", "idle", "outside", "no_room", "waiting", "ended"] as const) {
      const { unmount } = render(<SpeedDriftLine snapshot={snap(state)} />);
      expect(screen.queryByTestId("hud-drift")).toBeNull();
      unmount();
    }
  });
});
