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
const { DeskSection } = await import("./DeskSection");

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Desk quiet speed", () => {
  it("stays editable with Slow down on calls off, since QUIET on the HUD uses it too", () => {
    current!.calls = { ...current!.calls, enabled: false };
    render(<DeskSection />);
    expect(screen.getByRole("switch", { name: "Slow down on calls" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/Calls and QUIET on the HUD ease a running belt to the quiet speed/)).toBeInTheDocument();
    expect(screen.queryByText(/Microphone/)).toBeNull();
  });

  it("shows without the call switch on a computer with no microphone list", () => {
    current!.platform = { ...current!.platform, microphone: false };
    render(<DeskSection />);
    expect(screen.queryByRole("switch", { name: "Slow down on calls" })).toBeNull();
    expect(screen.getByText(/^QUIET on the HUD eases a running belt to the quiet speed/)).toBeInTheDocument();
  });
});

describe("System -> Desk status lines", () => {
  it("shows a refused program chip under the chips", async () => {
    current!.seated = { ...current!.seated, enabled: true };
    render(<DeskSection />);
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    const chips = screen.getByRole("list", { name: "Programs on the card" });
    await act(async () => fireEvent.click(within(chips).getAllByRole("button")[0]));
    const status = screen.getByRole("alert");
    expect(status).toHaveTextContent("Treadmill not connected");
    expect(chips.nextElementSibling).toBe(status);
  });

  it("shows a refused ATROPHY_WARNING switch under the switch", async () => {
    render(<DeskSection />);
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    const toggle = screen.getByRole("switch", { name: "ATROPHY_WARNING: Sit-too-long reminder" });
    await act(async () => fireEvent.click(toggle));
    expect(toggle.closest("div")!.parentElement).toHaveTextContent("Treadmill not connected");
  });
});
