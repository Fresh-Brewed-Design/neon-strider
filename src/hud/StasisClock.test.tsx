import { render, screen, within } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");

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

/** The belt paused 3:20 ago with no program, in `view`, then `patch`. */
function paused(view: AppSnapshot["hud_view"], patch: (s: AppSnapshot) => void = () => {}) {
  const s = structuredClone(mock);
  s.connection.connected = true;
  s.deck = { ...s.deck, status: "Paused", speed_kmh: 0, paused_for_s: 200 };
  s.active_program = null;
  s.hud = { ...s.hud, idle_mode: view === "strip" ? "strip" : "full", minified: view === "minified" };
  s.hud_view = view;
  patch(s);
  current = s;
  return render(<HudApp />);
}

describe("the STASIS clock on the HUD", () => {
  it("has the whole panel's status row while the belt is paused, and RE-JACK keeps its plain line", () => {
    paused("full");
    const line = within(screen.getByTestId("hud-status-row")).getByTestId("hud-stasis-line");
    expect(line).toHaveTextContent("STASIS 3:20 PAUSED");
    expect(screen.getByRole("button", { name: "RE-JACK resume belt" })).toBeInTheDocument();
    // The header keeps its one word.
    expect(document.querySelector(".hud-header")).not.toHaveTextContent("3:20");
  });

  it("shows nothing while the belt runs", () => {
    paused("full", (s) => (s.deck = { ...s.deck, status: "Running", speed_kmh: 3, paused_for_s: null }));
    expect(screen.queryByTestId("hud-stasis-line")).toBeNull();
  });

  it("takes today's total's place on the strip, after a rest and a folded ATROPHY_WARNING", () => {
    const strip = paused("strip");
    expect(screen.getByTestId("hud-strip-stasis")).toHaveTextContent("STASIS 3:20 PAUSED");
    expect(screen.queryByText("TODAY")).toBeNull();
    expect(screen.getByRole("button", { name: "RE-JACK resume belt" })).toBeInTheDocument();
    strip.unmount();
    paused("strip", (s) => (s.seated = { ...s.seated, folded: true, seated_minutes: 42 }));
    expect(screen.getByTestId("hud-seated-readout")).toBeInTheDocument();
    expect(screen.queryByTestId("hud-strip-stasis")).toBeNull();
  });

  it("sits in the running strip's program line, alone or after a paused program's label", () => {
    const alone = paused("minified");
    expect(screen.getByTestId("hud-run-stasis")).toHaveTextContent("STASIS 3:20 PAUSED");
    alone.unmount();
    paused("minified", (s) => {
      s.deck.paused_for_s = 3909;
      s.active_program = structuredClone(mock.active_program);
      if (s.active_program) s.active_program.paused = true;
    });
    expect(screen.getByTestId("hud-run-clock")).toHaveTextContent("1h05");
    expect(screen.getByText("PAUSED", { exact: false })).toHaveTextContent("PAUSED 1h05");
    expect(screen.getByRole("button", { name: "GO resume program" })).toBeInTheDocument();
  });
});
