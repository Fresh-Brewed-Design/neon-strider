import { act, renderHook } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import type { ConnectionSnapshot, LinkState } from "../ipc";
import { useRelink } from "./useRelink";

const conn = (link: LinkState): ConnectionSnapshot => ({
  connected: false,
  connecting: link.state === "connecting",
  device_id: null,
  has_saved_device: true,
  link,
  last_error: null,
  start_pending: false,
  start_result: null,
  connect_dialog_at_ms: null,
});

const reconnecting = (next_retry_s: number) => conn({ state: "reconnecting", attempt: 1, next_retry_s });

function setup(initial: ConnectionSnapshot) {
  return renderHook(({ c }) => useRelink(c), { initialProps: { c: initial } });
}

describe("useRelink", () => {
  it("sends connect_now and reads LINKING… until the link state moves", async () => {
    vi.mocked(invoke).mockClear();
    const { result, rerender } = setup(reconnecting(9));
    expect(result.current.view?.clickable).toBe(true);

    await act(async () => result.current.relink());
    expect(invoke).toHaveBeenCalledWith("dispatch", { command: { type: "connect_now" } });
    expect(result.current.view).toMatchObject({ clickable: false, text: "LINKING…" });

    // The countdown ticking is not an answer.
    rerender({ c: reconnecting(8) });
    expect(result.current.view?.text).toBe("LINKING…");

    // The watchdog started linking: the label shows the real state again.
    rerender({ c: conn({ state: "connecting", attempt: 1, of: 3 }) });
    expect(result.current.view?.text).toBeNull();

    // Back to waiting later is a new state, not the old click.
    rerender({ c: reconnecting(15) });
    expect(result.current.view).toMatchObject({ clickable: true, text: null });
  });

  it("shows a rejection as a HUD note and becomes clickable again", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("No treadmill is saved yet. Use Connect to pick one.");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    const { result } = setup(reconnecting(9));
    await act(async () => result.current.relink());
    expect(result.current.error).toBe("SYS // NO TREADMILL IS SAVED YET. USE CONNECT TO PICK ONE");
    expect(result.current.view?.clickable).toBe(true);
  });

  it("gives up on LINKING… after a few seconds with no change", async () => {
    vi.useFakeTimers();
    try {
      const { result } = setup(conn({ state: "adapter_unavailable" }));
      await act(async () => result.current.relink());
      expect(result.current.view?.text).toBe("LINKING…");
      act(() => vi.advanceTimersByTime(6000));
      expect(result.current.view).toMatchObject({ clickable: true, text: null });
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens the dashboard's connect dialog when no treadmill is saved", async () => {
    vi.mocked(invoke).mockClear();
    const { result } = setup({ ...conn({ state: "idle" }), has_saved_device: false });
    expect(result.current.view).toMatchObject({ clickable: true, opensConnectDialog: true });
    await act(async () => result.current.relink());
    expect(invoke).toHaveBeenCalledWith("dispatch", { command: { type: "open_connect_dialog" } });
    expect(invoke).not.toHaveBeenCalledWith("dispatch", { command: { type: "connect_now" } });
    // Nothing to wait for: no LINKING…, still clickable.
    expect(result.current.view).toMatchObject({ clickable: true, text: null });
  });

  it("does nothing while connected", async () => {
    vi.mocked(invoke).mockClear();
    const { result } = setup({ ...conn({ state: "connected" }), connected: true, has_saved_device: false });
    await act(async () => result.current.relink());
    expect(invoke).not.toHaveBeenCalled();
  });
});
