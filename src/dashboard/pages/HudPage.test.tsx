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
const { HudPage } = await import("./HudPage");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("HUD page minify shortcut", () => {
  it("names the minify shortcut and where to change it", () => {
    render(<HudPage />);
    expect(
      screen.getByText("Alt+Insert shrinks the HUD to a strip and back, set in System, ROUTINE, Shortcuts."),
    ).toBeInTheDocument();
  });

  it("says Off while it has no key", () => {
    const minify = current!.shortcuts.list.find((v) => v.shortcut === "toggle_minified")!;
    minify.keys = [];
    render(<HudPage />);
    expect(screen.getByText("Off. Set in System, ROUTINE, Shortcuts.")).toBeInTheDocument();
  });
});

describe("HUD page pickers with the keyboard", () => {
  it("moves the display on Enter after an arrow key, and a click on the primary forgets a missing one", async () => {
    render(<HudPage />);
    const group = screen.getByRole("radiogroup", { name: "Display" });
    const primary = screen.getByRole("radio", { name: /^Display 1/ });
    expect(group).toContainElement(primary);
    expect(primary).toHaveAttribute("tabindex", "0");
    await act(async () => fireEvent.keyDown(primary, { key: "ArrowDown" }));
    expect(dispatched()).toEqual([]);
    const second = screen.getByRole("radio", { name: /^Display 2/ });
    expect(second).toHaveFocus();
    await act(async () => fireEvent.keyDown(second, { key: "Enter" }));
    await act(async () => fireEvent.click(primary));
    expect(dispatched()).toEqual([
      { command: { type: "hud_set_monitor", monitor: mock.hud_monitors[1].name } },
      { command: { type: "hud_set_monitor", monitor: null } },
    ]);
  });

  it("sends the next idle mode on ArrowDown and the last on End", async () => {
    render(<HudPage />);
    const full = screen.getByRole("radio", { name: "Show everything" });
    await act(async () => fireEvent.keyDown(full, { key: "ArrowDown" }));
    await act(async () => fireEvent.keyDown(full, { key: "End" }));
    expect(dispatched()).toEqual([
      { command: { type: "hud_set_idle_mode", mode: "strip" } },
      { command: { type: "hud_set_idle_mode", mode: "hidden" } },
    ]);
  });
});
