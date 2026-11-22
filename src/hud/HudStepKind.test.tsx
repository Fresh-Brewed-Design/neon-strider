import { render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");

beforeAll(() => {
  // jsdom has no ResizeObserver; the hit region only needs it to exist.
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

afterEach(() => {
  current = null;
});

/** The mock walking Interval walk's 3rd brisk round (step 6 of 11), shown as `view`. */
function intervalRound(view: "full" | "minified", kind = true): AppSnapshot {
  const s = structuredClone(mock);
  s.active_program = {
    ...s.active_program!,
    program_id: 7,
    name: "Interval walk",
    step_index: 5,
    step_count: 11,
    seconds_remaining: 128,
    cycle: 1,
    kind: kind ? { kind: "interval", nth: 3, of: 5, label: "INTERVAL 3/5" } : undefined,
  };
  s.hud = { ...s.hud, minified: view === "minified" };
  s.hud_view = view;
  return s;
}

describe("HUD step kind", () => {
  it("leads the program line with the kind and keeps the step number", () => {
    current = intervalRound("full");
    const { container } = render(<HudApp />);
    const lines = [...container.querySelectorAll(".hud-sub")].map((e) => e.textContent);
    expect(lines).toContain("INTERVAL\u00a03/5 · step\u00a06/11 · 02:08 left");
  });

  it("shows the kind on the running strip", () => {
    current = intervalRound("minified");
    const { container } = render(<HudApp />);
    expect(container.querySelector(".hud-run-segments")?.textContent).toBe("Interval walk · INTERVAL\u00a03/5 · step\u00a06/11 · 02:08 left");
  });

  it("shows a step with no kind as before", () => {
    current = intervalRound("full", false);
    render(<HudApp />);
    expect(screen.getByText("Walk 6/11 · 02:08 left")).toBeInTheDocument();
  });
});
