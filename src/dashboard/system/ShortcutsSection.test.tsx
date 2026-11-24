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
const { ShortcutsSection } = await import("./ShortcutsSection");

const dispatched = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: unknown }).command);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Shortcuts", () => {
  it("lists every shortcut with its keys as the OS writes them", () => {
    render(<ShortcutsSection />);
    expect(screen.getByText("Start or pause")).toBeInTheDocument();
    expect(screen.getByText("Alt+Home")).toBeInTheDocument();
    expect(screen.getByText("Alt+Volume Up")).toBeInTheDocument();
    // Speed up already has two keys, so it offers no Add.
    expect(screen.queryByRole("button", { name: "Add a shortcut for Speed up" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add a shortcut for Start or pause" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reset to the defaults" })).not.toBeInTheDocument();
  });

  it("records a key with the shortcuts paused, and adds it", async () => {
    render(<ShortcutsSection />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Add a shortcut for Start or pause" })));
    expect(screen.getByRole("status")).toHaveTextContent("Press the keys… Esc cancels");
    // A modifier alone waits for the real key.
    await act(async () => fireEvent.keyDown(window, { code: "ControlLeft", ctrlKey: true }));
    await act(async () => fireEvent.keyDown(window, { code: "KeyS", ctrlKey: true, altKey: true }));
    expect(dispatched()).toEqual([
      { type: "shortcuts_recording", on: true },
      {
        type: "shortcuts_set",
        shortcut: "toggle_start",
        keys: [
          { code: "Home", ctrl: false, alt: true, shift: false, meta: false },
          { code: "KeyS", ctrl: true, alt: true, shift: false, meta: false },
        ],
      },
      { type: "shortcuts_recording", on: false },
    ]);
  });

  it("cancels on Esc without changing anything", async () => {
    render(<ShortcutsSection />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Add a shortcut for Start or pause" })));
    await act(async () => fireEvent.keyDown(window, { code: "Escape" }));
    expect(dispatched()).toEqual([
      { type: "shortcuts_recording", on: true },
      { type: "shortcuts_recording", on: false },
    ]);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("removes a key", async () => {
    render(<ShortcutsSection />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Remove Alt+Insert from Minify the HUD" })));
    expect(dispatched()).toEqual([{ type: "shortcuts_set", shortcut: "toggle_minified", keys: [] }]);
  });

  it("offers a reset once a shortcut changed, and crosses out a taken key", async () => {
    current!.shortcuts.changed = true;
    current!.shortcuts.list[0].keys[0].taken = true;
    render(<ShortcutsSection />);
    expect(screen.getByText("Alt+=")).toHaveClass("line-through");
    expect(screen.getByText(/Another app already uses the crossed-out key/)).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Reset to the defaults" })));
    expect(dispatched()).toEqual([{ type: "shortcuts_reset" }]);
  });
});
