import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { StrideFitLine } = await import("./HudPanel");

/** The mock's snapshot with STRIDE_FIT at `fit` and Rust's `above_stride_fit`. */
function snap(fit: number | null, above: boolean): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, stride_fit_kmh: fit, above_stride_fit: above };
  return s;
}

describe("STRIDE_FIT HUD line", () => {
  it("shows the fit speed while Rust says the belt is above it", () => {
    render(<StrideFitLine snapshot={snap(4.5, true)} />);
    const line = screen.getByTestId("hud-stride");
    expect(line).toHaveTextContent("ABOVE STRIDE_FIT");
    expect(line).toHaveTextContent("4.5km/h");
  });

  it("stays hidden at or under the fit, and while STRIDE_FIT is unset", () => {
    const { rerender } = render(<StrideFitLine snapshot={snap(4.5, false)} />);
    expect(screen.queryByTestId("hud-stride")).toBeNull();
    rerender(<StrideFitLine snapshot={snap(null, true)} />);
    expect(screen.queryByTestId("hud-stride")).toBeNull();
  });
});
