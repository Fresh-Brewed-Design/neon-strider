import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { LivePage } = await import("./LivePage");

const speedsSent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: { type: string; kmh?: number } }).command)
    .filter((c) => c.type === "set_speed")
    .map((c) => c.kmh as number);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
  current.connection.connected = true;
  current.deck.min_speed_kmh = 0.93;
  current.deck.max_speed_kmh = 6.2;
  current.deck.speed_steps = [];
});

describe("Live page speed slider", () => {
  it("starts at the belt's floor", () => {
    render(<LivePage />);
    const [speed] = screen.getAllByRole("slider");
    expect(speed).toHaveAttribute("min", "0.93");
  });

  it("never sends 0 when dragged to its lowest position", async () => {
    render(<LivePage />);
    const [speed] = screen.getAllByRole("slider");
    for (const v of ["2", "1", "0.5", "0"]) {
      await act(async () => fireEvent.change(speed, { target: { value: v } }));
    }
    const sent = speedsSent();
    expect(sent.length).toBeGreaterThan(0);
    expect(Math.min(...sent)).toBeGreaterThanOrEqual(0.93);
  });
});

describe("Live page STEP_MAP", () => {
  beforeEach(() => localStorage.clear());

  it("shows the running program's steps in place of the stats until switched", () => {
    render(<LivePage />);
    expect(screen.getByRole("list", { name: "Program steps" })).toBeInTheDocument();
    expect(screen.queryByText("DECK_CYCLES")).not.toBeInTheDocument();

    const view = screen.getByRole("group", { name: "Step map or stats" });
    expect(within(view).getByRole("button", { name: /map/i })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(within(view).getByRole("button", { name: /stats/i }));
    expect(screen.queryByRole("list", { name: "Program steps" })).not.toBeInTheDocument();
    expect(screen.getByText("DECK_CYCLES")).toBeInTheDocument();
    expect(within(view).getByRole("button", { name: /stats/i })).toHaveAttribute("aria-pressed", "true");
    expect(localStorage.getItem("liveStepMap")).toBe("closed");
  });

  it("keeps time, distance and today's distance in view beside the map", () => {
    current!.session.duration_s = 754;
    current!.session.distance_m = 1234;
    current!.session.daily_distance_m = 4560;
    render(<LivePage />);
    const readout = screen.getByRole("group", { name: "Walk stats" });
    expect(readout).toHaveTextContent("TIME 12:34");
    expect(readout).toHaveTextContent("DIST 1.23 KM");
    expect(readout).toHaveTextContent("TODAY 4.56 KM");

    fireEvent.click(screen.getByRole("button", { name: /stats/i }));
    expect(screen.queryByRole("group", { name: "Walk stats" })).not.toBeInTheDocument();
  });

  it("keeps the stats with no program running", () => {
    current!.active_program = null;
    render(<LivePage />);
    expect(screen.queryByRole("list", { name: "Program steps" })).not.toBeInTheDocument();
    expect(screen.getByText("DECK_CYCLES")).toBeInTheDocument();
  });

  it("puts each stat's plain line under its codename", () => {
    current!.active_program = null;
    render(<LivePage />);
    for (const [codename, plain] of [
      ["DECK_CYCLES", "walk time"],
      ["DISPLACEMENT", "walk distance"],
      ["DAILY_TOTAL", "distance today"],
      ["LIFETIME_ODOMETER", "all-time distance"],
    ]) {
      const line = screen.getByText(plain);
      expect(line.previousElementSibling?.textContent).toBe(codename);
    }
  });
});

describe("Live page offline", () => {
  it("dims the last readings the treadmill sent, as the HUD does", () => {
    current!.connection.connected = false;
    current!.active_program = null;
    render(<LivePage />);
    for (const title of ["Scroll to change speed.", "Scroll to change incline."]) {
      expect(screen.getByTitle(title)).toHaveClass("opacity-45");
    }
    expect(screen.getByText("DISPLACEMENT").closest(".hacker-panel")!.querySelector(".opacity-45")).not.toBeNull();
  });

  it("dims the walk stats beside STEP_MAP", () => {
    current!.connection.connected = false;
    render(<LivePage />);
    expect(screen.getByRole("group", { name: "Walk stats" })).toHaveClass("opacity-45");
  });

  it("leaves the readings bright while linked", () => {
    render(<LivePage />);
    expect(screen.getByTitle("Scroll to change speed.")).not.toHaveClass("opacity-45");
    expect(screen.getByRole("group", { name: "Walk stats" })).not.toHaveClass("opacity-45");
  });
});

describe("Live page QUIET", () => {
  it("shows the quiet speed and the speed QUIET puts back while it holds the belt", () => {
    current!.calls = { ...current!.calls, quiet_kmh: 2, quiet_now: true, quiet_now_back_kmh: 3.5 };
    render(<LivePage />);
    const line = screen.getByText(/QUIET \/\/ DAMPED/).closest("[role='status']")!;
    expect(line).toHaveTextContent("QUIET // DAMPED 2.0 km/h");
    expect(line).toHaveTextContent("QUIET again puts 3.5 km/h back.");
  });

  it("shows nothing while QUIET is off", () => {
    current!.calls = { ...current!.calls, quiet_now: false, quiet_now_back_kmh: null };
    render(<LivePage />);
    expect(screen.queryByText(/QUIET \/\/ DAMPED/)).not.toBeInTheDocument();
  });
});

describe("Live page maintenance banner", () => {
  it("sits in the header, so the page keeps its height at 760x600", () => {
    current!.maintenance.due = true;
    const { container } = render(<LivePage />);
    const banner = screen.getByText(/Belt service due/).closest("[role=status]")!;
    expect(banner.closest("header")).not.toBeNull();
    // The body is the scroll container; nothing in it is the banner.
    const body = container.querySelector("section > div.overflow-y-auto")!;
    expect(body.contains(banner)).toBe(false);
  });

  it("stacks with a header warning instead of adding body height", () => {
    current!.maintenance.due = true;
    current!.deck.standby = true;
    render(<LivePage />);
    const header = document.querySelector("header")!;
    expect(header).toHaveTextContent(/Belt service due/);
    expect(header).toHaveTextContent(/off or asleep/);
  });
});

describe("Live page tile codenames", () => {
  it("sizes a codename with its tile, never under 11 px", async () => {
    const { CODENAME_FIT } = await import("./LivePage");
    expect(CODENAME_FIT).toMatch(/clamp\(11px,[\d.]+cqw,12px\)/);
  });
});

describe("Live page calibration offer", () => {
  it("shows under the controls, so its arrival and SKIP never move them", () => {
    current!.calibration.offer = true;
    render(<LivePage onCalibrate={() => {}} />);
    const offer = screen.getByRole("region", { name: "Calibrate this treadmill" });
    const order = (a: Node, b: Node) => a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING;
    for (const control of [
      screen.getByRole("slider", { name: "Velocity target" }),
      screen.getByRole("slider", { name: "Tilt target" }),
      screen.getByRole("button", { name: /stasis/i }),
      screen.getByRole("button", { name: /dump/i }),
      screen.getByRole("button", { name: /hold/i }),
    ]) {
      expect(order(control, offer)).toBeTruthy();
    }
  });

  it("stays out of the page without a way to open calibration", () => {
    current!.calibration.offer = true;
    render(<LivePage />);
    expect(screen.queryByRole("region", { name: "Calibrate this treadmill" })).toBeNull();
  });
});

describe("Live page header", () => {
  it("is marked to hide its title in a short window, keeping it for screen readers", () => {
    render(<LivePage />);
    const header = document.querySelector("header")!;
    expect(header).toHaveClass("dash-page-fit");
    expect(screen.getByRole("heading", { name: "Live" })).toBeInTheDocument();
  });
});
