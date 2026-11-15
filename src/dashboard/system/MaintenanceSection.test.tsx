import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { MaintenanceSection } = await import("./MaintenanceSection");
const { MaintenanceCard } = await import("../../hud/HudCards");

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
