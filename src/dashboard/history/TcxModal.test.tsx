import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MergedSession } from "../../services/sessionMerge";
import { TcxModal } from "./TcxModal";

const session = {
  start_time: new Date(2026, 10, 19, 9, 0).getTime(),
  duration_seconds: 1800,
  distance_meters: 1800,
} as MergedSession;

function mockClipboard(writeText: () => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
}

describe("TcxModal copy", () => {
  afterEach(() => vi.restoreAllMocks());

  it("says the clipboard refused the TCX when the write fails", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    mockClipboard(() => Promise.reject(new Error("denied")));
    render(<TcxModal session={session} tcx="<Trackpoint>" onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect((await screen.findByRole("alert")).textContent).toBe("The clipboard refused the TCX.");
  });

  it("confirms when the TCX was copied", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    mockClipboard(writeText);
    render(<TcxModal session={session} tcx="<Trackpoint>" onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect((await screen.findByRole("status")).textContent).toBe("TCX copied to the clipboard.");
    expect(writeText).toHaveBeenCalledWith("<Trackpoint>");
  });
});
