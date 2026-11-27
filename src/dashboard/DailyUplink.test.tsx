import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { StravaSnapshot, StravaToday } from "../ipc";
import { DailyUplink } from "./DailyUplink";
import { dailyUplinkLine, useDailyUplink } from "./dailyUplinkText";

const { snapshot: mock } = await import("../dev/mockBackend");

const dispatched = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: unknown }).command);

const READY: StravaToday = { walks: 2, distance_km: 3.456, on_strava: 0, blocked: null, note: null };

function strava(today: Partial<StravaToday>, extra: Partial<StravaSnapshot> = {}): StravaSnapshot {
  return { ...mock.strava, last_result: null, syncing: false, today: { ...READY, ...today }, ...extra };
}

function Uplink({ snap, onDone }: { snap: StravaSnapshot; onDone?: () => void }) {
  const action = useDailyUplink(snap);
  return <DailyUplink strava={snap} action={action} onDone={onDone} />;
}

const button = () => screen.getByRole("button", { name: /DAILY_UPLINK/ });

beforeEach(() => vi.mocked(invoke).mockClear());

describe("DAILY_UPLINK line", () => {
  it("counts the walks and km while ready, else shows Rust's reason", () => {
    expect(dailyUplinkLine(READY)).toBe("2 walks · 3.46 km go up as one activity.");
    expect(dailyUplinkLine({ ...READY, walks: 1 })).toBe("1 walk · 3.46 km go up as one activity.");
    expect(dailyUplinkLine({ ...READY, blocked: "No walks today yet." })).toBe("No walks today yet.");
  });
});

describe("DAILY_UPLINK", () => {
  it("sends one upload-today command and shows the result under the button", async () => {
    const onDone = vi.fn();
    const { rerender } = render(<Uplink snap={strava({})} onDone={onDone} />);
    expect(button()).toBeEnabled();
    expect(button()).toHaveAttribute("title", "Uploads today's walks not on Strava yet as one activity.");
    expect(screen.getByText("today as one Strava activity")).toBeInTheDocument();

    await act(async () => fireEvent.click(button()));
    expect(dispatched()).toEqual([{ type: "strava_upload_today" }]);
    expect(button()).toBeDisabled();
    expect(button()).toHaveTextContent("Uploading…");

    const message = "Today uploaded as one activity: 2 walks, 3.46 km (activity 9)";
    rerender(
      <Uplink
        snap={strava(
          { walks: 0, distance_km: 0, on_strava: 1, blocked: "Today's walks are on Strava." },
          { last_result: { ok: true, message, at_ms: Date.now() + 1000, hint: "none" } },
        )}
        onDone={onDone}
      />,
    );
    expect(screen.getByText(message)).toBeInTheDocument();
    expect(screen.getByText("Today's walks are on Strava.")).toBeInTheDocument();
    expect(button()).toBeDisabled();
    expect(onDone).toHaveBeenCalled();
  });

  it("is off with the reason while there is nothing to send", () => {
    render(<Uplink snap={strava({ walks: 0, blocked: "A walk is running. Stop the belt to upload today." })} />);
    expect(button()).toBeDisabled();
    expect(screen.getByText("A walk is running. Stop the belt to upload today.")).toBeInTheDocument();
  });

  it("is off while another Strava upload runs", () => {
    render(<Uplink snap={strava({}, { syncing: true })} />);
    expect(button()).toBeDisabled();
  });

  it("says that walks already on Strava stay separate", () => {
    const note = "1 activity from today is already on Strava and stays separate.";
    render(<Uplink snap={strava({ on_strava: 1, note })} />);
    expect(screen.getByText(note)).toBeInTheDocument();
  });

  it("shows a refused press under the button", async () => {
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject("No walks today yet."));
    render(<Uplink snap={strava({})} />);
    await act(async () => fireEvent.click(button()));
    expect(screen.getByText(/No walks today yet\./)).toBeInTheDocument();
  });
});
