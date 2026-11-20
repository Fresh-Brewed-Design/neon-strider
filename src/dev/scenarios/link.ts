// The treadmill link. ?offline: no treadmill saved yet (the HUD's link label is
// not clickable). ?sim: the dev builds' simulated treadmill (the SIM tag).
// ?link=scanning|connecting|reconnecting|nodata|bt-off|failed previews each
// treadmill link state; countdowns tick down the way Rust's do.
// ?scan=empty|error|bt-off and ?connect=fail shape the connect dialog.
import type { ConnectionSnapshot } from "../../ipc";
import { later, params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const LAST_ERROR = { message: "Treadmill not found. Is it switched on?", at_ms: Date.now() - 4000 };
const LINK_SCENARIOS: Record<string, Partial<ConnectionSnapshot>> = {
  scanning: { link: { state: "scanning" } },
  connecting: { link: { state: "connecting", attempt: 2, of: 3 } },
  reconnecting: { link: { state: "reconnecting", attempt: 3, next_retry_s: 12 }, last_error: LAST_ERROR },
  nodata: { link: { state: "no_data", since_s: 14 } },
  "bt-off": {
    link: { state: "adapter_unavailable" },
    last_error: { message: "Bluetooth is off. Turn it on in Windows settings.", at_ms: Date.now() },
  },
  failed: { link: { state: "failed", message: LAST_ERROR.message }, last_error: LAST_ERROR },
};
const scanFlag = params.get("scan");
const connectFails = params.get("connect") === "fail";

/** Resolve after a moment, the way a Bluetooth call takes a while. */
const slow = <T,>(fn: () => T) => new Promise((resolve) => setTimeout(resolve, 1500)).then(fn);

export const link: Scenario = {
  apply() {
    if (params.has("offline")) Object.assign(snapshot.connection, { connected: false, has_saved_device: false, link: { state: "idle" } });
    if (params.has("sim")) Object.assign(snapshot.connection, { simulated: true, device_id: "SIM_DECK" });
    const linkFlag = params.get("link");
    if (linkFlag) {
      const flags = { connected: linkFlag === "nodata", connecting: linkFlag === "connecting" };
      Object.assign(snapshot.connection, flags, LINK_SCENARIOS[linkFlag]);
    }
  },
  /** The link's countdowns, once a second like the Rust ones. */
  tick() {
    const c = snapshot.connection;
    if (c.link.state === "reconnecting") {
      c.link = { ...c.link, next_retry_s: c.link.next_retry_s > 1 ? c.link.next_retry_s - 1 : 15 };
      return true;
    } else if (c.link.state === "no_data") {
      c.link = { state: "no_data", since_s: c.link.since_s + 1 };
      return true;
    }
    return false;
  },
  commands: {
    connect_now: () => {
      Object.assign(snapshot.connection, { link: { state: "connecting", attempt: 1, of: 3 }, connecting: true });
      later(1500, () =>
        Object.assign(snapshot.connection, { connected: true, connecting: false, link: { state: "connected" }, last_error: null }),
      );
    },
    open_connect_dialog: () => (snapshot.connection.connect_dialog_at_ms = Date.now()),
  },
  queries: {
    scan_bluetooth_devices: () => {
      if (scanFlag === "error") throw { kind: "generic", message: "Could not reach the treadmill. Try again." };
      if (scanFlag === "bt-off") throw { kind: "adapter_off", message: "Bluetooth is off. Turn it on in Windows settings." };
      return slow(() => (scanFlag === "empty" ? [] : [{ id: "preview-deck", name: "FS-Preview" }]));
    },
    connect_bluetooth_device: () =>
      slow(() => {
        if (connectFails) throw { kind: "not_found", message: "Treadmill not found. Is it switched on?" };
        return null;
      }),
  },
};
