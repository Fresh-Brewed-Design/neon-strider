import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { PresentingLine } = await import("./HudPanel");

/** The mock's snapshot with Rust's presenting state. */
function snap(enabled: boolean, presenting: boolean): AppSnapshot {
  const s = structuredClone(mock);
  s.presenting = { enabled, presenting, holding: enabled && presenting };
  return s;
}

describe("PRESENTING HUD line", () => {
  it("shows while Rust holds the cards back", () => {
    render(<PresentingLine snapshot={snap(true, true)} />);
    expect(screen.getByTestId("hud-presenting")).toHaveTextContent("PRESENTING // CARDS HELD");
  });

  it("stays hidden when nothing is held", () => {
    const { rerender } = render(<PresentingLine snapshot={snap(true, false)} />);
    expect(screen.queryByTestId("hud-presenting")).toBeNull();
    rerender(<PresentingLine snapshot={snap(false, true)} />);
    expect(screen.queryByTestId("hud-presenting")).toBeNull();
  });
});
