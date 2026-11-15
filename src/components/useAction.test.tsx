import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { errorText, useAction } from "./useAction";

describe("errorText", () => {
  it("reads strings, errors and typed Rust errors", () => {
    expect(errorText("Not connected")).toBe("Not connected");
    expect(errorText(new Error("Boom"))).toBe("Boom");
    expect(errorText({ kind: "not_found", message: "Treadmill not found." })).toBe("Treadmill not found.");
    expect(errorText(42)).toBe("Something went wrong");
  });
});

describe("useAction", () => {
  it("is pending at once, then done", async () => {
    const { result } = renderHook(() => useAction());
    let release: () => void = () => {};
    let pendingRun: Promise<unknown> = Promise.resolve();
    act(() => {
      pendingRun = result.current.run(() => new Promise<void>((r) => (release = r)), { done: "Saved." });
    });
    expect(result.current.pending).toBe(true);
    await act(async () => {
      release();
      await pendingRun;
    });
    expect(result.current).toMatchObject({ pending: false, error: null, done: "Saved." });
  });

  it("keeps a rejection next to the control", async () => {
    const { result } = renderHook(() => useAction());
    await act(async () => {
      await result.current.run(() => Promise.reject("Treadmill not connected"));
    });
    expect(result.current).toMatchObject({ pending: false, error: "Treadmill not connected", done: null });
  });

  it("lets a newer run win over an older one", async () => {
    const { result } = renderHook(() => useAction());
    let releaseOld: () => void = () => {};
    let oldRun: Promise<unknown> = Promise.resolve();
    act(() => {
      oldRun = result.current.run(() => new Promise<void>((r) => (releaseOld = r)), { done: "old" });
    });
    await act(async () => {
      await result.current.run(async () => {}, { done: "new" });
    });
    await act(async () => {
      releaseOld();
      await oldRun;
    });
    expect(result.current.done).toBe("new");
  });
});
