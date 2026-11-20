import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MergedSession } from "../../services/sessionMerge";
import { SessionRow } from "./SessionRow";

function walk(extra: Partial<MergedSession> = {}): MergedSession {
  const start = new Date(2026, 10, 19, 9, 0).getTime();
  return {
    id: 1,
    mergedIds: [1],
    start_time: start,
    end_time: start + 1_800_000,
    duration_seconds: 1800,
    distance_meters: 1800,
    raw_distance_meters: 1800,
    calories: 90,
    steps: 0,
    raw_steps: 0,
    avg_speed: 3.6,
    raw_avg_speed: 3.6,
    avg_incline: 0,
    strava_id: null,
    neural_heat: 0,
    neural_ticks: 0,
    last_hardware_time: 0,
    avg_heart_rate: null,
    max_heart_rate: null,
    ...extra,
  };
}

function row(session: MergedSession) {
  return render(
    <SessionRow
      session={session}
      expanded
      stravaLinked={false}
      uploadState="idle"
      onToggle={vi.fn()}
      onUpload={vi.fn()}
      onSave={vi.fn()}
      onOpenFolder={vi.fn()}
      onMarkUploaded={vi.fn()}
      onInspect={vi.fn()}
      onDelete={vi.fn()}
    />,
  );
}

describe("SessionRow heart rate", () => {
  it("shows the average and maximum for a walk saved with a sensor", () => {
    row(walk({ avg_heart_rate: 112, max_heart_rate: 131 }));
    expect(screen.getByText("Avg heart rate")).toBeTruthy();
    expect(screen.getByText("112 bpm")).toBeTruthy();
    expect(screen.getByText("Max heart rate")).toBeTruthy();
    expect(screen.getByText("131 bpm")).toBeTruthy();
  });

  it("shows no heart rate for a walk without a sensor", () => {
    const { container } = row(walk());
    expect(container.textContent).not.toMatch(/heart rate|bpm/i);
  });
});
