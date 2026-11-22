import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, FitToDeckOffer } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { useTreadmillStore } = await import("../../store/treadmillStore");
const { ProgramsPage } = await import("../pages/ProgramsPage");

const sent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, args]) => (args as { command: { type: string; id?: number } }).command);

/** Rust's offer for Interval walk (id 7) at STRIDE_FIT 4.5 km/h. */
const OFFER: FitToDeckOffer = {
  program_id: 7,
  summary: "5 steps run above STRIDE_FIT 4.5 km/h.",
  message: "FIT_TO_DECK sets 5 steps of Interval walk to STRIDE_FIT 4.5 km/h.\n6.0 km/h at 5.0% becomes 4.5 km/h at 8.5%.",
  hint: "The new incline keeps the same effort by the ACSM walking equation.\nFIT_TO_DECK never starts the program.",
  done: "Interval walk now runs at STRIDE_FIT 4.5 km/h with more incline.",
};

function show(offers: FitToDeckOffer[]) {
  const s = structuredClone(mock);
  s.active_program = null;
  s.fit_to_deck = offers;
  current = s;
  useTreadmillStore.setState({ pendingConfirmation: null });
  return render(<ProgramsPage />);
}

beforeEach(() => vi.mocked(invoke).mockClear());

describe("FIT_TO_DECK", () => {
  it("shows only on a program Rust offers it for", () => {
    show([OFFER]);
    expect(screen.getAllByText("FIT_TO_DECK")).toHaveLength(1);
    expect(screen.getByText(OFFER.summary)).toBeInTheDocument();
  });

  it("is hidden while Rust offers none", () => {
    show([]);
    expect(screen.queryByText("FIT_TO_DECK")).toBeNull();
  });

  it("asks first with Rust's words and saves only on confirm, never running the program", async () => {
    show([OFFER]);
    fireEvent.click(screen.getByText("FIT_TO_DECK"));
    const ask = useTreadmillStore.getState().pendingConfirmation;
    expect(ask?.title).toBe("FIT_TO_DECK");
    expect(ask?.message).toBe(OFFER.message);
    expect(ask?.hint).toBe(OFFER.hint);
    expect(sent()).toEqual([]);
    await act(async () => ask?.onConfirm());
    expect(sent()).toEqual([{ type: "program_fit_to_deck", id: 7 }]);
    expect(await screen.findByText(OFFER.done)).toBeInTheDocument();
  });

  it("cancelled, saves nothing", () => {
    show([OFFER]);
    fireEvent.click(screen.getByText("FIT_TO_DECK"));
    useTreadmillStore.getState().clearConfirmation();
    expect(sent()).toEqual([]);
  });
});
