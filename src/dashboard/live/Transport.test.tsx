import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot, StartPress } from "../../ipc";

const { snapshot: mock } = await import("../../dev/mockBackend");
const { Transport } = await import("./Transport");

function show(patch: (s: AppSnapshot) => void) {
  const s = structuredClone(mock);
  s.active_program = null;
  patch(s);
  return render(<Transport s={s} />);
}

describe("Transport belt buttons", () => {
  it("say what each does in plain words under the codename", () => {
    const stopped = show((s) => (s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 }));
    expect(screen.getByRole("button", { name: "Engage start" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dump end walk" })).toBeInTheDocument();
    stopped.unmount();
    const paused = show((s) => (s.deck = { ...s.deck, status: "Paused", speed_kmh: 0 }));
    expect(screen.getByRole("button", { name: "Re-jack resume belt" })).toBeInTheDocument();
    paused.unmount();
    show((s) => (s.deck = { ...s.deck, status: "Running", speed_kmh: 3 }));
    expect(screen.getByRole("button", { name: "Stasis pause belt" })).toBeInTheDocument();
  });

  it("keeps Re-jack's plain line while the STASIS clock counts (Velocity has the clock)", () => {
    show((s) => (s.deck = { ...s.deck, status: "Paused", speed_kmh: 0, paused_for_s: 3909 }));
    expect(screen.getByRole("button", { name: "Re-jack resume belt" })).toBeInTheDocument();
    expect(screen.queryByText("1h05")).toBeNull();
  });

  it("reads GO with resume on a paused program", () => {
    show((s) => {
      s.deck = { ...s.deck, status: "Paused", speed_kmh: 0 };
      s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null };
    });
    expect(screen.getByRole("button", { name: "GO resume program" })).toBeInTheDocument();
  });

  // Colour roles (#391): STASIS is cyan like ENGAGE; DUMP has its own magenta outline and stop square.
  it("keeps STASIS cyan and gives DUMP its own look", () => {
    show((s) => (s.deck = { ...s.deck, status: "Running", speed_kmh: 3 }));
    const stasis = screen.getByRole("button", { name: "Stasis pause belt" });
    expect(stasis.className).toContain("text-neon-cyan");
    expect(stasis.className).not.toContain("magenta");
    const dump = screen.getByRole("button", { name: "Dump end walk" });
    expect(dump.className).toContain("border-2");
    expect(dump.className).toContain("border-neon-magenta");
    expect(dump.querySelector(".bg-current")).not.toBeNull();
  });

  // #385: a start a program's GO (or the HUD's sit-too-long card) made shows under that control, so
  // Engage never repeats it.
  it("shows its own start failure but not one a program's GO or the HUD card made", () => {
    const failed = (s: AppSnapshot, press: StartPress) => {
      s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
      s.connection = {
        ...s.connection,
        start_pending: false,
        start_press: press,
        start_result: { ok: false, message: "The treadmill didn't respond.", at_ms: 1 },
      };
    };
    const own = show((s) => failed(s, "engage"));
    expect(screen.getByText("The treadmill didn't respond.")).toBeInTheDocument();
    own.unmount();
    for (const press of ["program_go", "seated_engage"] as const) {
      const other = show((s) => failed(s, press));
      expect(screen.queryByText("The treadmill didn't respond.")).toBeNull();
      other.unmount();
    }
  });

  it("leaves the waiting line to GO while GO's start is in flight", () => {
    show((s) => {
      s.deck = { ...s.deck, status: "Paused", speed_kmh: 0 };
      s.connection = { ...s.connection, start_pending: true, start_press: "program_go", start_result: null };
    });
    expect(screen.queryByText("Waiting for the treadmill to start…")).toBeNull();
    // Still held, so a second start is never sent over GO's.
    expect(screen.getByRole("button", { name: "Re-jack resume belt" })).toBeDisabled();
  });

  it("gives the offline line at least the belt buttons' height, so nothing under it jumps", () => {
    show((s) => (s.connection = { ...s.connection, connected: false }));
    expect(screen.getByText("Connect").closest(".hacker-panel")).toHaveClass("min-h-12");
  });
});
