import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBackendAction, type BackendResult } from "./useAction";

type Props = { result: BackendResult | null; busy: boolean };

function setup(initial: Props = { result: null, busy: false }) {
  return renderHook((p: Props) => useBackendAction(p.result, p.busy), { initialProps: initial });
}

describe("useBackendAction", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(10_000);
  });
  afterEach(() => vi.useRealTimers());

  it("stays pending after the command is accepted, until Rust records a newer result", async () => {
    const old = { ok: true, message: "Old upload", at_ms: 5_000 };
    const { result, rerender } = setup({ result: old, busy: true });
    await act(async () => {
      await result.current.run(async () => {});
    });
    expect(result.current.pending).toBe(true);
    expect(result.current.done).toBeNull();

    rerender({ result: { ok: true, message: "Uploaded 2.4 km", at_ms: 11_000 }, busy: false });
    expect(result.current).toMatchObject({ pending: false, error: null, done: "Uploaded 2.4 km" });
  });

  it("shows a failed result as the error", async () => {
    const { result, rerender } = setup();
    await act(async () => {
      await result.current.run(async () => {});
    });
    rerender({ result: { ok: false, message: "Can't reach Strava", at_ms: 11_000 }, busy: false });
    expect(result.current).toMatchObject({ pending: false, error: "Can't reach Strava", done: null });
  });

  it("keeps a rejected command's error and stops waiting", async () => {
    const { result } = setup();
    await act(async () => {
      await result.current.run(() => Promise.reject("Not connected"));
    });
    expect(result.current).toMatchObject({ pending: false, error: "Not connected", done: null });
  });

  it("settles after a quiet moment when the command records nothing", async () => {
    const { result, rerender } = setup({ result: null, busy: true });
    await act(async () => {
      await result.current.run(async () => {}, { done: "Opened." });
    });
    await act(async () => {
      vi.advanceTimersByTime(2_000);
    });
    // Rust is still busy, so it is not over yet.
    expect(result.current.pending).toBe(true);

    rerender({ result: null, busy: false });
    await act(async () => {
      vi.advanceTimersByTime(100);
    });
    expect(result.current).toMatchObject({ pending: false, error: null, done: "Opened." });
  });

  it("reset clears everything", async () => {
    const { result } = setup();
    await act(async () => {
      await result.current.run(() => Promise.reject("Nope"));
    });
    act(() => result.current.reset());
    expect(result.current).toMatchObject({ pending: false, error: null, done: null });
  });
});
