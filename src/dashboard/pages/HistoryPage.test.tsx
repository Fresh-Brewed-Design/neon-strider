import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, HistoryStats } from "../../ipc";
import type { Session } from "../../services/db/schema";
import { formatDayHeading } from "../../format";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { HistoryPage } = await import("./HistoryPage");

const DAY = 86_400_000;

function walk(id: number, start: number): Session {
  return {
    id, start_time: start, end_time: start + 1_800_000, duration_seconds: 1800,
    distance_meters: 2000, raw_distance_meters: 2000, calories: 100, steps: 0, raw_steps: 0,
    avg_speed: 4, raw_avg_speed: 4, avg_incline: 0, strava_id: null, neural_heat: 0, neural_ticks: 0,
    last_hardware_time: 0, avg_heart_rate: null, max_heart_rate: null, program_name: null,
  };
}

const now = new Date();
const RECENT = [walk(2, now.getTime() - 2 * DAY)];
// Well before the three listed months.
const OLD_START = new Date(now.getFullYear() - 1, now.getMonth(), 10, 9).getTime();
const OLD = [walk(1, OLD_START)];
const OLD_FROM = new Date(now.getFullYear() - 1, now.getMonth() - 2, 1).getTime();

function stats(oldest: number | null): HistoryStats {
  return {
    totals: { meters: 2_400_000, seconds: 2_160_000, kcal: 120_000, sessions: 1200 },
    meters_by_day: [],
    oldest_start_ms: oldest,
  };
}

function backend(older: () => unknown, oldest: number | null = OLD_START) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "history_sessions") return RECENT;
    if (cmd === "history_stats") return stats(oldest);
    if (cmd === "history_older") return older();
    return null;
  });
}

function show() {
  const s = structuredClone(mock);
  current = s;
  return render(<HistoryPage />);
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe("History beyond the listed months", () => {
  it("shows Rust's totals over every walk, not only the listed ones", async () => {
    backend(() => ({ sessions: [], from_ms: null }));
    show();
    expect(await screen.findByText("1200")).toBeInTheDocument();
    expect(screen.getByText("2400.0")).toBeInTheDocument();
  });

  it("names the range the totals cover", async () => {
    backend(() => ({ sessions: [], from_ms: null }));
    show();
    expect(await screen.findByText("Last 7 days")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "30 days" }));
    expect(await screen.findByText("Last 30 days")).toBeInTheDocument();
  });

  it("leads with one line and no zero cards when nothing was ever walked", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "history_sessions") return [];
      if (cmd === "history_stats") return { ...stats(null), totals: { meters: 0, seconds: 0, kcal: 0, sessions: 0 } };
      return null;
    });
    show();
    expect(await screen.findByTestId("history-empty")).toHaveTextContent("No walks yet.");
    expect(screen.queryByText("Last 7 days")).toBeNull();
    expect(screen.queryByText("Past year")).toBeNull();
  });

  it("lists older months on demand, with the result under the button", async () => {
    backend(() => ({ sessions: OLD, from_ms: OLD_FROM }));
    show();
    fireEvent.click(await screen.findByRole("radio", { name: "All" }));
    fireEvent.click(await screen.findByRole("button", { name: "Load older walks" }));
    const older = screen.getByTestId("history-older");
    const month = new Date(OLD_FROM).toLocaleDateString([], { month: "long", year: "numeric" });
    expect(await within(older).findByText("Loaded 1 older walk.")).toBeInTheDocument();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("history_older", { beforeMs: expect.any(Number) as number });
    expect(within(older).getByText(`Walks listed back to ${month}.`)).toBeInTheDocument();
    // Every walk is listed now, so the button goes.
    await waitFor(() => expect(within(older).queryByRole("button", { name: "Load older walks" })).toBeNull());
    expect(screen.getByRole("heading", { name: formatDayHeading(OLD_START) })).toBeInTheDocument();
  });

  it("says so when no older walk is saved", async () => {
    backend(() => ({ sessions: [], from_ms: null }));
    show();
    fireEvent.click(await screen.findByRole("radio", { name: "All" }));
    fireEvent.click(await screen.findByRole("button", { name: "Load older walks" }));
    expect(await screen.findByText("No older walks saved.")).toBeInTheDocument();
  });

  it("offers nothing older for the last 7 days or with every walk listed", async () => {
    backend(() => ({ sessions: [], from_ms: null }));
    const { unmount } = show();
    await screen.findByText("1200");
    expect(screen.queryByTestId("history-older")).toBeNull();
    unmount();
    backend(() => ({ sessions: [], from_ms: null }), RECENT[0].start_time);
    show();
    fireEvent.click(await screen.findByRole("radio", { name: "All" }));
    await screen.findByText("1200");
    expect(screen.queryByTestId("history-older")).toBeNull();
  });
});
