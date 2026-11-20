import { act, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { useGuidedTask } = await import("./useGuidedTask");
const { GuidedPinMarker } = await import("./GuidedPinMarker");

const dispatched = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, args]) => (args as { command: { type: string } }).command);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("useGuidedTask", () => {
  it("starts when the task opens and ends when it closes, once each", () => {
    const { rerender } = renderHook(({ active }) => useGuidedTask("ground_truth", active), {
      initialProps: { active: false },
    });
    expect(dispatched()).toEqual([]);
    rerender({ active: true });
    rerender({ active: true });
    expect(dispatched()).toEqual([{ type: "guided_task_start", task: "ground_truth" }]);
    rerender({ active: false });
    expect(dispatched()).toEqual([
      { type: "guided_task_start", task: "ground_truth" },
      { type: "guided_task_end", task: "ground_truth" },
    ]);
  });

  it("leaving the page mid-task leaves the pin to Rust", () => {
    const { unmount } = renderHook(() => useGuidedTask("ground_truth", true));
    unmount();
    expect(dispatched()).toEqual([{ type: "guided_task_start", task: "ground_truth" }]);
  });
});

describe("GuidedPinMarker", () => {
  it("shows nothing while unpinned", () => {
    const { container } = render(<GuidedPinMarker />);
    expect(container.textContent).toBe("");
  });

  it("names the task and UNPIN cancels the pin", async () => {
    current!.guided_task = { pinned: true, tasks: ["ground_truth"], unpin_in_s: 1500 };
    render(<GuidedPinMarker />);
    expect(screen.getByText("Dashboard held open for GROUND_TRUTH.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("UNPIN")));
    expect(dispatched()).toContainEqual({ type: "guided_task_cancel" });
  });
});
