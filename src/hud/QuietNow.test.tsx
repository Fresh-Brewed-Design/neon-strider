import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { Controls, QuietNowLine } = await import("./HudPanel");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

/** The mock walking at 3.5 km/h, with Quiet now holding it at 2.0 or not. */
function snap(on: boolean): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Running", target_speed_kmh: on ? 2 : 3.5 };
  s.calls = { ...s.calls, quiet_kmh: 2, quiet_now: on, quiet_now_back_kmh: on ? 3.5 : null };
  return s;
}

describe("Quiet now on the HUD", () => {
  it("offers QUIET beside DUMP and sends one quiet_now per press", async () => {
    vi.mocked(invoke).mockClear();
    render(<Controls snapshot={snap(false)} />);
    const button = screen.getByRole("button", { name: "QUIET slow down" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(button).toHaveAttribute("title", "Ease the belt to 2.0 km/h.");
    await act(async () => fireEvent.click(button));
    expect(dispatched()).toEqual([{ command: { type: "quiet_now" } }]);
  });

  it("reads as held while Quiet now holds the belt, naming the speed a second press puts back", () => {
    render(<Controls snapshot={snap(true)} />);
    const button = screen.getByRole("button", { name: "QUIET back to 3.5" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveClass("is-on");
    expect(button).toHaveAttribute("title", "Put 3.5 km/h back.");
  });

  it("is off while no treadmill is connected", () => {
    const s = snap(false);
    s.connection = { ...s.connection, connected: false };
    render(<Controls snapshot={s} />);
    expect(screen.getByRole("button", { name: "QUIET slow down" })).toBeDisabled();
  });

  it("shows Rust's refusal under the controls", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("The belt already runs at 2.0 km/h or slower.");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    render(<Controls snapshot={snap(false)} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "QUIET slow down" })));
    expect(screen.getByRole("alert")).toHaveTextContent("SYS // THE BELT ALREADY RUNS AT 2.0 KM/H OR SLOWER");
  });

  it("shows the QUIET line with the held and the restore speed while on", () => {
    const { unmount } = render(<QuietNowLine snapshot={snap(true)} />);
    const line = screen.getByTestId("hud-quiet");
    expect(line).toHaveTextContent("QUIET // HOLD");
    expect(line).toHaveTextContent("2.0/3.5 km/h");
    expect(line).toHaveAttribute("title", "QUIET puts 3.5 km/h back.");
    unmount();
    render(<QuietNowLine snapshot={snap(false)} />);
    expect(screen.queryByTestId("hud-quiet")).toBeNull();
  });
});

describe("Quiet now in the mocked backend", () => {
  /** The scenario's handler, as the previews dispatch it; a refusal comes back as its text. */
  async function press(): Promise<string | null> {
    const { calls } = await import("../dev/scenarios/calls");
    try {
      calls.commands?.quiet_now?.({ type: "quiet_now" });
      return null;
    } catch (e) {
      return String(e);
    }
  }

  it("eases a running belt, puts the speed back, and refuses a slow or still belt", async () => {
    const { snapshot } = await import("../dev/mockState");
    const before = structuredClone(snapshot);
    try {
      Object.assign(snapshot.deck, { status: "Running", target_speed_kmh: 3.5 });
      Object.assign(snapshot.calls, { quiet_kmh: 2, quiet_now: false, quiet_now_back_kmh: null });
      expect(await press()).toBeNull();
      expect(snapshot.calls.quiet_now_back_kmh).toBe(3.5);
      expect(snapshot.deck.target_speed_kmh).toBe(2);
      expect(await press()).toBeNull();
      expect(snapshot.calls.quiet_now).toBe(false);
      expect(snapshot.deck.target_speed_kmh).toBe(3.5);
      snapshot.deck.target_speed_kmh = 1.8;
      expect(await press()).toBe("The belt already runs at 2.0 km/h or slower.");
      snapshot.deck.status = "Stopped";
      expect(await press()).toBe("Quiet now works on a running belt.");
    } finally {
      Object.assign(snapshot.deck, before.deck);
      Object.assign(snapshot.calls, before.calls);
    }
  });
});
