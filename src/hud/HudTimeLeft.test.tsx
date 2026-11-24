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

const tMinus = (container: HTMLElement) => container.querySelector("[data-testid='hud-t-minus']")?.textContent ?? null;

function withLeft(left: number | null, paused = false): AppSnapshot {
  const s = structuredClone(mock);
  s.active_program = { ...s.active_program!, repeat_every_minutes: null, paused, wait: null, program_seconds_remaining: left };
  return s;
}

describe("HUD T-MINUS", () => {
  it("shows the whole program's time left and end under the step line", () => {
    current = withLeft(1390);
    const { container } = render(<HudApp />);
    expect(tMinus(container)).toMatch(/^T-MINUS 23:10 · ENDS \S+/);
    expect(container.querySelector(".hud-run-status")?.textContent).toContain("left");
  });

  it("shows only the time left while paused", () => {
    current = withLeft(1390, true);
    const { container } = render(<HudApp />);
    expect(tMinus(container)).toBe("T-MINUS 23:10");
  });

  it("shows nothing for a repeating program", () => {
    current = structuredClone(mock);
    const { container } = render(<HudApp />);
    expect(tMinus(container)).toBeNull();
  });
});
