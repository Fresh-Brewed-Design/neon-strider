import { render } from "@testing-library/react";
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

const nextLine = (container: HTMLElement) => container.querySelector(".hud-next")?.textContent ?? null;

describe("HUD next step", () => {
  it("shows the step after the running one under it", () => {
    const s = structuredClone(mock);
    const p = s.programs.find((x) => x.id === 7)!;
    s.active_program = { ...s.active_program!, program_id: 7, name: p.name, step_index: 5, step_count: 11, seconds_remaining: 128, cycle: 1, steps: p.steps, repeat_every_minutes: null };
    current = s;
    const { container } = render(<HudApp />);
    expect(nextLine(container)).toBe("NEXT // REST 3/5 · 3 min · 3.5 km/h");
  });

  it("shows the rest between cycles after a repeating program's last step", () => {
    current = structuredClone(mock);
    const { container } = render(<HudApp />);
    expect(nextLine(container)).toBe("NEXT // CYCLE_REST · 25 min");
  });

  it("leaves the line out while the program waits for GO", () => {
    const s = structuredClone(mock);
    s.active_program = { ...s.active_program!, paused: true, wait: "rest_complete" };
    current = s;
    const { container } = render(<HudApp />);
    expect(nextLine(container)).toBeNull();
  });
});
