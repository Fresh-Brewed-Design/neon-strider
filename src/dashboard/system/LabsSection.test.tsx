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

describe("System -> Labs typing by speed", () => {
  const TYPING = "Count key presses by walking speed";

  it("is off by default, shows no table and says what off means", () => {
    render(<LabsSection />);
    expect(screen.getByRole("switch", { name: TYPING })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("Switched off, nothing is counted or saved.")).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("asks Rust to switch it on", async () => {
    render(<LabsSection />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: TYPING })));
    expect(dispatched()).toEqual([{ command: { type: "typing_speed_set", on: true } }]);
  });

  it("shows the empty state when no walk has counts", () => {
    current!.typing_speed = { ...current!.typing_speed, enabled: true };
    render(<LabsSection />);
    expect(screen.getByText("No walks with key counts saved yet.")).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("shows each band's minutes and median", () => {
    current!.typing_speed = {
      ...current!.typing_speed,
      enabled: true,
      walks: 3,
      typing_minutes: 20,
      bands: [{ from_kmh: 3, to_kmh: 3.5, minutes: 14, median_kpm: 162 }],
    };
    render(<LabsSection />);
    expect(screen.getByRole("table")).toBeTruthy();
    expect(screen.getByText("3.0 to 3.5 km/h")).toBeTruthy();
    expect(screen.getByText("14")).toBeTruthy();
    expect(screen.getByText("162")).toBeTruthy();
    expect(screen.getByText("Medians over your last 3 walks with key counts.")).toBeTruthy();
  });
});
