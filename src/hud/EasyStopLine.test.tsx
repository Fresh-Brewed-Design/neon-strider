import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { Controls, EasyStopLine } = await import("./HudPanel");

const MESSAGE = "Slowing to 0.6 km/h, then stopping. DUMP again stops the belt at once.";

/** The mock's snapshot with the belt easing down, or not. */
function snap(easing: boolean): AppSnapshot {
  const s = structuredClone(mock);
  s.easy_stop = easing
    ? { enabled: true, ramp_s: 20, easing: true, stop_in_s: 12, to_kmh: 0.6, message: MESSAGE }
    : { enabled: true, ramp_s: 20, easing: false, stop_in_s: null, to_kmh: null, message: null };
  return s;
}

describe("easy stop HUD line", () => {
  it("shows the slowest speed and the countdown while easing, with Rust's sentence as the tooltip", () => {
    render(<EasyStopLine snapshot={snap(true)} />);
    const line = screen.getByTestId("hud-easystop");
    expect(line).toHaveTextContent("EASY STOP");
    expect(line).toHaveTextContent("to 0.6km/h12 s");
    expect(line).toHaveAttribute("title", MESSAGE);
  });

  it("stays hidden while the belt is not easing down", () => {
    render(<EasyStopLine snapshot={snap(false)} />);
    expect(screen.queryByTestId("hud-easystop")).toBeNull();
  });

  it("names DUMP's second press while easing", () => {
    const { unmount } = render(<Controls snapshot={snap(true)} />);
    expect(screen.getByText("stop now")).toBeInTheDocument();
    unmount();
    render(<Controls snapshot={snap(false)} />);
    expect(screen.getByText("end walk")).toBeInTheDocument();
  });
});
