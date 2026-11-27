import { act, render, screen } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";
import type { ToastPayload } from "./useToasts";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { UpdateCard } = await import("./HudCards");
const { dumpTitle } = await import("./status");

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

/** The whole panel with the belt stopped and no program, then `patch`. */
function panel(patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.hud_view = "full";
  s.hud = { ...s.hud, idle_mode: "full" };
  patch(s);
  return s;
}

/** Workday's rest is over on a still belt and cycle 4 waits for GO (as `?program=ready`). */
const waiting = (s: AppSnapshot) => {
  s.deck.status = "Paused";
  s.active_program = { ...structuredClone(mock.active_program!), cycle: 4, paused: true, wait: "rest_complete" };
};

const offline = (s: AppSnapshot) => Object.assign(s.connection, { connected: false, link: { state: "failed" } });

const show = (s: AppSnapshot) => {
  current = s;
  return render(<HudApp />);
};

const panelEl = (c: HTMLElement) => c.querySelector(".hud-panel");

describe("HUD panel at rest", () => {
  it("marks the panel resting while the linked belt is still", () => {
    const { container } = show(panel());
    expect(panelEl(container)).toHaveClass("is-resting");
  });

  it("never rests while the link is down", () => {
    const { container } = show(panel(offline));
    expect(panelEl(container)).not.toHaveClass("is-resting");
  });

  it("never rests while a program waits for GO", () => {
    const { container } = show(panel(waiting));
    expect(panelEl(container)).not.toHaveClass("is-resting");
  });

  it("does not rest while the belt runs", () => {
    const { container } = show(panel((s) => (s.deck.status = "Running")));
    expect(panelEl(container)).not.toHaveClass("is-resting");
  });
});

describe("HUD waiting for GO", () => {
  it("keeps one prompt on the program card, with what comes next, SKIP and END", () => {
    show(panel(waiting));
    expect(screen.getByText("REST COMPLETE // PRESS GO")).toBeInTheDocument();
    expect(screen.queryByText("READY")).not.toBeInTheDocument();
    expect(screen.queryByText(/GO starts the belt/)).not.toBeInTheDocument();
    // The card's GO and Engage's GO say the same: both start the belt.
    const gos = screen.getAllByRole("button", { name: "GO start belt" });
    expect(gos).toHaveLength(2);
    for (const b of gos) expect(b).toHaveAttribute("title", "Start the belt and carry on with the program.");
    expect(gos.filter((b) => b.classList.contains("hud-btn-go"))).toHaveLength(1);
    expect(screen.getByText(/cycle 4/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "SKIP next step" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "END end program" })).toBeInTheDocument();
    // The controls keep their layout: Engage reads GO in its own slot, DUMP beside it.
    // The shown word of each button (Engage's other words wait unseen in the same cell).
    const word = (b: Element) => {
      const first = b.childNodes[0];
      return (first instanceof Element ? (first.querySelector(".is-shown") ?? first) : first).textContent?.trim();
    };
    const transport = Array.from(document.querySelectorAll(".hud-transport > button")).map(word);
    expect(transport).toEqual(["GO", "DUMP", "QUIET"]);
    expect(screen.getByRole("button", { name: "QUIET slow down" })).toBeInTheDocument();
  });

  it("keeps Engage's GO when the program card is not pinned", () => {
    show(
      panel((s) => {
        waiting(s);
        s.hud.widgets = s.hud.widgets.filter((w) => w !== "PROGRAMS");
      }),
    );
    expect(screen.getByRole("button", { name: "GO start belt" })).toBeInTheDocument();
  });

  it("drops the toast that repeats the card's prompt, and keeps other toasts", async () => {
    let handler: ((e: { payload: ToastPayload }) => void) | undefined;
    vi.mocked(listen).mockImplementation((event, cb) => {
      if (event === "hud-update") handler = cb as typeof handler;
      return Promise.resolve(() => {});
    });
    try {
      show(panel(waiting));
      await act(async () => {
        handler?.({ payload: { type: "AUTOMATION", message: "REST COMPLETE // PRESS GO", isActionable: false } });
        handler?.({ payload: { type: "STRAVA", message: "UPLOADED" } });
      });
      expect(handler).toBeDefined();
      expect(screen.getAllByText("REST COMPLETE // PRESS GO")).toHaveLength(1);
      expect(screen.queryByText("SYS // AUTOMATION")).not.toBeInTheDocument();
      expect(screen.getByText("UPLOADED")).toBeInTheDocument();
    } finally {
      vi.mocked(listen).mockImplementation(() => Promise.resolve(() => {}));
    }
  });
});

describe("HUD controls offline", () => {
  it("says in the status row why they are off, once for the whole panel", () => {
    show(panel(offline));
    const lines = screen.getAllByTestId("hud-offline");
    expect(lines).toHaveLength(1);
    expect(lines[0]).toHaveTextContent("Treadmill offline.");
    expect(lines[0].closest(".hud-status-row")).not.toBeNull();
  });

  it("dims the readings and the program, and keeps END while offline", () => {
    show(
      panel((s) => {
        offline(s);
        s.active_program = structuredClone(mock.active_program);
      }),
    );
    expect(document.querySelector(".hud-panel")).toHaveClass("is-offline");
    expect(screen.getByRole("button", { name: "HOLD pause program" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "SKIP next step" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "END end program" })).toBeEnabled();
  });

  it("keeps the line when the controls are not pinned", () => {
    show(
      panel((s) => {
        offline(s);
        s.hud.widgets = s.hud.widgets.filter((w) => w !== "CONTROLS");
      }),
    );
    expect(screen.getByTestId("hud-offline").closest(".hud-status-row")).not.toBeNull();
  });
});

describe("HUD DUMP tooltip", () => {
  it("says why DUMP is off", () => {
    expect(dumpTitle(panel())).toBe("The belt is already stopped.");
    expect(dumpTitle(panel((s) => (s.deck.status = "Stopping")))).toBe("The belt is already stopping.");
    expect(dumpTitle(panel(offline))).toBe("Treadmill not connected.");
    expect(dumpTitle(panel((s) => (s.deck.status = "Running")))).toBe("Stop the belt and end the session.");
  });

  it("gives the running strip's DUMP a tooltip too", () => {
    show(
      panel((s) => {
        s.deck.status = "Running";
        s.hud_view = "minified";
      }),
    );
    expect(screen.getByRole("button", { name: "DUMP end walk" })).toHaveAttribute("title", "Stop the belt and end the session.");
  });
});

describe("HUD update card", () => {
  it("names the button for opening the notes, and the line agrees", () => {
    const s = panel((x) => {
      x.update = { ...x.update, hud_card: true, offer: { version: "1.3.0", notes: "", published: null } };
    });
    render(<UpdateCard snapshot={s} />);
    expect(screen.getByRole("button", { name: "NOTES" })).toHaveAttribute("title", "Open the release notes in the dashboard.");
    expect(screen.getByText("Read the release notes in the dashboard, then press Install.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "INSTALL" })).not.toBeInTheDocument();
  });
});
