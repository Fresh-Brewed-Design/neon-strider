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

describe("HUD page minify hotkey", () => {
  it("shows Alt + Insert on by default, with the key choices", () => {
    render(<HudPage />);
    expect(screen.getByRole("switch", { name: "Minify hotkey" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Alt + Insert shrinks the HUD to a strip and brings it back.")).toBeInTheDocument();
    const keys = screen.getAllByRole("radio").filter((r) => r.textContent?.startsWith("Alt + "));
    expect(keys.map((k) => k.textContent)).toEqual(["Alt + Insert", "Alt + Pause", "Alt + Scroll Lock"]);
    expect(screen.getByRole("radio", { name: "Alt + Insert" })).toHaveAttribute("aria-checked", "true");
  });

  it("sends a new key to Rust and keeps it on", async () => {
    render(<HudPage />);
    await act(async () => fireEvent.click(screen.getByRole("radio", { name: "Alt + Pause" })));
    expect(dispatched()).toEqual([
      { command: { type: "hud_set_minify_hotkey", hotkey: { enabled: true, key: "pause" } } },
    ]);
  });

  it("switches it off with the key kept", async () => {
    current!.hud.minify_hotkey = { enabled: true, key: "scroll_lock" };
    render(<HudPage />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Minify hotkey" })));
    expect(dispatched()).toEqual([
      { command: { type: "hud_set_minify_hotkey", hotkey: { enabled: false, key: "scroll_lock" } } },
    ]);
  });

  it("says other apps get the key while it is off, and hides the choices", () => {
    current!.hud.minify_hotkey = { enabled: false, key: "pause" };
    render(<HudPage />);
    expect(screen.getByRole("switch", { name: "Minify hotkey" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("Other apps get Alt + Pause.")).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "Alt + Pause" })).not.toBeInTheDocument();
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
