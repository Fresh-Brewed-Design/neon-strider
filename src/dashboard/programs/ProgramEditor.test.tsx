import { act, fireEvent, render, screen } from "@testing-library/react";
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

  it("shows no zone row or unit switch for a plain program", () => {
    render(<ProgramEditor initial={toDraft(current!.programs[0])} onDone={() => {}} />);
    expect(screen.queryByText("Heart zone")).toBeNull();
    expect(screen.queryByRole("radiogroup", { name: "Heart zone unit" })).toBeNull();
  });
});
