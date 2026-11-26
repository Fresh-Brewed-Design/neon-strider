import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, FitToTimeOption } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { ProgramsPage } = await import("../pages/ProgramsPage");

const sent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, args]) => (args as { command: { type: string } }).command);

/** Rust's picker for Interval walk (id 7): 35 min saved, 15 to 95. */
const INTERVAL: FitToTimeOption = {
  program_id: 7,
  saved_label: "35 min",
  lengths: [15, 20, 25, 30, 40, 95],
  note: "WARM_UP keeps its minutes.\nThe saved program stays as it is.",
};

function show(options: FitToTimeOption[]) {
  const s = structuredClone(mock);
  s.active_program = null;
  s.connection.connected = true;
  s.fit_to_time = options;
  current = s;
  return render(<ProgramsPage />);
}

const runButton = (name: string) => {
  const row = screen.getByLabelText(`Edit ${name}`).closest("div")!;
  return row.querySelector("button:last-of-type") as HTMLButtonElement;
};

beforeEach(() => vi.mocked(invoke).mockClear());

describe("FIT_TO_TIME", () => {
  it("shows a length picker only where Rust offers one", () => {
    show([INTERVAL]);
    const picker = screen.getByLabelText("Length of Interval walk") as HTMLSelectElement;
    expect(picker.value).toBe("");
    expect(screen.getByText("Length")).toBeInTheDocument();
    expect(Array.from(picker.options).map((o) => o.text)).toEqual(["35 min", "15 min", "20 min", "25 min", "30 min", "40 min", "95 min"]);
    expect(screen.queryByLabelText("Length of Post-meal")).toBeNull();
    expect(screen.queryByTestId("fit-to-time-note")).toBeNull();
  });

  it("runs as saved until a length is picked", async () => {
    show([INTERVAL]);
    await act(async () => fireEvent.click(runButton("Interval walk")));
    expect(sent()).toEqual([{ type: "program_run", id: 7 }]);
  });

  it("sends the picked length with the press and says what stays", async () => {
    show([INTERVAL]);
    fireEvent.change(screen.getByLabelText("Length of Interval walk"), { target: { value: "25" } });
    expect(screen.getByTestId("fit-to-time-note").textContent).toBe("WARM_UP keeps its minutes.The saved program stays as it is.");
    // Picking never starts the belt.
    expect(sent()).toEqual([]);
    await act(async () => fireEvent.click(runButton("Interval walk")));
    expect(sent()).toEqual([{ type: "program_run", id: 7, minutes: 25 }]);
  });

  it("shows a refused start under Run", async () => {
    show([INTERVAL]);
    vi.mocked(invoke).mockImplementationOnce(() => Promise.reject("Interval walk runs from 15 to 95 min."));
    await act(async () => fireEvent.click(runButton("Interval walk")));
    expect(screen.getByRole("alert").textContent).toContain("runs from 15 to 95 min");
  });
});

describe("LAST_RUN", () => {
  const rowOf = (name: string) => screen.getByLabelText(`Edit ${name}`).closest("li")!;

  it("tags the row of the program run last", () => {
    const s = structuredClone(mock);
    s.active_program = null;
    s.program_last_run_id = 2;
    current = s;
    render(<ProgramsPage />);
    expect(screen.getAllByText("LAST_RUN")).toHaveLength(1);
    expect(rowOf("Workday").textContent).toContain("LAST_RUN");
    expect(rowOf("Post-meal").textContent).not.toContain("LAST_RUN");
  });

  it("shows no tag before any program has run", () => {
    const s = structuredClone(mock);
    s.active_program = null;
    s.program_last_run_id = null;
    current = s;
    render(<ProgramsPage />);
    expect(screen.queryByText("LAST_RUN")).toBeNull();
  });

  it("leaves the tag off while the program runs", () => {
    const s = structuredClone(mock);
    s.program_last_run_id = s.active_program!.program_id;
    current = s;
    render(<ProgramsPage />);
    expect(screen.queryByText("LAST_RUN")).toBeNull();
  });
});
