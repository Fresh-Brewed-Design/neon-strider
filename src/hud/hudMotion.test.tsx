import { act, render, renderHook, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActiveProgram, AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

let reduced = false;
beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
beforeEach(() => {
  reduced = false;
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({ matches: reduced && query.includes("reduce"), media: query })),
  );
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const { snapshot: mock } = await import("../dev/mockBackend");
const { useLinkUp, useStepSweep, stepSweepLabel, sweepSpot } = await import("./hudMotion");
const { SCRAMBLE_FRAME_MS, SCRAMBLE_FRAMES } = await import("./HudHeader");
const { STEP_SWEEP_MS } = await import("./StepSweep");
const { HudApp } = await import("./HudApp");

const program = (patch: Partial<ActiveProgram> = {}): ActiveProgram => ({
  ...structuredClone(mock.active_program!),
  program_id: 7,
  step_index: 0,
  step_count: 3,
  cycle: 1,
  phase: "walking",
  paused: false,
  wait: null,
  kind: undefined,
  ...patch,
});

describe("useLinkUp", () => {
  it("counts each connect once, never the heartbeat's re-sent snapshots", () => {
    const { result, rerender } = renderHook((c: boolean | undefined) => useLinkUp(c), { initialProps: undefined });
    rerender(false);
    expect(result.current).toBe(0);
    rerender(true);
    expect(result.current).toBe(1);
    // The 2 s heartbeat sends the same link again.
    rerender(true);
    rerender(true);
    expect(result.current).toBe(1);
    rerender(false);
    rerender(true);
    expect(result.current).toBe(2);
  });

  it("never plays for a link already up when the HUD opens", () => {
    const { result, rerender } = renderHook((c: boolean | undefined) => useLinkUp(c), { initialProps: true });
    rerender(true);
    expect(result.current).toBe(0);
  });

  it("stays still under reduced motion", () => {
    reduced = true;
    const { result, rerender } = renderHook((c: boolean | undefined) => useLinkUp(c), { initialProps: false });
    rerender(true);
    expect(result.current).toBe(0);
  });
});

describe("useStepSweep", () => {
  type P = ActiveProgram | null | undefined;
  const hook = (initial: P) => renderHook((a: P) => useStepSweep(a), { initialProps: initial });

  it("sweeps once per step change, with the step's name", () => {
    const { result, rerender } = hook(program());
    expect(result.current).toBeNull();
    rerender(program());
    expect(result.current).toBeNull();
    rerender(program({ step_index: 1 }));
    expect(result.current).toEqual({ key: 1, label: "WALK 2/3" });
    // Heartbeat: the same step again.
    rerender(program({ step_index: 1, seconds_remaining: 100 }));
    expect(result.current?.key).toBe(1);
    rerender(program({ step_index: 2, kind: { kind: "interval", nth: 3, of: 5, label: "INTERVAL 3/5" } }));
    expect(result.current).toEqual({ key: 2, label: "INTERVAL 3/5" });
    // The next cycle's first step is a change too.
    rerender(program({ step_index: 0, cycle: 2 }));
    expect(result.current?.key).toBe(3);
  });

  it("never sweeps for the step a program starts on or the HUD opens on", () => {
    const { result, rerender } = hook(undefined);
    rerender(program({ step_index: 2 }));
    rerender(null);
    rerender(program({ program_id: 3 }));
    rerender(program({ program_id: 4 }));
    expect(result.current).toBeNull();
  });

  it("holds the sweep through a rest or a wait for GO until the step walks", () => {
    const { result, rerender } = hook(program());
    rerender(program({ phase: "resting", step_index: 0, cycle: 2 }));
    rerender(program({ step_index: 0, cycle: 2, paused: true, wait: "rest_complete" }));
    expect(result.current).toBeNull();
    rerender(program({ step_index: 0, cycle: 2 }));
    expect(result.current?.key).toBe(1);
    rerender(program({ step_index: 0, cycle: 2 }));
    expect(result.current?.key).toBe(1);
  });

  it("stays still under reduced motion", () => {
    reduced = true;
    const { result, rerender } = hook(program());
    rerender(program({ step_index: 1 }));
    expect(result.current).toBeNull();
  });

  it("names a one-step program's step plainly", () => {
    expect(stepSweepLabel(program({ step_count: 1 }))).toBe("WALK");
  });
});

describe("HUD motion", () => {
  const snap = (patch: Partial<AppSnapshot> = {}): AppSnapshot => ({ ...structuredClone(mock), ...patch });
  const linked = (connected: boolean) =>
    snap({
      connection: {
        ...structuredClone(mock.connection),
        connected,
        link: connected ? { state: "connected" } : { state: "connecting", attempt: 1, of: 3 },
      },
    });

  it("decodes the title once per connect, keeping its name for screen readers", () => {
    vi.useFakeTimers();
    current = linked(false);
    const { rerender } = render(<HudApp />);
    current = linked(true);
    rerender(<HudApp />);
    act(() => vi.advanceTimersByTime(SCRAMBLE_FRAME_MS));
    const noise = screen.getByTestId("hud-title-scramble");
    expect(noise).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByText("NEON STRIDER")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(SCRAMBLE_FRAME_MS * SCRAMBLE_FRAMES));
    expect(screen.queryByTestId("hud-title-scramble")).not.toBeInTheDocument();
    // The heartbeat re-sends the linked snapshot: no second decode.
    current = linked(true);
    rerender(<HudApp />);
    act(() => vi.advanceTimersByTime(SCRAMBLE_FRAME_MS * 2));
    expect(screen.queryByTestId("hud-title-scramble")).not.toBeInTheDocument();
  });

  it("sweeps the panel once with the new step's name, then clears", () => {
    vi.useFakeTimers();
    current = snap({ active_program: program() });
    const { rerender } = render(<HudApp />);
    expect(screen.queryByTestId("hud-step-sweep")).not.toBeInTheDocument();
    current = snap({ active_program: program({ step_index: 1 }) });
    rerender(<HudApp />);
    const sweep = screen.getByTestId("hud-step-sweep");
    expect(sweep).toHaveAttribute("aria-hidden", "true");
    expect(sweep).toHaveTextContent("STEP // WALK 2/3");
    act(() => vi.advanceTimersByTime(STEP_SWEEP_MS));
    expect(screen.queryByTestId("hud-step-sweep")).not.toBeInTheDocument();
    current = snap({ active_program: program({ step_index: 1 }) });
    rerender(<HudApp />);
    expect(screen.queryByTestId("hud-step-sweep")).not.toBeInTheDocument();
  });
});

describe("HUD motion CSS", () => {
  const css = readFileSync(join(process.cwd(), "src/hud/hud.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rule = (selector: string) => {
    for (const chunk of css.split("}")) {
      const [head, body] = chunk.split("{");
      if (body !== undefined && head.trim().endsWith(selector)) return body;
    }
    return "";
  };

  it("lays the motion over the panel without taking clicks or space", () => {
    for (const selector of [".hud-title-scramble", ".hud-step-sweep"]) {
      expect(rule(selector)).toMatch(/position:\s*absolute/);
      expect(rule(selector)).toMatch(/pointer-events:\s*none/);
    }
  });

  it("hides both under reduced motion", () => {
    const reducedBlocks = css.split("@media (prefers-reduced-motion: reduce)").slice(1).join(" ");
    expect(reducedBlocks).toMatch(/\.hud-title-scramble\s*\{\s*display:\s*none/);
    expect(reducedBlocks).toMatch(/\.hud-step-sweep\s*\{\s*display:\s*none/);
  });

  it("keeps the sweep's text at 11 px or more", () => {
    const sizes = [...css.matchAll(/\.hud-step-sweep[\w-]*\s*\{[^}]*font-size:\s*(\d+)px/g)].map((m) => Number(m[1]));
    expect(sizes.length).toBeGreaterThan(0);
    expect(Math.min(...sizes)).toBeGreaterThanOrEqual(11);
  });
});

describe("sweepSpot", () => {
  /** A panel 300 wide from y 0, its parts at the given rows; jsdom lays nothing out. */
  function panel(html: string, rows: Record<string, [number, number, number?, number?]>): HTMLElement {
    const el = document.createElement("section");
    el.innerHTML = html;
    const rect = (top: number, bottom: number, left = 0, right = 300) =>
      ({ top, bottom, left, right, width: right - left, height: bottom - top, x: left, y: top, toJSON() {} }) as DOMRect;
    el.getBoundingClientRect = () => rect(0, 500);
    for (const [selector, r] of Object.entries(rows)) {
      const part = el.querySelector<HTMLElement>(selector)!;
      part.getBoundingClientRect = () => rect(...r);
    }
    return el;
  }

  it("keeps the name over the whole panel's readings, above its controls", () => {
    const p = panel('<div class="hud-stats"></div><div class="hud-status-row"></div><div class="hud-controls"></div>', {
      ".hud-stats": [31, 230],
      ".hud-status-row": [230, 260],
    });
    expect(sweepSpot(p)).toEqual({ top: 31, left: 0, width: 300, height: 229 });
  });

  it("keeps the name over the running strip's readings, above its buttons", () => {
    const p = panel(
      '<div class="hud-strip hud-run-strip"><div class="hud-run-stats"></div><div class="hud-strip-row"></div></div>',
      { ".hud-run-stats": [31, 80], ".hud-strip-row": [110, 150] },
    );
    expect(sweepSpot(p)).toEqual({ top: 31, left: 0, width: 300, height: 79 });
  });

  it("keeps the name over the still strip's total, beside Engage", () => {
    const p = panel('<div class="hud-strip-row"><div class="hud-strip-cell"></div><button></button></div>', {
      ".hud-strip-cell": [31, 75, 0, 180],
    });
    expect(sweepSpot(p)).toEqual({ top: 31, left: 0, width: 180, height: 44 });
  });
});
