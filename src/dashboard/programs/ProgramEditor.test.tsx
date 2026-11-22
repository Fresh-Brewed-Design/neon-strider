import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { ProgramEditor } = await import("./ProgramEditor");
const { toDraft } = await import("./draft");

const saved = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: { type: string; program?: unknown } }).command)
    .filter((c) => c.type === "program_save");

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
  current.heart_rate = { ...current.heart_rate, max_bpm: 180, max_bpm_set: true };
});

const vo2Max = () => current!.programs.find((p) => p.name === "VO2 max")!;

describe("ProgramEditor heart zones", () => {
  it("saves the seeded VO2 max with its zones intact", async () => {
    const p = vo2Max();
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const [cmd] = saved();
    expect((cmd.program as typeof p).steps).toEqual(p.steps);
  });

  it("adds a zone typed in bpm, saved as % of the max heart rate", async () => {
    const p = current!.programs[0];
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Heart zone on step 1" }));
    expect(screen.getByRole("button", { name: "Heart zone on step 1" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("radio", { name: "bpm" }));
    fireEvent.change(screen.getByLabelText("Step 1 heart zone low"), { target: { value: "126" } });
    fireEvent.change(screen.getByLabelText("Step 1 heart zone high"), { target: { value: "144" } });
    expect(screen.getByText("70 to 80 % of max")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const [cmd] = saved();
    expect((cmd.program as typeof p).steps[0].zone).toEqual({ low_pct: 70, high_pct: 80 });
  });

  it("offers no trim choice while ZONE_TILT is off, and keeps a saved one", async () => {
    const p = structuredClone(vo2Max());
    p.steps[1].zone = { ...p.steps[1].zone!, trim: "incline" };
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    expect(screen.queryByRole("radiogroup", { name: "Step 2 heart zone trims" })).toBeNull();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const [cmd] = saved();
    expect((cmd.program as typeof p).steps[1].zone?.trim).toBe("incline");
  });

  it("saves incline or both with ZONE_TILT on, and leaves speed out", async () => {
    current!.heart_rate = { ...current!.heart_rate, incline_trim: true };
    const p = vo2Max();
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    const trims = (n: number) => screen.getByRole("radiogroup", { name: `Step ${n} heart zone trims` });
    expect(within(trims(2)).getByRole("radio", { name: "Speed" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(within(trims(2)).getByRole("radio", { name: "Incline" }));
    fireEvent.click(within(trims(4)).getByRole("radio", { name: "Both" }));
    expect(screen.getByText("Incline trims move at most one deck level a minute, up to 2 % from the step's incline.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const steps = (saved()[0].program as typeof p).steps;
    expect(steps[1].zone).toEqual({ low_pct: 85, high_pct: 95, trim: "incline" });
    expect(steps[3].zone).toEqual({ low_pct: 85, high_pct: 95, trim: "both" });
    expect(steps[5].zone).toEqual({ low_pct: 85, high_pct: 95 });
  });

  it("shows no zone row or unit switch for a plain program", () => {
    render(<ProgramEditor initial={toDraft(current!.programs[0])} onDone={() => {}} />);
    expect(screen.queryByText("Heart zone")).toBeNull();
    expect(screen.queryByRole("radiogroup", { name: "Heart zone unit" })).toBeNull();
  });
});

describe("ProgramEditor step kinds", () => {
  it("shows each step's kind and saves a changed one", async () => {
    const p = current!.programs.find((x) => x.name === "Interval walk")!;
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    const kind = (n: number) => screen.getByLabelText<HTMLSelectElement>(`Step ${n} kind`);
    expect(kind(1).value).toBe("warm_up");
    expect(within(kind(2)).getByRole("option", { name: "INTERVAL" })).toBeTruthy();
    fireEvent.change(kind(11), { target: { value: "cool_down" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const steps = (saved()[0].program as typeof p).steps;
    expect(steps[10].kind).toBe("cool_down");
    expect(steps[1].kind).toBe("interval");
  });

  it("loads a step with no kind as None and saves it without one", async () => {
    const p = structuredClone(current!.programs[0]);
    delete p.steps[0].kind;
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    expect(screen.getByLabelText<HTMLSelectElement>("Step 1 kind").value).toBe("");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect((saved()[0].program as typeof p).steps[0]).not.toHaveProperty("kind");
  });

  it("explains what a kind does", () => {
    render(<ProgramEditor initial={toDraft(current!.programs[0])} onDone={() => {}} />);
    expect(screen.getByText("A step's kind names the step on the HUD, the Live page and the tray menu.")).toBeTruthy();
  });
});
