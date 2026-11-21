import { act, renderHook } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { describe, expect, it, vi } from "vitest";
import type { HudConfig } from "../ipc";
import { usePlacement } from "./useHudDrag";

type Payload = { custom: { dx: number; top: number } | null; anchor: HudConfig["anchor"]; done: boolean };

function hud(patch: Partial<HudConfig> = {}): HudConfig {
  return { widgets: [], anchor: "bottom_right", monitor: null, dashboard_pinned: false, custom: null, idle_mode: "full", minified: false, minify_hotkey: { enabled: true, key: "insert" }, ...patch };
}

/** Capture the `hud-drag` listener the hook registers. */
async function setup(initial: HudConfig) {
  let handler: ((e: { payload: Payload }) => void) | undefined;
  vi.mocked(listen).mockImplementationOnce((_name, cb) => {
    handler = cb as typeof handler;
    return Promise.resolve(() => {});
  });
  const r = renderHook(({ cfg }) => usePlacement(cfg), { initialProps: { cfg: initial } });
  await act(async () => {});
  return { ...r, fire: (p: Payload) => act(() => handler?.({ payload: p })) };
}

describe("usePlacement", () => {
  it("follows the saved config when nothing is being dragged", async () => {
    const { result } = await setup(hud({ custom: { dx: 10, top: 300 } }));
    expect(result.current).toEqual({ custom: { dx: 10, top: 300 }, anchor: "bottom_right", dragging: false });
  });

  it("follows Rust's live frames during a drag, then hands over to the saved config", async () => {
    const { result, fire, rerender } = await setup(hud());
    fire({ custom: { dx: 5, top: 120 }, anchor: "bottom_right", done: false });
    expect(result.current).toEqual({ custom: { dx: 5, top: 120 }, anchor: "bottom_right", dragging: true });
    // Dropped: still shown from the event until the snapshot catches up.
    fire({ custom: { dx: 5, top: 140 }, anchor: "bottom_right", done: true });
    expect(result.current.custom).toEqual({ dx: 5, top: 140 });
    expect(result.current.dragging).toBe(false);
    rerender({ cfg: hud({ custom: { dx: 5, top: 140 } }) });
    expect(result.current.custom).toEqual({ dx: 5, top: 140 });
  });

  it("a drop into a corner shows that corner at once", async () => {
    const { result, fire } = await setup(hud({ custom: { dx: 5, top: 140 } }));
    fire({ custom: null, anchor: "top_left", done: true });
    expect(result.current).toEqual({ custom: null, anchor: "top_left", dragging: false });
  });
});
