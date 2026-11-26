import { render, screen, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { firstStepLine } = await import("./status");

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

afterEach(() => {
  current = null;
});

function show(patch: (s: AppSnapshot) => void = () => {}) {
  const s = structuredClone(mock);
  s.hud_view = "full";
  patch(s);
  current = s;
  return render(<HudApp />);
}

describe("HUD notes never take a line between the controls", () => {
  it("shows the deck's reply with Rust's plain words in the status row", () => {
    show((s) => {
      s.deck.target_note = "DECK_REFUSED // 3.5 KM/H, TILT 6.3 %";
      s.deck.target_note_hint = "The treadmill turned down 3.5 km/h. Pick another speed.";
    });
    const note = within(screen.getByTestId("hud-status-row")).getByTestId("hud-note");
    expect(note).toHaveTextContent("DECK_REFUSED // 3.5 KM/H, TILT 6.3 %");
    expect(note).toHaveTextContent("The treadmill turned down 3.5 km/h. Pick another speed.");
    // Nothing sits between the transport row and the steppers.
    expect(document.querySelector(".hud-module .hud-note")).toBeNull();
  });

  it("keeps the status row while nothing shows in it", () => {
    show();
    expect(screen.getByTestId("hud-status-row")).toBeInTheDocument();
    expect(screen.queryByTestId("hud-note")).toBeNull();
  });

  it("labels the steppers as targets and keeps their button names", () => {
    show();
    expect(screen.getByText("VEL TARGET")).toBeInTheDocument();
    expect(screen.getByText("TILT TARGET")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "VEL up" })).toBeInTheDocument();
  });
});

describe("RUN PROGRAM buttons", () => {
  it("say the speed a press sets: the first step's", () => {
    show((s) => (s.active_program = null));
    for (const p of mock.programs.slice(0, 6)) {
      expect(screen.getByRole("button", { name: `${p.name} ${firstStepLine(p)}` })).toBeInTheDocument();
    }
  });

  it("reads still for a program whose first step stands", () => {
    const p = structuredClone(mock.programs[0]);
    p.steps = [{ ...p.steps[0], speed_kmh: 0 }];
    expect(firstStepLine(p)).toBe("starts still");
    p.steps = [{ ...p.steps[0], speed_kmh: 3 }];
    expect(firstStepLine(p)).toBe("starts 3.0 km/h");
  });
});
