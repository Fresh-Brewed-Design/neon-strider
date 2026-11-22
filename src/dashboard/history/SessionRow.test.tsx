import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { EchoProgram } from "../../ipc";
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

function row(
  session: MergedSession,
  {
    live = false,
    onEcho = vi.fn<() => Promise<EchoProgram>>(),
    onOpenProgram = vi.fn<(id: number) => void>(),
  }: {
    live?: boolean;
    onEcho?: () => Promise<EchoProgram>;
    onOpenProgram?: (id: number) => void;
  } = {},
) {
  return render(
    <SessionRow
      session={session}
      expanded
      stravaLinked={false}
      uploadState="idle"
      live={live}
      onToggle={vi.fn()}
      onUpload={vi.fn()}
      onSave={vi.fn()}
      onOpenFolder={vi.fn()}
      onMarkUploaded={vi.fn()}
      onInspect={vi.fn()}
      onDelete={vi.fn()}
      onEcho={onEcho}
      onOpenProgram={onOpenProgram}
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

describe("SessionRow ECHO", () => {
  const saved: EchoProgram = { id: 7, name: "ECHO // 2026-11-19", step_count: 3 };

  it("saves the walk, says where, and opens the new program on request", async () => {
    let finish: (p: EchoProgram) => void = () => {};
    const onEcho = vi.fn(() => new Promise<EchoProgram>((r) => (finish = r)));
    const onOpenProgram = vi.fn();
    row(walk(), { onEcho, onOpenProgram });
    const button = screen.getByRole("button", { name: "ECHO" });
    fireEvent.click(button);
    expect(onEcho).toHaveBeenCalledOnce();
    expect(screen.getByText("Building the ECHO program…")).toBeTruthy();
    expect((button as HTMLButtonElement).disabled).toBe(true);
    await act(async () => finish(saved));
    expect(screen.getByText("Saved ECHO // 2026-11-19 to Programs.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open in Programs" }));
    expect(onOpenProgram).toHaveBeenCalledWith(7);
  });

  it("shows Rust's message when the walk gives no program", async () => {
    const onEcho = vi.fn(() => Promise.reject("ECHO needs a walk of at least 3 minutes."));
    row(walk(), { onEcho });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ECHO" })));
    expect(screen.getByRole("alert").textContent).toContain("ECHO needs a walk of at least 3 minutes.");
    expect(screen.queryByRole("button", { name: "Open in Programs" })).toBeNull();
  });

  it("waits while the walk is still recording", () => {
    const onEcho = vi.fn<() => Promise<EchoProgram>>();
    row(walk(), { live: true, onEcho });
    const button = screen.getByRole("button", { name: "ECHO" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.title).toBe("ECHO waits for the walk to end.");
  });
});
