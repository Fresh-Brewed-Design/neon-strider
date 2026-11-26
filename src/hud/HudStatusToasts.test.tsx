import { act, render, screen } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";
import type { ToastPayload } from "./useToasts";

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
  vi.mocked(listen).mockImplementation(() => Promise.resolve(() => {}));
});

/** The belt still, no program, in `view`, then `patch`. */
function still(view: AppSnapshot["hud_view"], patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.hud = { ...s.hud, idle_mode: view === "strip" || view === "hidden" ? view : "full" };
  s.hud_view = view;
  patch(s);
  return s;
}

/** Render the HUD and send it `payloads` as Rust's `hud-update` events, in order. */
async function toasts(s: AppSnapshot, payloads: ToastPayload[]) {
  let handler: ((e: { payload: ToastPayload }) => void) | undefined;
  vi.mocked(listen).mockImplementation((event, cb) => {
    if (event === "hud-update") handler = cb as typeof handler;
    return Promise.resolve(() => {});
  });
  current = s;
  render(<HudApp />);
  await act(async () => {
    for (const payload of payloads) handler?.({ payload });
  });
  expect(handler).toBeDefined();
}

describe("HUD status toasts", () => {
  it("drop the belt's state and LINKED while the panel's header shows them, in every view", async () => {
    for (const view of ["full", "strip", "minified"] as const) {
      await toasts(still(view), [
        { type: "LINK", message: "LINKED" },
        { type: "DECK", message: "STASIS" },
      ]);
      expect(screen.queryByText("SYS // DECK")).not.toBeInTheDocument();
      expect(screen.queryByText("SYS // LINK")).not.toBeInTheDocument();
      document.body.innerHTML = "";
    }
  });

  it("keep SESSION SAVED when a state change follows it (each has its own slot)", async () => {
    const saved = "SESSION SAVED · 10:00 · 0.67 km · 4.0 km/h · TODAY 0.67 km";
    await toasts(still("strip"), [
      { type: "STATUS", message: saved, isActionable: true },
      { type: "DECK", message: "STANDBY" },
    ]);
    expect(screen.getByText("SYS // STATUS")).toBeInTheDocument();
    expect(document.querySelector(".hud-toast-message")).toHaveTextContent(/^SESSION SAVED/);
    expect(screen.queryByText("STANDBY", { selector: ".hud-toast *" })).not.toBeInTheDocument();
  });

  it("show the state in the app's words while no panel shows", async () => {
    await toasts(still("hidden"), [
      { type: "LINK", message: "LINKED" },
      { type: "DECK", message: "RE-JACK" },
    ]);
    expect(screen.getByText("SYS // LINK")).toBeInTheDocument();
    expect(screen.getByText("LINKED")).toBeInTheDocument();
    expect(screen.getByText("SYS // DECK")).toBeInTheDocument();
    expect(screen.getByText("RE-JACK")).toBeInTheDocument();
  });

  it("say on a program toast that the deck must be woken first while it sleeps", async () => {
    const asleep = still("strip", (s) => {
      s.deck.standby = true;
      s.active_program = { ...structuredClone(mock.active_program!), name: "Workday", phase: "resting", seconds_remaining: 30 };
    });
    await toasts(asleep, [{ type: "AUTOMATION", message: "WORKDAY IN 30S", isActionable: true }]);
    const wake = screen.getByTestId("hud-toast-wake");
    expect(wake).toHaveTextContent("wake deck first");
    expect(wake).toHaveAttribute("title", "Wake the deck first with the treadmill's power button or remote.");
    expect(screen.getByRole("button", { name: "SKIP REST" })).toBeInTheDocument();
  });

  it("add no wake line while the deck is awake", async () => {
    await toasts(still("strip"), [{ type: "AUTOMATION", message: "WORKDAY IN 30S", isActionable: true }]);
    expect(screen.queryByTestId("hud-toast-wake")).not.toBeInTheDocument();
  });
});
