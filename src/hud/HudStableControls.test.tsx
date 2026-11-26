import { render, screen, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { ENGAGE_WORDS, firstStepLine } = await import("./status");
const { splitSegments } = await import("./segments");

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
    const row = screen.getByTestId("hud-status-row");
    const note = within(row).getByTestId("hud-note");
    expect(note).toHaveTextContent("DECK_REFUSED // 3.5 KM/H, TILT 6.3 %");
    // Its one-line form while the themed line fits: the plain words stay for screen readers and the tooltip.
    expect(note).toHaveClass("is-one-line");
    expect(note).toHaveTextContent("The treadmill turned down 3.5 km/h. Pick another speed.");
    expect(row).toHaveAttribute("title", "The treadmill turned down 3.5 km/h. Pick another speed.");
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

/** The words laid in one swap cell, shown or not. */
const swapWords = (el: Element | null) => Array.from(el?.querySelectorAll(":scope > span") ?? []).map((e) => e.textContent);

describe("HUD controls keep one place in every program state", () => {
  const running = (s: AppSnapshot) => {
    s.deck.status = "Running";
    s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: false, wait: null };
  };
  const waiting = (s: AppSnapshot) => {
    s.deck.status = "Paused";
    s.active_program = { ...structuredClone(mock.active_program!), paused: true, wait: "rest_complete" };
  };

  it("lays the RUN PROGRAM grid unseen under a running program, and a blank running program under the grid", () => {
    const { unmount } = show(running);
    const grid = document.querySelector(".hud-program-grid");
    expect(grid).toHaveClass("hud-reserve");
    expect(grid).toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByRole("button", { name: new RegExp(`^${mock.programs[0].name}`, "i") })).toBeNull();
    expect(document.querySelector(".hud-program-run.hud-reserve")).not.toBeNull();
    unmount();
    show((s) => (s.active_program = null));
    expect(document.querySelector(".hud-program-grid")).not.toHaveClass("hud-reserve");
    expect(document.querySelector(".hud-program-run.hud-reserve")).not.toBeNull();
  });

  it("gives STASIS and HOLD every wording they take, so their size never changes", () => {
    const words = (patch: (s: AppSnapshot) => void) => {
      const { unmount } = show(patch);
      const engage = document.querySelector(".hud-transport .hud-btn-main");
      const hold = document.querySelector(".hud-program-run:not(.hud-reserve) .hud-auto-actions .hud-btn");
      const out = {
        engage: swapWords(engage?.querySelector(".hud-swap") ?? null),
        engageSub: swapWords(engage?.querySelector(".hud-btn-sub") ?? null),
        hold: swapWords(hold?.querySelector(".hud-swap") ?? null),
        holdSub: swapWords(hold?.querySelector(".hud-btn-sub") ?? null),
      };
      unmount();
      return out;
    };
    const a = words(running);
    expect(a.engage).toEqual([...ENGAGE_WORDS]);
    expect(a.holdSub).toEqual(["pause program", "resume program", "start belt"]);
    expect(words(waiting)).toEqual(a);
  });

  it("keeps the minify button's place while a program waits for GO", () => {
    show(waiting);
    expect(screen.getByRole("button", { name: "Minify the HUD" })).toBeDisabled();
  });

  it("keeps room for PULSE_RATE's zone line while a sensor is picked", () => {
    show((s) => {
      s.hud.widgets = ["HEART_RATE", ...s.hud.widgets];
      s.heart_rate.sensor = { id: "watch", name: "Watch" };
      s.heart_rate.zone = null;
      s.heart_rate.cool_down = null;
    });
    expect(document.querySelector(".hud-stat .hud-zone.hud-reserve")).not.toBeNull();
  });

  it("keeps the running strip's program line room with no program", () => {
    show((s) => {
      s.hud_view = "minified";
      s.active_program = null;
    });
    expect(document.querySelector(".hud-run-program.hud-reserve")).not.toBeNull();
  });
});

describe("HUD cards and toasts", () => {
  it("gives ATROPHY_WARNING's ENGAGE the cyan ENGAGE has everywhere", () => {
    show((s) => (s.seated.show_card = true));
    expect(screen.getByRole("button", { name: "ENGAGE" })).toHaveClass("accent-cyan");
  });

  it("splits a toast into whole parts that keep their separators", () => {
    expect(splitSegments("CALL DETECTED // QUIET MODE")).toEqual([
      { text: "CALL DETECTED", sep: "//" },
      { text: "QUIET MODE", sep: "" },
    ]);
    expect(splitSegments("SESSION SAVED · 32:14 · 1.72 km").map((p) => p.sep)).toEqual(["·", "·", ""]);
    expect(splitSegments("WORKDAY IN 30S")).toEqual([{ text: "WORKDAY IN 30S", sep: "" }]);
  });
});
