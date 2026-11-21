import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { LabsSection } = await import("./LabsSection");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);
const TILT = "ZONE_TILT: Let heart zones trim the incline";

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Labs ZONE_TILT", () => {
  it("is off by default and says what off means", () => {
    render(<LabsSection />);
    expect(screen.getByRole("switch", { name: TILT })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("Switched off, every heart zone trims the speed only.")).toBeTruthy();
  });

  it("asks Rust to switch it on", async () => {
    render(<LabsSection />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: TILT })));
    expect(dispatched()).toEqual([{ command: { type: "heart_rate_incline_trim_set", on: true } }]);
  });

  it("shows Rust's state", () => {
    current!.heart_rate = { ...current!.heart_rate, incline_trim: true };
    render(<LabsSection />);
    expect(screen.getByRole("switch", { name: TILT })).toHaveAttribute("aria-checked", "true");
  });
});
