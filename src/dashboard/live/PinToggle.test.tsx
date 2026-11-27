import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PinToggle } from "./PinToggle";

beforeEach(() => vi.mocked(invoke).mockClear());

describe("PinToggle", () => {
  it("shows a refused pin in a note under itself", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    render(<PinToggle widget="DISTANCE" pinned={false} />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Show on HUD" })));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("dispatch", { command: { type: "hud_toggle_widget", widget: "DISTANCE" } });
    expect(screen.getByRole("alert")).toHaveTextContent("Treadmill not connected");
  });

  it("clears the note after a few seconds", async () => {
    vi.useFakeTimers();
    try {
      vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
      render(<PinToggle widget="DISTANCE" pinned={false} />);
      await act(async () => fireEvent.click(screen.getByRole("button")));
      expect(screen.getByRole("alert")).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(8000));
      expect(screen.queryByRole("alert")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
