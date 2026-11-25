import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AppSnapshot } from "../../ipc";

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
    expect(screen.getByRole("button", { name: "Re-jack resume" })).toBeInTheDocument();
    paused.unmount();
    show((s) => (s.deck = { ...s.deck, status: "Running", speed_kmh: 3 }));
    expect(screen.getByRole("button", { name: "Stasis pause" })).toBeInTheDocument();
  });

  it("shows the STASIS clock under Re-jack while paused", () => {
    const paused = show((s) => (s.deck = { ...s.deck, status: "Paused", speed_kmh: 0, paused_for_s: 3909 }));
    expect(screen.getByRole("button", { name: "Re-jack resume STASIS\u00a01:05:09" })).toBeInTheDocument();
    paused.unmount();
    show((s) => (s.deck = { ...s.deck, status: "Running", speed_kmh: 3, paused_for_s: null }));
    expect(screen.queryByText(/STASIS\u00a0/)).toBeNull();
  });

  it("reads GO with resume on a paused program", () => {
    show((s) => {
      s.deck = { ...s.deck, status: "Paused", speed_kmh: 0 };
      s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null };
    });
    expect(screen.getByRole("button", { name: "GO resume" })).toBeInTheDocument();
  });
});
