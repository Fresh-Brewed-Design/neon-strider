import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { COLD_KEYS, type AppSnapshot, type AppStateFrame } from "../ipc";
import { mergeAppState } from "./mergeAppState";

// Only the fields these tests read; the rest are never touched by a merge.
function snapshot(speed: number, programName: string): AppSnapshot {
  return {
    deck: { speed_kmh: speed },
    session: { distance_m: speed * 100 },
    programs: [{ id: 1, name: programName }],
    strava: { connected: true },
  } as unknown as AppSnapshot;
}

function full(s: AppSnapshot): AppStateFrame {
  return { full: true, ...s };
}

/** What Rust sends when only hot parts changed: no cold keys at all. */
function hot(s: AppSnapshot): AppStateFrame {
  const fields: Record<string, unknown> = { ...s };
  for (const key of COLD_KEYS) delete fields[key];
  return { full: false, ...fields } as AppStateFrame;
}

describe("mergeAppState", () => {
  it("takes a full frame as the whole snapshot, without its flag", () => {
    const next = mergeAppState(null, full(snapshot(3, "Brisk")));
    expect(next).toEqual(snapshot(3, "Brisk"));
    expect(next).not.toHaveProperty("full");
  });

  it("keeps the cold parts, the same objects, through hot frames", () => {
    const first = mergeAppState(null, full(snapshot(3, "Brisk")))!;
    const next = mergeAppState(first, hot(snapshot(4, "ignored")))!;
    expect(next.deck.speed_kmh).toBe(4);
    expect(next.session.distance_m).toBe(400);
    expect(next.programs).toBe(first.programs);
    expect(next.strava).toBe(first.strava);
    expect(next).not.toHaveProperty("full");
  });

  it("replaces the cold parts when a full frame brings new ones", () => {
    const first = mergeAppState(null, full(snapshot(3, "Brisk")))!;
    const next = mergeAppState(first, full(snapshot(3, "Renamed")))!;
    expect(next.programs[0].name).toBe("Renamed");
  });

  it("drops a hot frame before there is a snapshot to merge it into", () => {
    expect(mergeAppState(null, hot(snapshot(3, "Brisk")))).toBeNull();
  });
});

describe("useAppState with frames", () => {
  let handler: EventCallback<AppStateFrame> | null = null;

  beforeEach(() => {
    vi.resetModules();
    handler = null;
    vi.mocked(listen).mockImplementation((_event, cb) => {
      handler = cb as EventCallback<AppStateFrame>;
      return Promise.resolve(() => {});
    });
    vi.mocked(invoke).mockImplementation(() => Promise.resolve(snapshot(1, "Fetched")));
  });

  function send(frame: AppStateFrame) {
    handler!({ event: "app-state", id: 1, payload: frame });
  }

  it("listens for this window's frames only", async () => {
    const state = await import("./useAppState");
    state.subscribeAppSnapshot(() => {})();
    await Promise.resolve();
    expect(vi.mocked(listen)).toHaveBeenCalledWith("app-state", expect.any(Function), {
      target: { kind: "WebviewWindow", label: "main" },
    });
  });

  it("merges hot frames onto the fetched snapshot", async () => {
    const state = await import("./useAppState");
    const seen: AppSnapshot[] = [];
    state.subscribeAppSnapshot((s) => seen.push(s));
    await vi.waitFor(() => expect(state.getAppSnapshot()).not.toBeNull());
    const fetched = state.getAppSnapshot()!;
    send(hot(snapshot(5, "ignored")));
    const now = state.getAppSnapshot()!;
    expect(now.deck.speed_kmh).toBe(5);
    expect(now.programs).toBe(fetched.programs);
    send(full(snapshot(5, "Saved")));
    expect(state.getAppSnapshot()!.programs[0].name).toBe("Saved");
    expect(seen.length).toBe(3);
  });
});
