import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { Controls, QuietNowLine } = await import("./HudPanel");
const { NotePop } = await import("./HudNote");
const { useHudSend } = await import("./useHudSend");

/** The controls with the panel's note, as HudApp wires them. */
function Panel({ snapshot }: { snapshot: AppSnapshot }) {
  const { send, error } = useHudSend();
  return (
    <>
      <NotePop note={error ? { text: error } : null} />
      <Controls snapshot={snapshot} send={send} />
    </>
  );
}

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
    render(<Panel snapshot={snap(false)} />);
    const button = screen.getByRole("button", { name: "QUIET slow down" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(button).toHaveAttribute("title", "Ease the belt to 2.0 km/h.");
    await act(async () => fireEvent.click(button));
    expect(dispatched()).toEqual([{ command: { type: "quiet_now" } }]);
  });

  it("reads as held while Quiet now holds the belt, naming the speed a second press puts back", () => {
    render(<Panel snapshot={snap(true)} />);
    const button = screen.getByRole("button", { name: "QUIET back 3.5" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveClass("is-on");
    expect(button).toHaveAttribute("title", "QUIET holds the belt at 2.0 km/h. Press again for 3.5 km/h.");
  });

  it("is off while no treadmill is connected", () => {
    const s = snap(false);
    s.connection = { ...s.connection, connected: false };
    render(<Panel snapshot={s} />);
    expect(screen.getByRole("button", { name: "QUIET slow down" })).toBeDisabled();
  });

  it("shows Rust's refusal in the panel's note", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("The belt already runs at 2.0 km/h or slower.");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    render(<Panel snapshot={snap(false)} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "QUIET slow down" })));
    expect(screen.getByRole("alert")).toHaveTextContent("SYS // THE BELT ALREADY RUNS AT 2.0 KM/H OR SLOWER");
  });

  it("shows the QUIET line with the held and the restore speed while on", () => {
    const { unmount } = render(<QuietNowLine snapshot={snap(true)} />);
    const line = screen.getByTestId("hud-quiet");
    expect(line).toHaveTextContent("QUIET // DAMPED");
    expect(line).toHaveTextContent("2.0/3.5 km/h");
    expect(line).toHaveAttribute("title", "QUIET puts 3.5 km/h back.");
    unmount();
    render(<QuietNowLine snapshot={snap(false)} />);
    expect(screen.queryByTestId("hud-quiet")).toBeNull();
  });
});

/** The mock on a call: the belt held at 2.0 km/h, 3.5 km/h back once the call ends. */
function onCall(): AppSnapshot {
  const s = snap(false);
  s.deck = { ...s.deck, target_speed_kmh: 2 };
  s.calls = { ...s.calls, mic_in_use: true, quiet_active: true, call_back_kmh: 3.5 };
  return s;
}

describe("QUIET during a call", () => {
  it("lights QUIET and says a press keeps the belt slow after the call", () => {
    render(<Panel snapshot={onCall()} />);
    const button = screen.getByRole("button", { name: "QUIET keep slow" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveClass("is-on");
    expect(button).toHaveAttribute("title", "A call holds the belt at 2.0 km/h. Press to keep 2.0 km/h after the call.");
  });

  it("fills the status row like a pressed QUIET, with the speed the call's end puts back", () => {
    render(<QuietNowLine snapshot={onCall()} />);
    const line = screen.getByTestId("hud-quiet");
    expect(line).toHaveTextContent("QUIET // DAMPED");
    expect(line).toHaveTextContent("2.0/3.5 km/h");
    expect(line).toHaveAttribute("title", "After a call, the belt goes back to 3.5 km/h.");
  });

  it("keeps QUIET one size whether it is off, on a call or holding the belt", () => {
    const words = (s: AppSnapshot) => {
      const { container, unmount } = render(<Panel snapshot={s} />);
      const all = Array.from(container.querySelectorAll(".hud-btn-quiet .hud-swap > span")).map((e) => e.textContent);
      unmount();
      return all.filter((w) => !w?.startsWith("back ") || w === "back 00.0");
    };
    expect(words(snap(false))).toEqual(words(onCall()));
    expect(words(snap(true))).toEqual(words(onCall()));
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
