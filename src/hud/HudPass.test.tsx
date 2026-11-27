import { fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
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
  vi.mocked(invoke).mockClear();
});

/** The mock's running program, made repeating or not, as the full panel. */
function running(repeat: number | null, extra: Partial<NonNullable<AppSnapshot["active_program"]>> = {}): AppSnapshot {
  const s = structuredClone(mock);
  s.active_program = { ...s.active_program!, repeat_every_minutes: repeat, ...extra };
  s.hud = { ...s.hud, minified: false };
  s.hud_view = "full";
  return s;
}

/** The program row's PASS (not the blank reserve under it). */
const passButton = () =>
  screen.getAllByRole("button", { name: /PASS/ }).find((b) => !b.closest(".hud-reserve")) as HTMLButtonElement;

describe("HUD PASS (#428)", () => {
  it("sits between SKIP and END and sends the command on a repeating program", () => {
    current = running(30);
    const { container } = render(<HudApp />);
    const row = container.querySelector(".hud-program-run:not(.hud-reserve) .hud-auto-actions")!;
    const names = [...row.querySelectorAll("button")].map((b) => b.textContent ?? "");
    expect(names.map((n) => n.split(" ")[0]).slice(1)).toEqual(["SKIP", "PASS", "END"]);
    expect(names[0]).toMatch(/^HOLD/);
    const pass = passButton();
    expect(pass).toBeEnabled();
    expect(pass).toHaveAttribute("title", "Skip one cycle and keep the schedule.");
    expect(pass).toHaveTextContent("skip cycle");
    fireEvent.click(pass);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("dispatch", { command: { type: "program_pass" } });
  });

  it("keeps its slot, greyed out, on a program that does not repeat", () => {
    current = running(null);
    render(<HudApp />);
    const pass = passButton();
    expect(pass).toBeDisabled();
    expect(pass).toHaveAttribute("title", "Only a program that repeats has a cycle to pass.");
  });

  it("is drawn in the blank reserve too, so the module keeps its height", () => {
    current = running(null);
    const { container } = render(<HudApp />);
    const reserve = container.querySelector(".hud-program-run.hud-reserve .hud-auto-actions")!;
    expect(reserve.querySelectorAll("button")).toHaveLength(4);
  });

  it("names the cycles passed in the label slot during the rest", () => {
    current = running(30, {
      phase: "resting",
      paused: false,
      wait: null,
      pass_note: { label: "CYCLE 4 PASSED", sentence: "Cycle 4 passed. Cycle 5 stays on schedule." },
    });
    render(<HudApp />);
    const label = screen.getByTestId("hud-program-label");
    expect(label).toHaveTextContent("CYCLE 4 PASSED");
    expect(label).toHaveClass("hud-pass-label");
  });

  it("reads PROGRAM without a pass", () => {
    current = running(30, { phase: "walking", paused: false, wait: null, pass_note: undefined });
    render(<HudApp />);
    expect(screen.getByTestId("hud-program-label")).toHaveTextContent("PROGRAM");
  });
});
