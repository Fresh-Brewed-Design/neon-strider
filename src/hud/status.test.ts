import { describe, expect, it } from "vitest";
import { snapshot as previewSnapshot } from "../dev/mockBackend";
import type { AppSnapshot } from "../ipc";
import { startFailure } from "../state/linkStatus";
import { linkLabel, relinkKey, relinkView, stasisClockOf } from "./status";

function snap(patch: { deck?: Partial<AppSnapshot["deck"]>; connection?: Partial<AppSnapshot["connection"]> } = {}) {
  const base: AppSnapshot = structuredClone(previewSnapshot);
  return {
    ...base,
    deck: { ...base.deck, ...patch.deck },
    connection: { ...base.connection, ...patch.connection },
  };
}

describe("linkLabel", () => {
  it("names the deck state while linked", () => {
    expect(linkLabel(snap({ deck: { status: "Running" } }))).toEqual({ text: "RUNNING", tone: "live" });
    expect(linkLabel(snap({ deck: { status: "Paused" } }))).toEqual({ text: "STASIS", tone: "idle" });
    expect(linkLabel(snap({ deck: { status: "Stopped" } }))).toEqual({ text: "STANDBY", tone: "idle" });
    expect(linkLabel(snap({ deck: { status: "Starting" } }))).toEqual({ text: "SPIN-UP", tone: "live" });
    expect(linkLabel(snap({ deck: { status: "Stopping" } }))).toEqual({ text: "SPIN-DOWN", tone: "idle" });
  });

  it("keeps the header's word bare while the STASIS clock counts", () => {
    expect(linkLabel(snap({ deck: { status: "Paused", paused_for_s: 3909 } }))).toEqual({ text: "STASIS", tone: "idle" });
  });

  it("does not claim STANDBY for a status byte it doesn't know", () => {
    expect(linkLabel(snap({ deck: { status: "Stopped", unknown_status_code: 9 } }))).toEqual({
      text: "STATE ?9",
      tone: "warn",
    });
  });

  it("says the deck is off while it is in standby", () => {
    expect(linkLabel(snap({ deck: { status: "Stopped", standby: true } }))).toEqual({ text: "DECK OFF", tone: "warn" });
  });

  it("shows the link problem when offline", () => {
    const offline = snap({ connection: { connected: false, link: { state: "idle" } } });
    expect(linkLabel(offline)).toEqual({ text: "NO_CARRIER", tone: "down" });
  });
});

describe("startFailure", () => {
  const failed = { ok: false, message: "No answer", at_ms: 1 };

  it("reports an unanswered Engage while the belt is still", () => {
    expect(startFailure(snap({ deck: { status: "Stopped" }, connection: { start_result: failed } }))).toBe("No answer");
  });

  it("drops it once the belt moves", () => {
    expect(startFailure(snap({ deck: { status: "Running" }, connection: { start_result: failed } }))).toBeNull();
  });
});

describe("relinkView", () => {
  const down = (link: AppSnapshot["connection"]["link"], has_saved_device = true) =>
    snap({ connection: { connected: false, connecting: link.state === "connecting", link, has_saved_device } }).connection;
  const reconnecting = down({ state: "reconnecting", attempt: 2, next_retry_s: 9 });

  it("is clickable while the link is down and a treadmill is saved", () => {
    for (const link of [
      { state: "reconnecting", attempt: 2, next_retry_s: 9 },
      { state: "adapter_unavailable" },
      { state: "failed", message: "Treadmill not found." },
      { state: "idle" },
    ] as const) {
      const v = relinkView(down(link), false);
      expect(v.clickable, link.state).toBe(true);
      expect(v.text).toBeNull();
      expect(v.title).toMatch(/Click to reconnect now\.$/);
    }
  });

  it("keeps the watchdog's countdown and last error in the tooltip", () => {
    const c = { ...reconnecting, last_error: { message: "Treadmill not found.", at_ms: 1 } };
    expect(relinkView(c, false).title).toBe("Trying again in 9 s. Treadmill not found. Click to reconnect now.");
    const asked = { ...reconnecting, last_error: { message: "Is it switched on?", at_ms: 1 } };
    expect(relinkView(asked, false).title).toBe("Trying again in 9 s. Is it switched on? Click to reconnect now.");
    expect(relinkView(down({ state: "idle" }), false).title).toBe("Find and connect a treadmill. Click to reconnect now.");
    // Not two "Click to" hints.
    expect(relinkView(down({ state: "failed", message: "Treadmill not found." }), false).title).toBe(
      "Treadmill not found. Click to reconnect now.",
    );
  });

  it("is inert while connected, scanning or already linking", () => {
    expect(relinkView(snap().connection, false)).toEqual({
      clickable: false,
      opensConnectDialog: false,
      text: null,
      title: null,
    });
    const noData = snap({ connection: { connected: true, link: { state: "no_data", since_s: 14 } } }).connection;
    expect(relinkView(noData, false).clickable).toBe(false);
    expect(relinkView(down({ state: "scanning" }), false).clickable).toBe(false);
    expect(relinkView(down({ state: "connecting", attempt: 1, of: 3 }), false).clickable).toBe(false);
  });

  it("opens the connect dialog with no treadmill saved", () => {
    for (const link of [{ state: "idle" }, { state: "adapter_unavailable" }] as const) {
      expect(relinkView(down(link, false), false), link.state).toEqual({
        clickable: true,
        opensConnectDialog: true,
        text: null,
        title: "No treadmill saved yet. Click to pick one.",
      });
    }
    // Not while scanning or linking.
    expect(relinkView(down({ state: "scanning" }, false), false).clickable).toBe(false);
  });

  it("reads LINKING… while a click waits, and can't be clicked twice", () => {
    expect(relinkView(reconnecting, true)).toEqual({
      clickable: false,
      opensConnectDialog: false,
      text: "LINKING…",
      title: "Trying the saved treadmill now…",
    });
  });

  it("ignores the countdown ticking when deciding the state moved", () => {
    const later = down({ state: "reconnecting", attempt: 2, next_retry_s: 8 });
    expect(relinkKey(later)).toBe(relinkKey(reconnecting));
    expect(relinkKey(down({ state: "connecting", attempt: 1, of: 3 }))).not.toBe(relinkKey(reconnecting));
  });
});

describe("stasisClockOf", () => {
  it("counts the pause up, m:ss then hours and minutes", () => {
    const paused = (paused_for_s: number) => stasisClockOf(snap({ deck: { status: "Paused", paused_for_s } }));
    expect(paused(0)).toBe("0:00");
    expect(paused(200)).toBe("3:20");
    expect(paused(3599)).toBe("59:59");
    expect(paused(3909)).toBe("1h05");
  });

  it("shows nothing unless the linked belt is paused", () => {
    // A clock left over from a pause never shows once the belt moves.
    expect(stasisClockOf(snap({ deck: { status: "Running", paused_for_s: 12 } }))).toBeNull();
    expect(stasisClockOf(snap({ deck: { status: "Paused", paused_for_s: null } }))).toBeNull();
    expect(stasisClockOf(snap({ deck: { status: "Paused", paused_for_s: 12 }, connection: { connected: false } }))).toBeNull();
  });
});
