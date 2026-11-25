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
const { NEW_PROGRAM, toDraft } = await import("./draft");
const { useTreadmillStore } = await import("../../store/treadmillStore");
const { leavePage } = await import("../leavePage");

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

describe("ProgramEditor cool-down pulse target (#233)", () => {
  it("offers the pulse target on a cool-down step only, and saves it as % of max", async () => {
    const p = structuredClone(vo2Max());
    delete p.steps[9].pulse_end;
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    expect(screen.queryByLabelText("Step 1 ends on heart rate")).toBeNull();
    // A saved cool-down without a target loads as it was saved.
    const box = screen.getByLabelText("Step 10 ends on heart rate");
    expect(screen.getByLabelText<HTMLInputElement>("Step 10 pulse target").disabled).toBe(true);
    fireEvent.click(box);
    expect(screen.getByLabelText<HTMLInputElement>("Step 10 pulse target").value).toBe("60");
    expect(screen.getByText("108 bpm")).toBeTruthy();
    expect(screen.getByText("With no heart rate reading, the cool-down step ends on its minutes.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const steps = (saved()[0].program as typeof p).steps;
    expect(steps[9].pulse_end).toEqual({ below_pct: 60 });
    expect(steps[0]).not.toHaveProperty("pulse_end");
  });

  it("loads the seeded VO2 max with its cool-down ending on heart rate, and saves it unticked", async () => {
    const p = vo2Max();
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    const box = screen.getByLabelText<HTMLInputElement>("Step 10 ends on heart rate");
    expect(box.checked).toBe(true);
    expect(screen.getByLabelText<HTMLInputElement>("Step 10 pulse target").value).toBe("60");
    fireEvent.click(box);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect((saved()[0].program as typeof p).steps[9]).not.toHaveProperty("pulse_end");
  });

  it("ticks the target at 60 % when a walking timed step becomes COOL_DOWN", async () => {
    const p = current!.programs.find((x) => x.name === "Interval walk")!;
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    expect(p.steps[10]).not.toHaveProperty("pulse_end");
    fireEvent.change(screen.getByLabelText("Step 11 kind"), { target: { value: "cool_down" } });
    expect(screen.getByLabelText<HTMLInputElement>("Step 11 ends on heart rate").checked).toBe(true);
    expect(screen.getByLabelText<HTMLInputElement>("Step 11 pulse target").value).toBe("60");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const steps = (saved()[0].program as typeof p).steps;
    expect(steps[10]).toMatchObject({ kind: "cool_down", pulse_end: { below_pct: 60 } });
    expect(steps.filter((s) => s.pulse_end)).toHaveLength(1);
  });

  it("ticks it in bpm while the editor shows bpm", () => {
    const p = structuredClone(vo2Max());
    p.steps = [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "walk" }];
    p.steps[0].zone = { low_pct: 60, high_pct: 70 };
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    // The zone brings up the unit switch; with it off, the step can end on heart rate.
    fireEvent.click(screen.getByRole("radio", { name: "bpm" }));
    fireEvent.click(screen.getByLabelText("Heart zone on step 1"));
    fireEvent.change(screen.getByLabelText("Step 1 kind"), { target: { value: "cool_down" } });
    expect(screen.getByLabelText<HTMLInputElement>("Step 1 pulse target").value).toBe("108");
  });

  it("leaves the target off on a step that can't end on heart rate, and drops it off COOL_DOWN", async () => {
    const p = structuredClone(vo2Max());
    p.steps = [
      { duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "walk", zone: { low_pct: 60, high_pct: 70 } },
      { duration_minutes: 2, speed_kmh: 0, incline_pct: 0, kind: "rest" },
      { duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "walk" },
      { duration_minutes: null, speed_kmh: 3, incline_pct: 0, kind: "walk" },
    ];
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    for (const n of [1, 2, 3, 4]) fireEvent.change(screen.getByLabelText(`Step ${n} kind`), { target: { value: "cool_down" } });
    for (const n of [1, 2, 4]) expect(screen.getByLabelText<HTMLInputElement>(`Step ${n} ends on heart rate`).checked).toBe(false);
    expect(screen.getByLabelText<HTMLInputElement>("Step 3 ends on heart rate").checked).toBe(true);
    fireEvent.change(screen.getByLabelText("Step 3 kind"), { target: { value: "walk" } });
    expect(screen.queryByLabelText("Step 3 ends on heart rate")).toBeNull();
    fireEvent.change(screen.getByLabelText("Step 1 kind"), { target: { value: "walk" } });
    fireEvent.change(screen.getByLabelText("Step 2 kind"), { target: { value: "walk" } });
    fireEvent.change(screen.getByLabelText("Step 4 kind"), { target: { value: "walk" } });
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect((saved()[0].program as typeof p).steps.some((s) => s.pulse_end)).toBe(false);
  });

  it("shows the target in bpm with the unit switch", () => {
    const p = structuredClone(vo2Max());
    p.steps = [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "cool_down", pulse_end: { below_pct: 60 } }];
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    expect(screen.getByText("Pulse target in")).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: "bpm" }));
    expect(screen.getByLabelText<HTMLInputElement>("Step 1 pulse target").value).toBe("108");
    expect(screen.getByText("60 % of max")).toBeTruthy();
  });
});
});

describe("ProgramEditor STRIDE_FIT flag", () => {
  const brisk = () => current!.programs.find((p) => p.name === "Brisk")!;
  const flag = () => screen.queryByText(/^Above STRIDE_FIT/);

  it("flags a step above STRIDE_FIT and leaves its speed alone", async () => {
    current!.deck.stride_fit_kmh = 4.5;
    const p = brisk();
    render(<ProgramEditor initial={toDraft(p)} onDone={() => {}} />);
    expect(flag()?.textContent).toBe("Above STRIDE_FIT 4.5 km/h");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    const [cmd] = saved();
    expect((cmd.program as typeof p).steps[0].speed_kmh).toBe(5);
  });

  it("shows no flag at or below STRIDE_FIT", () => {
    current!.deck.stride_fit_kmh = 5;
    render(<ProgramEditor initial={toDraft(brisk())} onDone={() => {}} />);
    expect(flag()).toBeNull();
  });

  it("shows no flag while STRIDE_FIT is unset", () => {
    current!.deck.stride_fit_kmh = null;
    render(<ProgramEditor initial={toDraft(brisk())} onDone={() => {}} />);
    expect(flag()).toBeNull();
  });

  it("follows a speed typed into the step", () => {
    current!.deck.stride_fit_kmh = 5.5;
    render(<ProgramEditor initial={toDraft(brisk())} onDone={() => {}} />);
    expect(flag()).toBeNull();
    fireEvent.change(screen.getByDisplayValue("5.0"), { target: { value: "5.6" } });
    expect(flag()).toBeTruthy();
  });
});

describe("ProgramEditor unsaved edits (#368)", () => {
  const brisk = () => current!.programs.find((p) => p.name === "Brisk")!;
  const pending = () => useTreadmillStore.getState().pendingConfirmation;
  const speed = () => screen.getByLabelText<HTMLInputElement>("Step 1 speed in km/h");

  beforeEach(() => useTreadmillStore.setState({ pendingConfirmation: null, leaveGuard: null }));

  it("closes at once on Cancel, X or Escape with nothing changed", () => {
    const done = vi.fn();
    render(<ProgramEditor initial={toDraft(brisk())} onDone={done} />);
    fireEvent.click(screen.getAllByRole("button", { name: "Cancel" })[0]);
    fireEvent.click(screen.getAllByRole("button", { name: "Cancel" })[1]);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(done).toHaveBeenCalledTimes(3);
    expect(pending()).toBeNull();
    expect(useTreadmillStore.getState().leaveGuard).toBeNull();
  });

  it("asks before discarding an edit, and keeps the edit on Cancel", () => {
    const done = vi.fn();
    render(<ProgramEditor initial={toDraft(brisk())} onDone={done} />);
    fireEvent.change(speed(), { target: { value: "4.5" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Cancel" })[1]);
    expect(done).not.toHaveBeenCalled();
    expect(pending()).toMatchObject({ title: "Discard edits", message: 'Discard the unsaved edits to "Brisk"?' });
    act(() => useTreadmillStore.getState().clearConfirmation());
    expect(speed().value).toBe("4.5");
    fireEvent.keyDown(window, { key: "Escape" });
    act(() => pending()!.onConfirm());
    expect(done).toHaveBeenCalledTimes(1);
  });

  it("leaves Escape to an open dialog", () => {
    const done = vi.fn();
    render(<ProgramEditor initial={toDraft(brisk())} onDone={done} />);
    fireEvent.change(speed(), { target: { value: "4.5" } });
    fireEvent.click(screen.getAllByRole("button", { name: "Cancel" })[0]);
    const first = pending();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(pending()).toBe(first);
    expect(done).not.toHaveBeenCalled();
  });

  it("doesn't ask once an edit is typed back", () => {
    const done = vi.fn();
    render(<ProgramEditor initial={toDraft(brisk())} onDone={done} />);
    fireEvent.change(speed(), { target: { value: "4.5" } });
    fireEvent.change(speed(), { target: { value: "5" } });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(pending()).toBeNull();
    expect(done).toHaveBeenCalledTimes(1);
  });

  it("guards another page only while the draft has edits", () => {
    const done = vi.fn();
    const go = vi.fn();
    render(<ProgramEditor initial={NEW_PROGRAM} onDone={done} />);
    leavePage(go);
    expect(go).toHaveBeenCalledTimes(1);
    fireEvent.change(speed(), { target: { value: "4" } });
    leavePage(go);
    expect(go).toHaveBeenCalledTimes(1);
    expect(pending()?.message).toBe("Discard the new program?");
    act(() => pending()!.onConfirm());
    expect(done).toHaveBeenCalledTimes(1);
    expect(go).toHaveBeenCalledTimes(2);
  });
});

describe("ProgramEditor errors at the field (#368)", () => {
  const speed = () => screen.getByLabelText<HTMLInputElement>("Step 1 speed in km/h");
  const minutes = () => screen.getByLabelText<HTMLInputElement>("Step 1 minutes");
  const p = () => current!.programs.find((x) => x.name === "Brisk")!;

  it("marks a speed that isn't a number under its step as it is typed", () => {
    render(<ProgramEditor initial={toDraft(p())} onDone={() => {}} />);
    fireEvent.change(speed(), { target: { value: "abc" } });
    expect(speed()).toHaveAttribute("aria-invalid", "true");
    const line = document.getElementById(speed().getAttribute("aria-describedby")!);
    expect(line).toHaveTextContent("Speed needs a number.");
    expect(minutes()).not.toHaveAttribute("aria-invalid");
  });

  it("marks a speed above the deck's top, and Save names the step without sending", async () => {
    render(<ProgramEditor initial={toDraft(p())} onDone={() => {}} />);
    fireEvent.change(speed(), { target: { value: "99" } });
    expect(screen.getByText("Speed must be 0 or 0.6 to 10.5 km/h.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(saved()).toEqual([]);
    expect(screen.getByRole("alert")).toHaveTextContent("Fix step 1 to save.");
    fireEvent.change(speed(), { target: { value: "5" } });
    expect(screen.queryByRole("alert")).toBeNull();
    expect(speed()).not.toHaveAttribute("aria-invalid");
  });

  it("shows an emptied field only after Save", async () => {
    render(<ProgramEditor initial={toDraft(p())} onDone={() => {}} />);
    fireEvent.change(minutes(), { target: { value: "" } });
    expect(minutes()).not.toHaveAttribute("aria-invalid");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(minutes()).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Minutes need a number.")).toBeTruthy();
  });

  it("puts Rust's step error under that step", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Step 1: a zone needs a walking speed");
    render(<ProgramEditor initial={toDraft(p())} onDone={() => {}} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Save" })));
    expect(screen.getByText("A zone needs a walking speed.")).toBeTruthy();
    expect(screen.getByRole("alert")).toHaveTextContent("Fix step 1 to save.");
    fireEvent.change(speed(), { target: { value: "4" } });
    expect(screen.queryByText("A zone needs a walking speed.")).toBeNull();
  });
});
