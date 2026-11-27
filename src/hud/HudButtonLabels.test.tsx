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
    // DUMP is off on a stopped belt, and its tooltip says why.
    expect(screen.getByRole("button", { name: "DUMP end walk" })).toHaveAttribute("title", "The belt is already stopped.");
    unmount();
    show(panel((s) => (s.deck.status = "Running")));
    expect(screen.getByRole("button", { name: "STASIS pause belt" })).toHaveAttribute("title", "Pause the belt.");
    expect(screen.getByRole("button", { name: "DUMP end walk" })).toHaveAttribute("title", "Stop the belt and end the session.");
  });

  it("covers every state of the main button's line", () => {
    expect(engageSub(panel((s) => (s.deck.status = "Paused")))).toBe("resume belt");
    expect(engageTitle(panel((s) => (s.deck.status = "Paused")))).toBe("Resume the belt.");
    expect(engageSub(panel((s) => (s.connection.start_pending = true)))).toBe("starting");
    const go = panel((s) => {
      s.deck.status = "Paused";
      s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null };
    });
    expect(engageSub(go)).toBe("resume program");
    expect(engageTitle(go)).toBe("Resume the program.");
    const clocked = panel((s) => {
      s.deck.status = "Paused";
      s.deck.paused_for_s = 200;
    });
    // The STASIS clock has its own place: the plain line still says what a press does.
    expect(engageSub(clocked)).toBe("resume belt");
  });

  it("say the deck must be woken first while it sleeps, on ENGAGE and on GO", () => {
    const wake = "Wake the deck first at the treadmill or remote.";
    const asleep = panel((s) => (s.deck.standby = true));
    expect(engageSub(asleep)).toBe("wake deck first");
    expect(engageTitle(asleep)).toBe(wake);
    const waiting = panel((s) => {
      s.deck.standby = true;
      s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: "rest_complete" };
    });
    expect(engageSub(waiting)).toBe("wake deck first");
    show(waiting);
    // The main button and the program card's GO both say so; the press is unchanged.
    expect(screen.getAllByRole("button", { name: "GO wake deck first" })).toHaveLength(2);
    for (const b of screen.getAllByRole("button", { name: "GO wake deck first" })) expect(b).toHaveAttribute("title", wake);
  });

  it("start the belt from GO after a wait, in the program card's words", () => {
    const waiting = panel((s) => {
      s.deck.status = "Stopped";
      s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: "rest_complete" };
    });
    expect(engageSub(waiting)).toBe("start belt");
    expect(engageTitle(waiting)).toBe("Start the belt and carry on with the program.");
  });

  it("label the running program's buttons and give them tooltips", () => {
    show(
      panel((s) => {
        s.deck.status = "Running";
        s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: false, wait: null };
      }),
    );
    expect(screen.getByRole("button", { name: "HOLD pause program" })).toHaveAttribute("title", "Hold the program.");
    expect(screen.getByRole("button", { name: "SKIP next step" })).toHaveAttribute("title", "Skip to the next step.");
    expect(screen.getByRole("button", { name: "END end program" })).toHaveAttribute("title", "End the program.");
  });

  it("show GO with resume program while the program is paused", () => {
    show(panel((s) => (s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null })));
    // The main button and the program row both read GO.
    const go = screen.getAllByRole("button", { name: "GO resume program" });
    expect(go).toHaveLength(2);
    for (const b of go) expect(b).toHaveAttribute("title", "Resume the program.");
  });
});

describe("HUD program list offline", () => {
  const offline = (state: object, saved = true) =>
    panel((s) => Object.assign(s.connection, { connected: false, has_saved_device: saved, link: state }));

  it("says why the program buttons are off and offers RETRY NOW while RELINK counts down", async () => {
    vi.mocked(invoke).mockClear();
    show(offline({ state: "reconnecting", attempt: 1, next_retry_s: 9 }));
    expect(screen.getByTestId("hud-offline")).toHaveTextContent("Link lost. RELINK 9S.");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "RETRY NOW" })));
    expect(dispatched()).toEqual([{ command: { type: "connect_now" } }]);
  });

  it("offers CONNECT once the retries gave up", () => {
    show(offline({ state: "failed" }));
    expect(screen.getByRole("button", { name: "CONNECT" })).toBeInTheDocument();
  });

  it("opens the dashboard's connect dialog when no treadmill is saved", async () => {
    vi.mocked(invoke).mockClear();
    show(offline({ state: "idle" }, false));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "CONNECT" })));
    expect(dispatched()).toEqual([{ command: { type: "open_connect_dialog" } }]);
  });

  it("offers no Connect while a link is being made, and shows nothing when linked", () => {
    const { unmount } = show(offline({ state: "connecting", attempt: 1, of: 3 }));
    expect(screen.getByTestId("hud-offline")).toHaveTextContent("Linking, try 1 of 3.");
    expect(screen.queryByRole("button", { name: "CONNECT" })).not.toBeInTheDocument();
    unmount();
    show(panel());
    expect(screen.queryByTestId("hud-offline")).not.toBeInTheDocument();
  });
});
