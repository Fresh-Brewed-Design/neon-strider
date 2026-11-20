import { renderHook } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { EyeBreakSnapshot } from "../ipc";

/** Every pitch the fake audio context was asked to play, in order. */
const played: number[] = [];

class FakeParam {
  value = 0;
  setValueAtTime() {}
  exponentialRampToValueAtTime() {}
}
class FakeNode {
  frequency = new FakeParam();
  gain = new FakeParam();
  connect(next: FakeNode) {
    return next;
  }
  start() {
    played.push(this.frequency.value);
  }
  stop() {}
}
class FakeAudioContext {
  currentTime = 0;
  destination = new FakeNode();
  resume() {
    return Promise.resolve();
  }
  createOscillator() {
    return new FakeNode();
  }
  createGain() {
    return new FakeNode();
  }
}

beforeAll(() => {
  (globalThis as { AudioContext?: unknown }).AudioContext = FakeAudioContext;
});
beforeEach(() => {
  played.length = 0;
});

const { snapshot: mock } = await import("../dev/mockBackend");
const { useEyeBreakTones, CLOSE_TONE, SQUEEZE_TONE, OPEN_TONE, END_TONE } = await import("./eyeBreakTones");

function eye(patch: Partial<EyeBreakSnapshot>): EyeBreakSnapshot {
  return { ...mock.eye_break, enabled: true, ...patch };
}

describe("useEyeBreakTones", () => {
  it("plays each guided blink step's tone, then the end tone", () => {
    const { rerender } = renderHook((e: EyeBreakSnapshot) => useEyeBreakTones(e), {
      initialProps: eye({ phase: "upcoming", blink_seq: 0 }),
    });
    expect(played).toEqual([]);
    rerender(eye({ phase: "break", blink: "close", blink_seq: 1 }));
    rerender(eye({ phase: "break", blink: "squeeze", blink_seq: 2 }));
    rerender(eye({ phase: "break", blink: "open", blink_seq: 3 }));
    rerender(eye({ phase: "idle", blink: null, blink_seq: 3, end_seq: 1 }));
    expect(played).toEqual([...CLOSE_TONE, ...SQUEEZE_TONE, ...OPEN_TONE, ...END_TONE]);
  });

  it("stays quiet with Tones off and for an old break when the HUD opens", () => {
    const { rerender } = renderHook((e: EyeBreakSnapshot) => useEyeBreakTones(e), {
      initialProps: eye({ phase: "break", blink: "close", blink_seq: 7 }),
    });
    rerender(eye({ phase: "break", blink: "squeeze", blink_seq: 8, sound: false }));
    expect(played).toEqual([]);
  });

  it("tells the three steps and the end apart", () => {
    const tones = [CLOSE_TONE, SQUEEZE_TONE, OPEN_TONE, END_TONE].map((t) => t.join(","));
    expect(new Set(tones).size).toBe(4);
  });
});
