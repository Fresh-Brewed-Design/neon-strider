import { render, renderHook } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, ProgramTonesSnapshot } from "../ipc";

/** Every pitch the fake audio context was asked to play, in order. jsdom has no AudioContext. */
const played: number[] = [];
class FakeAudioContext {
  currentTime = 0;
  destination = {};
  resume() {
    return Promise.resolve();
  }
  createOscillator() {
    const osc = { frequency: { value: 0 }, connect: (n: unknown) => n, start: () => played.push(osc.frequency.value), stop() {} };
    return osc;
  }
  createGain() {
    return { gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} }, connect: (n: unknown) => n };
  }
}

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

beforeAll(() => {
  vi.stubGlobal("AudioContext", FakeAudioContext);
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});
beforeEach(() => {
  played.length = 0;
});

const { snapshot: mock } = await import("../dev/mockBackend");
const { programTones: scenario } = await import("../dev/scenarios/programtones");
const { useProgramTones, STEP_CUE, WAIT_CUE, SPEED_UP_CUE, CUE_FRESH_MS } = await import("./programTones");
const { CLOSE_TONE, SQUEEZE_TONE, OPEN_TONE, END_TONE } = await import("./eyeBreakTones");
const { HudApp } = await import("./HudApp");

const NOW = 1_800_000_000_000;
const now = () => NOW;

function tones(patch: Partial<ProgramTonesSnapshot>): ProgramTonesSnapshot {
  return { enabled: true, seq: 0, cue: null, at_ms: 0, ...patch };
}

describe("useProgramTones", () => {
  it("plays each cue once when Rust's counter moves", () => {
    const { rerender } = renderHook((t: ProgramTonesSnapshot) => useProgramTones(t, now), { initialProps: tones({}) });
    expect(played).toEqual([]);
    rerender(tones({ seq: 1, cue: "step", at_ms: NOW }));
    rerender(tones({ seq: 1, cue: "step", at_ms: NOW }));
    rerender(tones({ seq: 2, cue: "speed_up", at_ms: NOW }));
    rerender(tones({ seq: 3, cue: "wait", at_ms: NOW }));
    expect(played).toEqual([...STEP_CUE, ...SPEED_UP_CUE, ...WAIT_CUE]);
    expect(played).toEqual([880, 440, 660, 880, 880, 440, 880, 440]);
  });

  it("never plays an old cue when the HUD opens or reloads", () => {
    const { rerender } = renderHook((t: ProgramTonesSnapshot) => useProgramTones(t, now), {
      initialProps: tones({ seq: 4, cue: "wait", at_ms: NOW }),
    });
    // A window that gets a cue late (shown again after it fired) skips it too.
    rerender(tones({ seq: 5, cue: "step", at_ms: NOW - CUE_FRESH_MS - 1 }));
    expect(played).toEqual([]);
  });

  it("stays quiet with Program tones off", () => {
    const { rerender } = renderHook((t: ProgramTonesSnapshot) => useProgramTones(t, now), {
      initialProps: tones({ enabled: false }),
    });
    rerender(tones({ enabled: false, seq: 1, cue: "step", at_ms: NOW }));
    expect(played).toEqual([]);
  });

  it("sounds unlike every eye-break tone", () => {
    const all = [STEP_CUE, WAIT_CUE, SPEED_UP_CUE, CLOSE_TONE, SQUEEZE_TONE, OPEN_TONE, END_TONE].map((t) => t.join(","));
    expect(new Set(all).size).toBe(all.length);
  });
});

describe("HUD program tones", () => {
  it("plays the step cue when the preview's SKIP moves a running program", () => {
    const s = structuredClone(mock);
    s.deck = { ...s.deck, status: "Running" };
    s.program_tones = { ...s.program_tones, enabled: true };
    current = s;
    const { rerender } = render(<HudApp />);
    expect(played).toEqual([]);

    // The mocked backend's SKIP, as `?programtones` wires it.
    Object.assign(mock.deck, { status: "Running" });
    mock.program_tones.enabled = true;
    const before = mock.program_tones.seq;
    scenario.commands!.program_skip!({ type: "program_skip" });
    expect(mock.program_tones.seq).toBe(before + 1);
    expect(mock.program_tones.cue).toBe("step");

    current = { ...structuredClone(s), program_tones: structuredClone(mock.program_tones) };
    rerender(<HudApp />);
    expect(played).toEqual(STEP_CUE);
  });
});
