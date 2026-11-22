import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../../state/useAppState", () => ({
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../../dev/mockBackend");
const { StrideFit } = await import("./StrideFit");

const sent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: { type: string; kmh?: number | null } }).command)
    .filter((c) => c.type === "stride_fit_set");

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("StrideFit", () => {
  it("saves a typed speed for Rust to keep", async () => {
    current!.deck.stride_fit_kmh = null;
    render(<StrideFit />);
    expect(screen.queryByRole("button", { name: "CLEAR" })).toBeNull();
    fireEvent.change(screen.getByLabelText("STRIDE_FIT speed in km/h"), { target: { value: "5.4" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "SAVE" })));
    expect(sent()).toEqual([{ type: "stride_fit_set", kmh: 5.4 }]);
  });

  it("fills in the belt's speed with USE CURRENT and clears a saved speed", async () => {
    current!.deck.stride_fit_kmh = 5;
    current!.deck.speed_kmh = 4.8;
    render(<StrideFit />);
    const field = screen.getByLabelText("STRIDE_FIT speed in km/h") as HTMLInputElement;
    expect(field.value).toBe("5.0");
    fireEvent.click(screen.getByRole("button", { name: "USE CURRENT" }));
    expect(field.value).toBe("4.8");
    expect(sent()).toEqual([]);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "CLEAR" })));
    expect(sent()).toEqual([{ type: "stride_fit_set", kmh: null }]);
  });
});
