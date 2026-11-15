import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { engageLabel } = await import("./status");

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

/** The mock's snapshot, with the belt stopped and no program, then `patch`. */
function still(view: AppSnapshot["hud_view"], patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.hud = { ...s.hud, idle_mode: view === "full" ? "full" : view };
  s.hud_view = view;
  patch(s);
  return s;
}

function show(s: AppSnapshot) {
  current = s;
  return render(<HudApp />);
}

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

describe("HUD strip", () => {
  it("shows the link state, today's total, expand and Engage, and nothing else", () => {
    const { container } = show(still("strip"));
    expect(screen.getByText("NEON STRIDER")).toBeInTheDocument();
    expect(screen.getByText("STANDBY")).toBeInTheDocument();
    expect(screen.getByText("TODAY")).toBeInTheDocument();
    expect(screen.getByText("4.31")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show the whole HUD" })).toHaveAttribute("title");
    expect(screen.getByRole("button", { name: "ENGAGE" })).toBeEnabled();
    // The full panel's stats and controls are gone.
    expect(screen.queryByText("DUMP")).not.toBeInTheDocument();
    expect(container.querySelector(".hud-stats")).toBeNull();
  });

  it("expand and Engage each send one command", async () => {
    vi.mocked(invoke).mockClear();
    show(still("strip"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Show the whole HUD" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ENGAGE" })));
    expect(dispatched()).toEqual([{ command: { type: "hud_expand" } }, { command: { type: "toggle_start" } }]);
  });

  it("labels Engage like the panel does", () => {
    show(still("strip", (s) => (s.deck.status = "Paused")));
    expect(screen.getByRole("button", { name: "RE-JACK" })).toBeInTheDocument();
  });

  it("reads WAKING… while a start is pending, disabled when offline", () => {
    const { unmount } = show(still("strip", (s) => (s.connection.start_pending = true)));
    expect(screen.getByRole("button", { name: "WAKING…" })).toBeInTheDocument();
    unmount();
    show(
      still("strip", (s) =>
        Object.assign(s.connection, { connected: false, has_saved_device: true, link: { state: "reconnecting", attempt: 1, next_retry_s: 9 } }),
      ),
    );
    expect(screen.getByRole("button", { name: "ENGAGE" })).toBeDisabled();
    // Offline, the link status is still the relink button.
    expect(screen.getByRole("button", { name: "RELINK 9S" })).toBeInTheDocument();
  });

  it("shows a rejected Engage right under the strip", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    show(still("strip"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ENGAGE" })));
    expect(screen.getByRole("alert")).toHaveTextContent("SYS // TREADMILL NOT CONNECTED");
  });

  it("shows the last Engage's failure", () => {
    show(
      still("strip", (s) => (s.connection.start_result = { ok: false, message: "The treadmill didn't respond.", at_ms: 1 })),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("The treadmill didn't respond.");
  });
});

describe("engageLabel", () => {
  it("covers every state of the main button", () => {
    expect(engageLabel(still("full"))).toBe("ENGAGE");
    expect(engageLabel(still("full", (s) => (s.deck.status = "Paused")))).toBe("RE-JACK");
    expect(engageLabel(still("full", (s) => (s.connection.start_pending = true)))).toBe("WAKING…");
    expect(engageLabel(still("full", (s) => (s.deck.status = "Running")))).toBe("STASIS");
    const go = still("full", (s) => {
      s.deck.status = "Paused";
      s.active_program = { ...structuredClone(mock.active_program!), paused: true };
    });
    expect(engageLabel(go)).toBe("GO");
  });
});

describe("HUD hidden", () => {
  it("renders nothing when there is nothing else to show", () => {
    const { container } = show(still("hidden"));
    expect(container).toBeEmptyDOMElement();
  });

  it("still shows a card that needs an answer, without the panel", () => {
    const { container } = show(
      still("hidden", (s) => Object.assign(s.walkaway, { card: "paused", reason: "lock" })),
    );
    expect(container.querySelector(".hud-panel")).toBeNull();
    expect(container.querySelector(".hud-walkaway")).not.toBeNull();
    expect(screen.queryByText("NEON STRIDER")).not.toBeInTheDocument();
  });
});

describe("HUD full", () => {
  it("is the whole panel, as before", () => {
    const { container } = show(still("full"));
    expect(container.querySelector(".hud-stats")).not.toBeNull();
    expect(screen.getByText("DUMP")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Show the whole HUD" })).not.toBeInTheDocument();
  });
});
