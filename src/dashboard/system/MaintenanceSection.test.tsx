import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { MaintenanceSection } = await import("./MaintenanceSection");
const { MaintenanceCard } = await import("../../hud/HudCards");
const { MaintenanceBanner } = await import("./MaintenanceBanner");
const { useTreadmillStore } = await import("../../store/treadmillStore");

function due(): AppSnapshot {
  const s = structuredClone(mock);
  s.maintenance = { ...s.maintenance, distance_since_km: 203.4, due: true, progress: 1.02 };
  return s;
}

describe("maintenance texts", () => {
  it("shows the default interval in km and quotes the sources", () => {
    current = structuredClone(mock);
    render(<MaintenanceSection />);
    expect(screen.getByText(/every 200 km or 3 months/)).toBeTruthy();
    expect(screen.getByText("/ 200 km")).toBeTruthy();
    expect(screen.getByText("Deck sticker: 200 km (125 miles). Jogwell's manual: 100 miles.")).toBeTruthy();
    expect(screen.getByText("km")).toBeTruthy();
    expect(screen.queryByText(/\bmi\b/)).toBeNull();
  });

  it("HUD card counts distance in km", () => {
    render(<MaintenanceCard snapshot={due()} />);
    expect(screen.getByText(/203 KM · 38 DAYS SINCE LAST/)).toBeTruthy();
  });
});

describe("maintenance status lines", () => {
  it("shows a refused Mark serviced under the System button", async () => {
    current = structuredClone(mock);
    render(<MaintenanceSection />);
    fireEvent.click(screen.getByRole("button", { name: "Mark serviced" }));
    vi.mocked(invoke).mockRejectedValueOnce("Database is busy");
    await act(async () => useTreadmillStore.getState().pendingConfirmation!.onConfirm());
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("dispatch", { command: { type: "maintenance_mark_serviced" } });
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Database is busy");
    expect(screen.getByRole("button", { name: "Mark serviced" }).nextElementSibling).toBe(alert);
  });

  it("shows a refused Mark serviced under the Live banner", async () => {
    current = due();
    render(<MaintenanceBanner />);
    fireEvent.click(screen.getByRole("button", { name: "Mark serviced" }));
    vi.mocked(invoke).mockRejectedValueOnce("Database is busy");
    await act(async () => useTreadmillStore.getState().pendingConfirmation!.onConfirm());
    expect(screen.getByRole("alert")).toHaveTextContent("Database is busy");
  });

  it("shows a refused basis under the basis buttons", async () => {
    current = structuredClone(mock);
    render(<MaintenanceSection />);
    vi.mocked(invoke).mockRejectedValueOnce("Database is busy");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Belt hours + days" })));
    expect(screen.getByRole("group", { name: "Count service by" }).nextElementSibling).toHaveTextContent("Database is busy");
  });
});
