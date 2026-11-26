import { render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, HudCard } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { SeatedCard } = await import("./HudCards");

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

/** Every card due at once, the belt still, on the strip; Rust picked `card`. */
function allDue(card: HudCard, patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.hud = { ...s.hud, idle_mode: "strip" };
  s.hud_view = "strip";
  s.seated = { ...s.seated, show_card: true, seated_minutes: 46 };
  s.eye_break = { ...s.eye_break, enabled: true, phase: "upcoming", remaining_s: 60 };
  s.update = { ...s.update, hud_card: true };
  s.weekly = { ...s.weekly, show_card: true };
  s.hud_card = card;
  patch(s);
  return s;
}

function show(s: AppSnapshot) {
  current = s;
  return render(<HudApp />);
}

const cards = () => screen.queryAllByText(/^(ATROPHY_WARNING|OCULAR_IMPLANT|LAST WEEK|UPDATE READY)/);

describe("HUD cards, one at a time (#421)", () => {
  it("shows only the card Rust names, however many are due", () => {
    show(allDue("eye_break"));
    expect(cards().map((c) => c.textContent)).toEqual(["OCULAR_IMPLANT"]);
  });

  it("shows the next one once Rust moves on", () => {
    show(allDue("weekly"));
    expect(screen.getByText(/LAST WEEK/)).toBeInTheDocument();
    expect(cards()).toHaveLength(1);
  });

  it("shows the DEFERRED eye break while a walk runs, and no ATROPHY_WARNING", () => {
    show(
      allDue("eye_break", (s) => {
        s.deck = { ...s.deck, status: "Running", speed_kmh: 4 };
        s.hud_view = "full";
        s.eye_break = { ...s.eye_break, phase: "waiting", remaining_s: 492 };
      }),
    );
    expect(cards().map((c) => c.textContent)).toEqual(["OCULAR_IMPLANT // DEFERRED"]);
  });

  it("shows none while Rust names none, with the panel still up", () => {
    show(allDue("none"));
    expect(cards()).toHaveLength(0);
    expect(screen.getByRole("button", { name: "ENGAGE start" })).toBeInTheDocument();
  });
});

describe("ATROPHY_WARNING folded into the strip (#422)", () => {
  it("shows the seated time in yellow in today's total's place, with Engage where it was", () => {
    const { container } = show(allDue("none", (s) => (s.seated.folded = true)));
    const readout = screen.getByTestId("hud-seated-readout");
    expect(readout).toHaveClass("accent-yellow");
    expect(readout.textContent).toBe("ATROPHY_WARNING46MIN IDLE");
    expect(screen.queryByText("TODAY")).toBeNull();
    expect(screen.getByRole("button", { name: "ENGAGE start" })).toBeInTheDocument();
    expect(container.querySelector(".hud-seated")).toBeNull();
  });

  it("is a status line on the whole panel", () => {
    show(
      allDue("none", (s) => {
        s.seated.folded = true;
        s.hud = { ...s.hud, idle_mode: "full" };
        s.hud_view = "full";
      }),
    );
    expect(screen.getByTestId("hud-seated-line").textContent).toBe("ATROPHY_WARNING46MIN IDLE");
  });

  it("is not there while the card is due unfolded", () => {
    show(allDue("seated"));
    expect(screen.queryByTestId("hud-seated-readout")).toBeNull();
  });
});

describe("SNOOZE lengthens (#422)", () => {
  it("says how long this snooze is", () => {
    const s = allDue("seated", (x) => (x.seated.snooze_minutes = 30));
    render(<SeatedCard snapshot={s} />);
    expect(screen.getByRole("button", { name: "SNOOZE 30 MIN" })).toBeInTheDocument();
  });
});
