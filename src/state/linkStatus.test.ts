import { describe, expect, it } from "vitest";
import type { ConnectionSnapshot, LinkState } from "../ipc";
import { mmss } from "../format";
import { linkView } from "./linkStatus";

const conn = (link: LinkState, connected = false): ConnectionSnapshot => ({
  connected,
  connecting: link.state === "connecting",
  device_id: null,
  has_saved_device: true,
  link,
  last_error: null,
  start_pending: false,
  start_result: null,
  connect_dialog_at_ms: null,
});

describe("linkView", () => {
  it("labels every state for the rail and the HUD", () => {
    const cases: [LinkState, string, string][] = [
      [{ state: "idle" }, "OFFLINE", "LINK DOWN"],
      [{ state: "scanning" }, "SCANNING", "SCANNING"],
      [{ state: "connecting", attempt: 2, of: 3 }, "LINKING 2/3", "LINKING 2/3"],
      [{ state: "reconnecting", attempt: 1, next_retry_s: 12 }, "RECONNECT 12s", "RELINK 12S"],
      [{ state: "adapter_unavailable" }, "BT OFF", "BT OFFLINE"],
      [{ state: "failed", message: "Treadmill not found. Is it switched on?" }, "LINK FAILED", "LINK FAILED"],
      [{ state: "no_data", since_s: 14 }, "NO DATA", "NO SIGNAL"],
      [{ state: "connected" }, "LINKED", "LINKED"],
    ];
    for (const [link, rail, hud] of cases) {
      const v = linkView(conn(link, link.state === "connected" || link.state === "no_data"));
      expect(v.label).toBe(rail);
      expect(v.hudLabel).toBe(hud);
      expect(v.hudLabel.length).toBeLessThanOrEqual(16);
    }
  });

  it("offers Retry now only while waiting to retry", () => {
    expect(linkView(conn({ state: "reconnecting", attempt: 1, next_retry_s: 3 })).canRetry).toBe(true);
    expect(linkView(conn({ state: "connected" }, true)).canRetry).toBe(false);
  });

  it("puts the last error in the tooltip", () => {
    const c = conn({ state: "reconnecting", attempt: 2, next_retry_s: 9 });
    c.last_error = { message: "Treadmill not found. Is it switched on?", at_ms: 1 };
    expect(linkView(c).title).toContain("Treadmill not found");
  });
});

describe("mmss", () => {
  it("formats countdowns", () => {
    expect(mmss(252)).toBe("4:12");
    expect(mmss(5)).toBe("0:05");
    expect(mmss(-3)).toBe("0:00");
  });
});
