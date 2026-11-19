import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { SeatedCard } = await import("./HudCards");

/** The mock, idle for 63 minutes with the belt still, then `patch`. */
function seated(patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.seated = { ...s.seated, show_card: true, seated_minutes: 63, program_ids: s.programs.slice(0, 3).map((p) => p.id) };
  patch(s);
  return s;
}

const offline = (s: AppSnapshot) => (s.connection.connected = false);
const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);
const buttons = () => screen.getAllByRole("button").map((b) => b.textContent);

beforeEach(() => vi.mocked(invoke).mockClear());

describe("SeatedCard", () => {
  it("says the belt has been idle, never that you are sitting", () => {
    const { container } = render(<SeatedCard snapshot={seated()} />);
    expect(screen.getByText("63 MIN IDLE")).toBeInTheDocument();
    expect(container.textContent).not.toMatch(/seat|sitting|sit|sat/i);
  });

  it("puts ENGAGE first, then the programs in the configured order, then one way to hide it", () => {
    const s = seated((x) => (x.seated.program_ids = [3, 1]));
    render(<SeatedCard snapshot={s} />);
    const names = (id: number) => s.programs.find((p) => p.id === id)!.name;
    expect(buttons()).toEqual(["ENGAGE", names(3), names(1), "SNOOZE 15 MIN"]);
    expect(screen.getByText("63 MIN IDLE")).toBeInTheDocument();
    expect(screen.queryByText(/DISMISS|LATER/)).not.toBeInTheDocument();
  });

  it("each button sends one command, ENGAGE its own", async () => {
    const s = seated();
    render(<SeatedCard snapshot={s} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ENGAGE" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: s.programs[0].name })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "SNOOZE 15 MIN" })));
    expect(dispatched()).toEqual([
      { command: { type: "seated_engage" } },
      { command: { type: "seated_start", id: s.programs[0].id } },
      { command: { type: "seated_snooze" } },
    ]);
  });

  it("with no programs, still offers ENGAGE", () => {
    render(<SeatedCard snapshot={seated((x) => (x.seated.program_ids = []))} />);
    expect(buttons()).toEqual(["ENGAGE", "SNOOZE 15 MIN"]);
    expect(screen.getByRole("button", { name: "ENGAGE" })).toBeEnabled();
  });

  it("with the treadmill off, ENGAGE waits and the line says why; programs can still be queued", () => {
    render(<SeatedCard snapshot={seated(offline)} />);
    expect(screen.getByRole("button", { name: "ENGAGE" })).toBeDisabled();
    expect(screen.getByText("Treadmill off. Switch it on, or queue a program.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: mock.programs[0].name })).toBeEnabled();
  });

  it("with the treadmill off and no programs, says to switch it on", () => {
    render(<SeatedCard snapshot={seated((x) => (offline(x), (x.seated.program_ids = [])))} />);
    expect(screen.getByText("Treadmill off. Switch it on to engage.")).toBeInTheDocument();
  });

  it("says nothing extra while linked", () => {
    const { container } = render(<SeatedCard snapshot={seated()} />);
    expect(container.querySelector(".hud-sub")).toBeNull();
  });

  it("marks a queued program and says when it starts", () => {
    const s = seated((x) => (offline(x), (x.seated.pending_program_id = x.programs[1].id)));
    render(<SeatedCard snapshot={s} />);
    expect(screen.getByRole("button", { name: s.programs[1].name })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(`${s.programs[1].name} starts when the treadmill is on.`)).toBeInTheDocument();
  });

  it("shows a refused start (deck asleep) right under the buttons", () => {
    const message = "Treadmill is off or asleep. Switch it on, then press Engage.";
    render(
      <SeatedCard
        snapshot={seated((x) => {
          x.deck.standby = true;
          x.connection.start_result = { ok: false, message, at_ms: 1 };
        })}
      />,
    );
    const card = screen.getAllByRole("alert")[0];
    expect(within(card).getByText(message)).toBeInTheDocument();
    // The refusal replaces the standing hint rather than repeating it.
    expect(screen.queryByText("Treadmill off. Switch it on to engage.")).not.toBeInTheDocument();
  });

  it("says the deck is off while it sleeps, before any press", () => {
    render(<SeatedCard snapshot={seated((x) => (x.deck.standby = true))} />);
    expect(screen.getByText("Treadmill off. Switch it on to engage.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "ENGAGE" })).toBeEnabled();
  });

  it("shows a rejected ENGAGE inside the card", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("The treadmill is offline");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    render(<SeatedCard snapshot={seated()} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ENGAGE" })));
    expect(screen.getByText("SYS // THE TREADMILL IS OFFLINE")).toBeInTheDocument();
  });
});
