import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";
import { shareUnchanged } from "./shareUnchanged";

// Capture the `app-state` listener so the tests can publish snapshots themselves.
const ipc = vi.hoisted(() => ({ emit: null as ((s: AppSnapshot) => void) | null }));
vi.mock("../ipc", () => ({
  onAppState: (cb: (s: AppSnapshot) => void) => {
    ipc.emit = cb;
    return Promise.resolve(() => {});
  },
  // Never resolves: only the published events count.
  getSnapshot: () => new Promise(() => {}),
}));

const { getAppSnapshot, useAppState } = await import("./useAppState");

describe("shareUnchanged", () => {
  const base = () => ({
    connection: { connected: true, link: { state: "linked" } },
    live: { speed_kmh: 3.2, elapsed_s: 10 },
    weekly: { days: [{ m: 100 }, { m: 200 }], total_m: 300 },
    ids: [1, 2, 3],
  });

  it("keeps references for unchanged subtrees and replaces changed ones", () => {
    const prev = base();
    const next = base();
    next.live.elapsed_s = 11;
    next.weekly.days[1].m = 250;

    const out = shareUnchanged(prev, next);
    expect(out).toEqual(next);
    expect(out).not.toBe(prev);
    expect(out.connection).toBe(prev.connection);
    expect(out.ids).toBe(prev.ids);
    expect(out.live).not.toBe(prev.live);
    expect(out.weekly).not.toBe(prev.weekly);
    expect(out.weekly.days).not.toBe(prev.weekly.days);
    expect(out.weekly.days[0]).toBe(prev.weekly.days[0]);
    expect(out.weekly.days[1]).not.toBe(prev.weekly.days[1]);
  });

  it("returns the previous object when nothing changed", () => {
    const prev = base();
    expect(shareUnchanged(prev, base())).toBe(prev);
  });

  it("sees added, removed and resized parts as changes", () => {
    const prev = { a: { x: 1 }, list: [1, 2], opt: null as { y: number } | null };
    const grown = shareUnchanged(prev, { a: { x: 1 }, list: [1, 2, 3], opt: null });
    expect(grown.list).toEqual([1, 2, 3]);
    expect(grown.a).toBe(prev.a);

    const shrunk = shareUnchanged(prev, { a: { x: 1 }, list: [1], opt: null });
    expect(shrunk.list).toEqual([1]);

    const filled = shareUnchanged(prev, { a: { x: 1 }, list: [1, 2], opt: { y: 2 } });
    expect(filled.opt).toEqual({ y: 2 });
    expect(filled).not.toBe(prev);

    const extraKey = shareUnchanged<Record<string, unknown>>(prev.a, { x: 1, z: 2 });
    expect(extraKey).toEqual({ x: 1, z: 2 });
    const lessKey = shareUnchanged<Record<string, unknown>>({ x: 1, z: 2 }, { x: 1 });
    expect(lessKey).toEqual({ x: 1 });
    expect(shareUnchanged<unknown>([1], { 0: 1 })).toEqual({ 0: 1 });
  });

  it("starts from nothing", () => {
    const next = base();
    expect(shareUnchanged(null, next)).toBe(next);
  });
});

describe("useAppState with a heartbeat", () => {
  const snap = (elapsed: number, weeklyTotal = 300) =>
    ({
      session: { active_id: 7 },
      live: { elapsed_s: elapsed, speed_kmh: 3.2 },
      weekly: { total_m: weeklyTotal, days: [{ m: 100 }] },
      desk_sync: { state: "idle" },
      wall_clock: { minutes: 12 },
      today_timeline: { spans: [{ from: 1, to: 2 }] },
      strava: { saved_session_ids: [4, 5] },
    }) as unknown as AppSnapshot;

  // The fake snapshot's own shape, for reading it back.
  type Fake = ReturnType<typeof snap> & { live: { elapsed_s: number }; weekly: { total_m: number } };
  const current = () => getAppSnapshot() as unknown as Fake | null;

  // The History page's snapshot selectors, counting renders.
  const renders = { n: 0 };
  function useHistoryLike() {
    renders.n += 1;
    return {
      activeId: useAppState((s) => s.session.active_id),
      weekly: useAppState((s) => s.weekly),
      deskSync: useAppState((s) => s.desk_sync),
      wallClock: useAppState((s) => s.wall_clock),
      today: useAppState((s) => s.today_timeline),
      savedIds: useAppState((s) => s.strava.saved_session_ids),
    };
  }

  it("does not re-render a History-style consumer when only the heartbeat changed", () => {
    const { result } = renderHook(useHistoryLike);
    expect(ipc.emit).not.toBeNull();
    act(() => ipc.emit!(snap(1)));
    const first = result.current;
    const rendersAfterFirst = renders.n;

    act(() => ipc.emit!(snap(2)));
    act(() => ipc.emit!(snap(3)));
    expect(renders.n).toBe(rendersAfterFirst);
    expect(current()?.live).toEqual({ elapsed_s: 3, speed_kmh: 3.2 });
    expect(getAppSnapshot()?.weekly).toBe(first.weekly);

    // A real change to a selected part re-renders it, with the new object.
    act(() => ipc.emit!(snap(4, 450)));
    expect(renders.n).toBe(rendersAfterFirst + 1);
    expect(result.current.weekly).not.toBe(first.weekly);
    expect(current()?.weekly.total_m).toBe(450);
    expect(result.current.weekly).toBe(current()?.weekly);
    expect(result.current.savedIds).toBe(first.savedIds);
    expect(result.current.deskSync).toBe(first.deskSync);
  });

  it("keeps the snapshot object when an identical one arrives", () => {
    act(() => ipc.emit!(snap(9)));
    const before = getAppSnapshot();
    act(() => ipc.emit!(snap(9)));
    expect(getAppSnapshot()).toBe(before);
  });
});
