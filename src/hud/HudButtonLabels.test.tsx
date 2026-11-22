import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { engageSub, engageTitle } = await import("./status");

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

/** The panel with the belt stopped and no program, then `patch`. */
function panel(patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.hud_view = "full";
  s.hud = { ...s.hud, idle_mode: "full" };
  patch(s);
  return s;
}

const show = (s: AppSnapshot) => {
  current = s;
  return render(<HudApp />);
};

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

describe("HUD belt buttons", () => {
  it("name what each does in plain words under the codename", () => {
    const { unmount } = show(panel());
    expect(screen.getByRole("button", { name: "ENGAGE start" })).toHaveAttribute("title", "Start the belt.");
    expect(screen.getByRole("button", { name: "DUMP end walk" })).toHaveAttribute("title", "Stop the belt and end the session.");
    unmount();
    show(panel((s) => (s.deck.status = "Running")));
    expect(screen.getByRole("button", { name: "STASIS pause" })).toHaveAttribute("title", "Pause the belt.");
  });

  it("covers every state of the main button's line", () => {
    expect(engageSub(panel((s) => (s.deck.status = "Paused")))).toBe("resume");
    expect(engageTitle(panel((s) => (s.deck.status = "Paused")))).toBe("Resume the belt.");
    expect(engageSub(panel((s) => (s.connection.start_pending = true)))).toBe("starting");
    const go = panel((s) => {
      s.deck.status = "Paused";
      s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null };
    });
    expect(engageSub(go)).toBe("resume");
    expect(engageTitle(go)).toBe("Resume the program.");
  });

  it("label the running program's buttons and give them tooltips", () => {
    show(
      panel((s) => {
        s.deck.status = "Running";
        s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: false, wait: null };
      }),
    );
    expect(screen.getByRole("button", { name: "HOLD pause" })).toHaveAttribute("title", "Pause the program.");
    expect(screen.getByRole("button", { name: "SKIP next step" })).toHaveAttribute("title", "Skip to the next step.");
    expect(screen.getByRole("button", { name: "END end program" })).toHaveAttribute("title", "End the program.");
  });

  it("show GO with resume while the program is paused", () => {
    show(panel((s) => (s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null })));
    // The main button and the program row both read GO.
    const go = screen.getAllByRole("button", { name: "GO resume" });
    expect(go).toHaveLength(2);
    for (const b of go) expect(b).toHaveAttribute("title", "Resume the program.");
  });
});

describe("HUD program list offline", () => {
  const offline = (state: object, saved = true) =>
    panel((s) => Object.assign(s.connection, { connected: false, has_saved_device: saved, link: state }));

  it("says why the program buttons are off and offers Connect", async () => {
    vi.mocked(invoke).mockClear();
    show(offline({ state: "reconnecting", attempt: 1, next_retry_s: 9 }));
    expect(screen.getByTestId("hud-offline")).toHaveTextContent("Treadmill not connected.");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "CONNECT" })));
    expect(dispatched()).toEqual([{ command: { type: "connect_now" } }]);
  });

  it("opens the dashboard's connect dialog when no treadmill is saved", async () => {
    vi.mocked(invoke).mockClear();
    show(offline({ state: "idle" }, false));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "CONNECT" })));
    expect(dispatched()).toEqual([{ command: { type: "open_connect_dialog" } }]);
  });

  it("offers no Connect while a link is being made, and shows nothing when linked", () => {
    const { unmount } = show(offline({ state: "connecting", attempt: 1, of: 3 }));
    expect(screen.getByTestId("hud-offline")).toHaveTextContent("Connecting to the treadmill.");
    expect(screen.queryByRole("button", { name: "CONNECT" })).not.toBeInTheDocument();
    unmount();
    show(panel());
    expect(screen.queryByTestId("hud-offline")).not.toBeInTheDocument();
  });
});
