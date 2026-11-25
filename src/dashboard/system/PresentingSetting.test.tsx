import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { PresentingSetting } = await import("./PresentingSetting");

const NAME = "Hold cards while presenting";
const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Desk hold cards while presenting", () => {
  it("shows Rust's switch, on by default, with its one-line hint", () => {
    render(<PresentingSetting />);
    expect(screen.getByRole("switch", { name: NAME })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText(/Cards that can wait show once the slideshow or full-screen app closes/)).toBeInTheDocument();
  });

  it("asks Rust to switch it off", async () => {
    render(<PresentingSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: NAME })));
    expect(dispatched()).toEqual([{ command: { type: "presenting_set_config", enabled: false } }]);
  });

  it("shows it off when Rust says so", () => {
    current!.presenting = { enabled: false, presenting: true, holding: false };
    render(<PresentingSetting />);
    expect(screen.getByRole("switch", { name: NAME })).toHaveAttribute("aria-checked", "false");
  });

  it("stays out of the way on a Mac, where nothing reports presenting", () => {
    current!.platform = { ...current!.platform, os: "macos" };
    render(<PresentingSetting />);
    expect(screen.queryByRole("switch", { name: NAME })).toBeNull();
  });
});
