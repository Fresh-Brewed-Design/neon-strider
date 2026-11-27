import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

let current: AppSnapshot | null = null;
vi.mock("../state/useAppState", () => ({ useAppSnapshot: () => current }));

const { snapshot: mock } = await import("../dev/mockBackend");
const { HudApp } = await import("./HudApp");
const { engageLabel } = await import("./status");

beforeAll(() => {
  // jsdom has no ResizeObserver; the hit region only needs it to exist.
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

afterEach(() => {
  current = null;
});

/** The mock's snapshot, with the belt stopped and no program, then `patch`. */
function still(view: AppSnapshot["hud_view"], patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.deck = { ...s.deck, status: "Stopped", speed_kmh: 0 };
  s.active_program = null;
  s.hud = { ...s.hud, idle_mode: view === "strip" || view === "hidden" ? view : "full" };
  s.hud_view = view;
  patch(s);
  return s;
}

function show(s: AppSnapshot) {
  current = s;
  return render(<HudApp />);
}

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

describe("HUD strip", () => {
  it("shows the link state, today's total, expand and Engage, and nothing else", () => {
    const { container } = show(still("strip"));
    expect(screen.getByText("NEON STRIDER")).toBeInTheDocument();
    expect(screen.getByText("STANDBY")).toBeInTheDocument();
    expect(screen.getByText("TODAY")).toBeInTheDocument();
    expect(screen.getByText("4.31")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show the whole HUD" })).toHaveAttribute("title");
    expect(screen.getByRole("button", { name: "ENGAGE start" })).toBeEnabled();
    // The full panel's stats and controls are gone.
    expect(screen.queryByText("DUMP")).not.toBeInTheDocument();
    expect(container.querySelector(".hud-stats")).toBeNull();
  });

  it("shows today's walking time beside TODAY, and none before the first walk", () => {
    show(still("strip", (s) => (s.session.daily_time_s = 8_040)));
    expect(screen.getByTestId("hud-strip-time")).toHaveTextContent("2h 14m");
    expect(screen.getByTestId("hud-strip-time").closest(".hud-strip-total")).toHaveTextContent("TODAY");
  });

  it("leaves the time out before the first walk of the day", () => {
    show(still("strip", (s) => (s.session.daily_time_s = 0)));
    expect(screen.getByText("TODAY")).toBeInTheDocument();
    expect(screen.queryByTestId("hud-strip-time")).toBeNull();
  });

  it("shows a program's rest and its countdown in the total's place, Engage where it was", () => {
    const { container } = show(
      still("strip", (s) => {
        s.active_program = { ...structuredClone(mock.active_program!), name: "Workday", phase: "resting", seconds_remaining: 1294, paused: false, wait: null };
      }),
    );
    const rest = screen.getByTestId("hud-strip-rest");
    expect(rest).toHaveTextContent("Workday // REST");
    expect(rest).toHaveTextContent("21:34");
    expect(rest).toHaveAttribute("title", "Workday · Rest · next walk in 21:34");
    expect(screen.queryByText("TODAY")).not.toBeInTheDocument();
    // The rest takes the total's cell, so the row keeps its shape and Engage its slot.
    const row = container.querySelector(".hud-strip-row")!;
    expect(Array.from(row.children).map((c) => c.className)).toEqual(["hud-strip-cell", "hud-btn hud-btn-main hud-btn-stack accent-cyan"]);
    expect(screen.getByRole("button", { name: "ENGAGE start" })).toBeEnabled();
  });

  it("puts a program's rest before a folded ATROPHY_WARNING, and that before today's total", () => {
    const rest = (s: AppSnapshot) =>
      (s.active_program = { ...structuredClone(mock.active_program!), name: "Workday", phase: "resting", seconds_remaining: 1294, paused: false, wait: null });
    const { unmount } = show(
      still("strip", (s) => {
        rest(s);
        s.seated.folded = true;
      }),
    );
    expect(screen.getByTestId("hud-strip-rest")).toBeInTheDocument();
    expect(screen.queryByTestId("hud-seated-readout")).not.toBeInTheDocument();
    unmount();
    show(still("strip", (s) => (s.seated.folded = true)));
    expect(screen.getByTestId("hud-seated-readout")).toBeInTheDocument();
    expect(screen.queryByText("TODAY")).not.toBeInTheDocument();
  });

  it("expand and Engage each send one command", async () => {
    vi.mocked(invoke).mockClear();
    show(still("strip"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Show the whole HUD" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ENGAGE start" })));
    expect(dispatched()).toEqual([{ command: { type: "hud_expand" } }, { command: { type: "toggle_start" } }]);
  });

  it("labels Engage like the panel does", () => {
    show(still("strip", (s) => (s.deck.status = "Paused")));
    expect(screen.getByRole("button", { name: "RE-JACK resume belt" })).toBeInTheDocument();
  });

  it("reads WAKING… while a start is pending, disabled when offline", () => {
    const { unmount } = show(still("strip", (s) => (s.connection.start_pending = true)));
    expect(screen.getByRole("button", { name: "WAKING… starting" })).toBeInTheDocument();
    unmount();
    show(
      still("strip", (s) =>
        Object.assign(s.connection, { connected: false, has_saved_device: true, link: { state: "reconnecting", attempt: 1, next_retry_s: 9 } }),
      ),
    );
    expect(screen.getByRole("button", { name: "ENGAGE start" })).toBeDisabled();
    // Offline, the link status is still the relink button.
    expect(screen.getByRole("button", { name: "RELINK 9S" })).toBeInTheDocument();
  });

  it("shows a rejected Engage right under the strip", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    show(still("strip"));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "ENGAGE start" })));
    expect(screen.getByRole("alert")).toHaveTextContent("SYS // TREADMILL NOT CONNECTED");
  });

  it("shows the last Engage's failure", () => {
    show(
      still("strip", (s) => (s.connection.start_result = { ok: false, message: "The treadmill didn't respond.", at_ms: 1 })),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("The treadmill didn't respond.");
  });

  // #403: the sit-too-long card's own ENGAGE shows its failure on the card, once.
  it("leaves the card's own failed ENGAGE to the card while the card is up", () => {
    const NO_ANSWER = "The treadmill didn't respond.";
    const own = (card: AppSnapshot["hud_card"]) =>
      still("strip", (s) => {
        s.hud_card = card;
        s.seated = { ...s.seated, show_card: card === "seated", seated_minutes: 63 };
        s.connection.start_press = "seated_engage";
        s.connection.start_result = { ok: false, message: NO_ANSWER, at_ms: 1 };
      });
    const { unmount } = show(own("seated"));
    expect(screen.getAllByText(NO_ANSWER)).toHaveLength(1);
    expect(screen.getByText(NO_ANSWER).closest(".hud-seated")).not.toBeNull();
    unmount();
    // The card gone (snoozed or folded): the panel says it.
    show(own("none"));
    expect(screen.getAllByText(NO_ANSWER)).toHaveLength(1);
    expect(screen.getByText(NO_ANSWER).closest(".hud-seated")).toBeNull();
  });
});

describe("engageLabel", () => {
  it("covers every state of the main button", () => {
    expect(engageLabel(still("full"))).toBe("ENGAGE");
    expect(engageLabel(still("full", (s) => (s.deck.status = "Paused")))).toBe("RE-JACK");
    expect(engageLabel(still("full", (s) => (s.connection.start_pending = true)))).toBe("WAKING…");
    expect(engageLabel(still("full", (s) => (s.deck.status = "Running")))).toBe("STASIS");
    const go = still("full", (s) => {
      s.deck.status = "Paused";
      s.active_program = { ...structuredClone(mock.active_program!), paused: true };
    });
    expect(engageLabel(go)).toBe("GO");
  });
});

describe("HUD hidden", () => {
  it("renders nothing when there is nothing else to show", () => {
    const { container } = show(still("hidden"));
    expect(container).toBeEmptyDOMElement();
  });

  it("still shows a card that needs an answer, without the panel", () => {
    const { container } = show(
      still("hidden", (s) => {
        Object.assign(s.walkaway, { card: "paused", reason: "lock" });
        s.hud_card = "walk_away";
      }),
    );
    expect(container.querySelector(".hud-panel")).toBeNull();
    expect(container.querySelector(".hud-walkaway")).not.toBeNull();
    expect(screen.queryByText("NEON STRIDER")).not.toBeInTheDocument();
  });
});

describe("HUD full", () => {
  it("is the whole panel, as before", () => {
    const { container } = show(still("full"));
    expect(container.querySelector(".hud-stats")).not.toBeNull();
    expect(screen.getByText("DUMP")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Show the whole HUD" })).not.toBeInTheDocument();
  });
});

/** The mock's snapshot with the belt walking and the HUD minified, then `patch`. */
function minified(patch: (s: AppSnapshot) => void = () => {}): AppSnapshot {
  const s = structuredClone(mock);
  s.hud = { ...s.hud, minified: true };
  s.hud_view = "minified";
  patch(s);
  return s;
}

describe("HUD minified", () => {
  it("shows speed, time, distance, the program step, expand, Stasis and Dump", () => {
    const { container } = show(minified());
    expect(screen.getByText("VEL")).toBeInTheDocument();
    expect(screen.getByText("TIME")).toBeInTheDocument();
    expect(screen.getByText("DIST")).toBeInTheDocument();
    expect(screen.getByText("PROGRAM")).toBeInTheDocument();
    expect(container.querySelector(".hud-run-segments")?.textContent).toBe("Workday · WALK · 04:12 left · cycle 3");
    expect(container.querySelectorAll(".hud-run-seg")).toHaveLength(4);
    expect(screen.getByRole("button", { name: "Show the whole HUD" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "STASIS pause belt" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "DUMP end walk" })).toBeEnabled();
    // The whole panel's stats, steppers and program buttons are gone, and so is minify.
    expect(container.querySelector(".hud-stats")).toBeNull();
    expect(screen.queryByText("TILT")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Minify the HUD" })).not.toBeInTheDocument();
  });

  it("hides the dot of a program segment that ends a line", () => {
    // jsdom has no layout: put the last two segments on a second row.
    const top = vi.spyOn(HTMLElement.prototype, "offsetTop", "get").mockImplementation(function (this: HTMLElement) {
      return this.textContent?.startsWith("04:12") || this.textContent?.startsWith("cycle") ? 20 : 0;
    });
    try {
      const { container } = show(minified());
      const segs = [...container.querySelectorAll(".hud-run-seg")];
      expect(segs.map((e) => e.hasAttribute("data-wrapped"))).toEqual([false, true, false, false]);
    } finally {
      top.mockRestore();
    }
  });

  it("leaves the program line out when no program runs", () => {
    show(minified((s) => (s.active_program = null)));
    expect(screen.queryByText("PROGRAM")).not.toBeInTheDocument();
    expect(screen.getByText("VEL")).toBeInTheDocument();
  });

  it("leaves heart rate off the strip without a sensor", () => {
    const { container } = show(minified());
    expect(screen.queryByText("HR")).not.toBeInTheDocument();
    expect(container.querySelector("[data-testid=hud-run-pulse]")).toBeNull();
    expect(container.querySelector("[data-testid=sparkline]")).toBeNull();
  });

  it("adds PULSE_RATE with its spark chart once a sensor is picked", () => {
    const { container } = show(
      minified((s) =>
        Object.assign(s.heart_rate, {
          sensor: { id: "x", name: "Watch" },
          status: "connected",
          bpm: 128,
          history: [120, null, 124, 128],
        }),
      ),
    );
    const row = container.querySelector("[data-testid=hud-run-pulse]");
    expect(row).toHaveTextContent("HR128BPM");
    expect(row?.querySelector("[data-testid=sparkline]")).not.toBeNull();
    // Speed, time and distance stay on their own line.
    expect(container.querySelectorAll(".hud-run-stats .hud-strip-total")).toHaveLength(3);
  });

  it("shows -- and no chart while a picked sensor is not sending yet", () => {
    const { container } = show(
      minified((s) => Object.assign(s.heart_rate, { sensor: { id: "x", name: "Watch" }, status: "waiting", bpm: null, history: [] })),
    );
    expect(container.querySelector("[data-testid=hud-run-pulse]")).toHaveTextContent("HR--BPM");
    expect(container.querySelector("[data-testid=sparkline]")).toBeNull();
  });

  it("expand, Stasis and Dump each send one command", async () => {
    vi.mocked(invoke).mockClear();
    show(minified());
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Show the whole HUD" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "STASIS pause belt" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "DUMP end walk" })));
    expect(dispatched()).toEqual([
      { command: { type: "hud_set_minified", minified: false } },
      { command: { type: "toggle_start" } },
      { command: { type: "stop" } },
    ]);
  });

  it("shows a rejected press right under the strip", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    vi.spyOn(console, "error").mockImplementationOnce(() => {});
    const { container } = show(minified());
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "DUMP end walk" })));
    expect(container.querySelector(".hud-run-strip [role=alert]")).toHaveTextContent("SYS // TREADMILL NOT CONNECTED");
  });

  it("still shows a card that needs an answer", () => {
    const { container } = show(
      minified((s) => {
        Object.assign(s.walkaway, { card: "warning", pause_in_s: 20, reason: "idle" });
        s.hud_card = "walk_away";
      }),
    );
    expect(container.querySelector(".hud-run-strip")).not.toBeNull();
    expect(container.querySelector(".hud-walkaway")).not.toBeNull();
  });

  it("the whole panel has a minify button that sends one command", async () => {
    vi.mocked(invoke).mockClear();
    show(still("full", (s) => (s.deck.status = "Running")));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Minify the HUD" })));
    expect(dispatched()).toEqual([{ command: { type: "hud_set_minified", minified: true } }]);
  });

  it("keeps the minify button in its place but off for the first 30 s a program waits for GO (the panel stays whole)", () => {
    show(
      still("full", (s) => {
        s.deck.status = "Paused";
        s.active_program = { ...structuredClone(mock.active_program!), paused: true, wait: "rest_complete" };
        s.hud_go_hold = true;
      }),
    );
    const minify = screen.getByRole("button", { name: "Minify the HUD" });
    expect(minify).toBeDisabled();
    expect(minify).toHaveAttribute("title", "The whole HUD stays up for 30 seconds while a program waits for GO.");
  });

  it("lets minify work again once an unanswered GO's hold is over", () => {
    show(
      still("full", (s) => {
        s.deck.status = "Paused";
        s.active_program = { ...structuredClone(mock.active_program!), paused: true, wait: "rest_complete" };
        s.hud_go_hold = false;
      }),
    );
    expect(screen.getByRole("button", { name: "Minify the HUD" })).toBeEnabled();
  });

  it("falls back to the strip with a yellow GO as its main button after the hold", async () => {
    vi.mocked(invoke).mockClear();
    show(
      still("strip", (s) => {
        s.deck.status = "Paused";
        s.active_program = { ...structuredClone(mock.active_program!), paused: true, wait: "rest_complete" };
      }),
    );
    const go = screen.getByRole("button", { name: "GO start belt" });
    expect(go).toHaveClass("accent-yellow");
    expect(go).toHaveAttribute("title", "Start the belt and carry on with the program.");
    // The same one press as before: nothing starts the belt but GO.
    await act(async () => fireEvent.click(go));
    expect(dispatched()).toEqual([{ command: { type: "toggle_start" } }]);
  });
});

/** Oscillators the HUD's tones started, by pitch (Hz). jsdom has no AudioContext. */
const tones: number[] = [];
class FakeAudioContext {
  currentTime = 0;
  destination = {};
  resume() {
    return Promise.resolve();
  }
  createOscillator() {
    const osc = { frequency: { value: 0 }, connect: (n: unknown) => n, start: () => tones.push(osc.frequency.value), stop() {} };
    return osc;
  }
  createGain() {
    return { gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} }, connect: (n: unknown) => n };
  }
}

/** Nothing running: no treadmill linked, the belt still, no program, the HUD hidden while still. */
function nothingRunning(eye: Partial<AppSnapshot["eye_break"]>): AppSnapshot {
  return still("hidden", (s) => {
    Object.assign(s.connection, { connected: false, has_saved_device: true, link: { state: "idle" } });
    s.eye_break = { ...s.eye_break, enabled: true, ...eye };
    s.hud_card = "eye_break";
  });
}

// Fresh Brewed Design, 2026-11-19: eye breaks show even when nothing is running.
describe("HUD eye breaks with nothing running", () => {
  beforeAll(() => vi.stubGlobal("AudioContext", FakeAudioContext));

  it("shows the heads-up, the cooldown card and the resume card without the panel", () => {
    const { container, rerender } = show(nothingRunning({ phase: "upcoming", remaining_s: 48 }));
    expect(container.querySelector(".hud-panel")).toBeNull();
    expect(screen.getByText("IMPLANT COOLDOWN IN 48 S")).toBeInTheDocument();
    current = nothingRunning({ phase: "break", remaining_s: 72 });
    rerender(<HudApp />);
    expect(screen.getByText("OCULAR_IMPLANT // COOLDOWN")).toBeInTheDocument();
    expect(screen.getByText("Look away from the screen.")).toBeInTheDocument();
    expect(container.querySelector(".hud-panel")).toBeNull();
  });

  it("plays the blink and end tones", () => {
    tones.length = 0;
    const { rerender } = show(nothingRunning({ phase: "upcoming", remaining_s: 5, blink_seq: 0 }));
    expect(tones).toEqual([]);
    current = nothingRunning({ phase: "break", remaining_s: 90, blink_seq: 1 });
    rerender(<HudApp />);
    expect(tones).toEqual([660]);
    current = nothingRunning({ phase: "break", remaining_s: 80, blink_seq: 2 });
    rerender(<HudApp />);
    expect(tones).toEqual([660, 660]);
    current = nothingRunning({ phase: "idle", remaining_s: 0, blink_seq: 2, end_seq: 1 });
    rerender(<HudApp />);
    expect(tones).toEqual([660, 660, 660, 880]);
  });
});
